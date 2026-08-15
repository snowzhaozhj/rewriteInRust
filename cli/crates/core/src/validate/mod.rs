//! 配置/状态校验模块。
//!
//! 提供状态文件完整性检查、前置条件验证、blocked 模块检查与自动解除。

pub mod rules;
pub mod tiers;

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::error::{MigrateError, Result};
use crate::state::host_index::quote_key;
use crate::state::{HostIndex, HostResolution, MigrationStateMachine, STATE_SCHEMA_VERSION};
use crate::types::state::{MigrationStateFile, ModuleStatus, ProjectState};

/// 校验状态文件完整性。
///
/// 检查项：
/// - version 非空且 schema 主版本号与当前 CLI 兼容（见 [`check_version_compat`]）
/// - state_history 非空且末条状态与当前状态一致
/// - state_history 相邻状态满足合法转换（Init→Profile→…→Graduate）
/// - 前置条件：各状态要求的数据字段是否存在
/// - 防御性告警（不硬判损坏）：无签批审计的 done、非法 subagent_call status、
///   `blocked_by` 幽灵引用
pub fn validate_state(state_file: &MigrationStateFile) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();

    // schema 版本兼容性：非空 + 主版本号匹配（跨主版本拒绝加载）。
    check_version_compat(&state_file.version)?;

    // state_history 非空
    if state_file.state_history.is_empty() {
        return Err(MigrateError::SchemaValidation(
            "state_history 为空，至少应包含初始状态".to_owned(),
        ));
    }

    // 最后一条历史记录的状态应与当前状态一致
    if let Some(last) = state_file.state_history.last() {
        if last.state != state_file.state {
            return Err(MigrateError::SchemaValidation(format!(
                "state_history 末尾状态 ({}) 与当前状态 ({}) 不一致",
                last.state, state_file.state
            )));
        }
    }

    // 历史首条必须是状态机起点 Init。windows(2) 对单元素历史不做任何检查，
    // 若缺此项，伪造的 [Plan] 单元素历史可在前置条件满足时蒙混过关。
    if let Some(first) = state_file.state_history.first() {
        if first.state != ProjectState::Init {
            return Err(MigrateError::SchemaValidation(format!(
                "state_history 首条状态应为 init，实际为 {}（历史链起点被篡改或损坏）",
                first.state
            )));
        }
    }

    // exited_at 链完整性：除最后一条外都应有 exited_at（已退出），最后一条不应有
    // （当前所处状态）。防止伪造同时"进行中"的多条历史或断裂的时间链。
    let last_idx = state_file.state_history.len() - 1;
    for (i, entry) in state_file.state_history.iter().enumerate() {
        if i == last_idx {
            if entry.exited_at.is_some() {
                return Err(MigrateError::SchemaValidation(format!(
                    "state_history 末条（当前状态 {}）不应有 exited_at",
                    entry.state
                )));
            }
        } else if entry.exited_at.is_none() {
            return Err(MigrateError::SchemaValidation(format!(
                "state_history 非末条（状态 {}）缺少 exited_at",
                entry.state
            )));
        }
    }

    // state_history 相邻状态必须满足合法转换。正常流程由 machine.rs 的 transition
    // 保证，此处是对落盘文件的独立防御（检测外部篡改/损坏导致的跳级或回退历史）。
    for pair in state_file.state_history.windows(2) {
        if !pair[0].state.can_transition_to(pair[1].state) {
            return Err(MigrateError::SchemaValidation(format!(
                "state_history 含非法状态转换：{} → {}",
                pair[0].state, pair[1].state
            )));
        }
    }

    // 前置条件检查
    check_preconditions(state_file)?;

    // 可选警告：模块相关
    if state_file.state == ProjectState::SprintLoop && state_file.modules.is_empty() {
        warnings.push("处于 sprint_loop 阶段但 modules 为空".to_owned());
    }

    if state_file.state == ProjectState::SprintLoop && state_file.sprint.is_none() {
        warnings.push("处于 sprint_loop 阶段但 sprint 未设置".to_owned());
    }

    // MDR-019 防御性可观测：正常的 reviewing → done 只能由 approve_module 写入签批审计。
    // `update_module`、手工改 JSON 或旧版 CLI 仍可能造出无凭据的 done；校验不把旧状态硬判损坏，
    // 但必须告警，避免这种旁路静默存在。
    for (name, module) in &state_file.modules {
        if module.status == ModuleStatus::Done
            && !module.attempts.iter().any(|attempt| {
                attempt.result.starts_with("approved:human")
                    || attempt.result.starts_with("auto_approved_by_policy:")
            })
        {
            warnings.push(format!(
                "模块 `{name}` 已为 done，但 attempts 中缺少译后签批审计（approved:human 或 auto_approved_by_policy:<id>）"
            ));
        }
    }

    // M4 防御性可观测：`--status` 的四值域只在 CLI 参数层强校验，读侧此前完全无约束——
    // 旧 state 文件里的已废弃值（`success`/`failed`）、手工编辑的错拼、或绕过 CLI 直调
    // `push_subagent_call` 写入的任意字符串，反序列化都不报错。将来真按状态聚合统计时，
    // 这些值会被当未知态默默漏算，正是收窄值域要消灭的失败模式换到读侧。
    // 不硬判损坏（旧文件仍可读），但必须告警。
    const SUBAGENT_CALL_STATUSES: [&str; 4] = ["started", "ok", "error", "timeout"];
    let mut unknown_statuses: Vec<&str> = state_file
        .subagent_calls
        .iter()
        .map(|call| call.status.as_str())
        .filter(|status| !SUBAGENT_CALL_STATUSES.contains(status))
        .collect();
    if !unknown_statuses.is_empty() {
        unknown_statuses.sort_unstable();
        unknown_statuses.dedup();
        warnings.push(format!(
            "subagent_calls 含非法 status 值 {unknown_statuses:?}（合法值域 {SUBAGENT_CALL_STATUSES:?}）\
             ——已废弃的 success/failed 或手工编辑所致，按状态聚合统计时会被漏算"
        ));
    }

    // M4 防御性可观测：`blocked_by` 引用了既非登记模块、也不属于任何 composite 组的
    // key（幽灵引用）。对 blocked 模块，后果是**永久**阻塞——依赖根本不存在、永远不会
    // 进终态，`--auto-unblock` 也就永远不会解除它。此前无任何告警，编排器只看到它落在
    // `still_blocked` 里，与「依赖还在翻译中」完全无法区分。
    //
    // 同一语义在 `state deps` 侧早已做对（幽灵依赖单列 `unresolved` + warning，且
    // run.md 明令不得填进 `blocked_by`），读侧却漏了——本扫描补上这处不对称。
    //
    // 扫描与 `--check-blocked` 的机读明细共用 [`scan_ghost_references`]，避免两处覆盖
    // 口径不一致（曾经告警扫全部模块、`ghost_refs` 只取 blocked 模块，导致「warnings
    // 报了但机读字段是空数组」）。
    let ghosts = scan_ghost_references(state_file);
    if !ghosts.is_empty() {
        // 只陈述**可直接观测的事实**：哪些模块的 `blocked_by` 指向未登记 key，及它此刻是否
        // 正在阻塞。处置指向 `state repair`（MDR-022）——它是**确定性的单一动作**（删除无处
        // 归属的条目），不是按状态推导出来的「处方」。
        //
        // 曾经有一版按四个正交谓词（能否进 blocked / reset 是否需 `--force` / 项目是否
        // graduate / blocked 有无锚点）为每个状态推导「该跑哪条命令」，被审查全数推翻：依据
        // 只是「看出边」（一步可达），却对多步可达的状态（如 `degrade_* → translating →
        // blocked`）下了**「不可能」断言**，而反例存在（详见 MDR-021「第四轮」段）。**此处不得
        // 再按状态分叉给不同命令**——repair 对全部 11 个 status 一视同仁，正是因此才不需要
        // 任何可达性预言。同理不写「这是唯一入口」：`transition` 的两步往返也能清掉引用
        // （MDR-021 待办 3），声称唯一即又是一条假断言。
        let mut blocked_now: Vec<String> = Vec::new();
        let mut residual: Vec<String> = Vec::new();
        for g in &ghosts {
            let line = format!("{} → {}", quote_key(&g.module), quote_key(&g.missing));
            if g.status == ModuleStatus::Blocked {
                blocked_now.push(line);
            } else {
                residual.push(line);
            }
        }
        let mut parts: Vec<String> = Vec::new();
        if !blocked_now.is_empty() {
            parts.push(format!(
                "以下 blocked 模块的 blocked_by 指向未登记的 key：被引依赖永不进终态、\
                 `--auto-unblock` 永不放行，模块将永久阻塞（在 `still_blocked` 里与\
                 「依赖仍在翻译中」无法区分，须靠本告警与 `ghost_refs` 才能分辨）: {}",
                blocked_now.join("、")
            ));
        }
        if !residual.is_empty() {
            parts.push(format!(
                "以下模块（当前非 blocked）残留指向未登记 key 的 blocked_by: {}",
                residual.join("、")
            ));
        }
        warnings.push(format!(
            "{}——成因是 state 与 source-graph 不同步。处置：`{REPAIR_GHOST_COMMAND}`\
             （只删无处归属的条目，可归一的合法引用与宿主歧义引用都保留，不改状态、不清进度\
             字段；清完后剩余依赖全部终态的模块由 `--auto-unblock` 按 `pre_blocked_status` \
             恢复）。\
             **前提**是被引 key 确实不该存在——若它本应是登记模块、只是 analyze 漏登记，\
             删引用会让引用方提前解除阻塞，那种情况应重跑 analyze 重建 state。不要靠等待\
             依赖就绪，也不要用 `state populate-modules`（对非 pending 模块拒绝重填）。\
             逐条机读明细（`{{module, missing, status}}`）见 `--check-blocked` 输出的 \
             `ghost_refs`",
            parts.join("；")
        ));
    }

    // M4 防御性可观测：`member_files` 跨组互斥不变量被破坏（同一文件的宿主不唯一）。
    // `machine.rs` 的 `canonical_module_key` 对此是硬错（MDR-015:55），但校验命令不能硬错
    // ——旧文件须可读。这里报出来，并在 `check_blocked_modules` 侧按最保守的非终态处理：
    // 宿主组状态各异，静默择一会让 `--auto-unblock` 据错误宿主判 ready、真的把模块解除
    // （实证：X 同属 done 组与 translating 组时，取到 done 那组即解除落盘）。
    //
    // **扫全量划分，不沿 `blocked_by` 引用路径撞见**（MDR-023）：此前这里遍历各模块的
    // `blocked_by`、只对被引用到的 key 判歧义，于是「破坏存在但没有任何 `blocked_by` 引用
    // 到它」时 `validate state` 一声不响，而下一步对该模块的 `transition`/`reset` 会硬错
    // ——体检说健康、动手就报错。判据取 `HostIndex::broken_partitions`（与 `resolve` 同一
    // 实现的 `Ambiguous` 那一支），故告警口径与判定口径不可能各说各话。
    let broken = HostIndex::build(state_file).broken_partitions();
    if !broken.is_empty() {
        let listed: Vec<String> = broken
            .iter()
            .map(|(file, hosts)| {
                let hosts = hosts
                    .iter()
                    .map(|h| quote_key(h))
                    .collect::<Vec<_>>()
                    .join("、");
                format!("{}（同属 {}）", quote_key(file), hosts)
            })
            .collect();
        let commands = KEY_NORMALIZING_COMMANDS
            .iter()
            .map(|c| format!("`{c}`"))
            .collect::<Vec<_>>()
            .join("/");
        warnings.push(format!(
            "member_files 跨组互斥不变量被破坏，以下文件的宿主不唯一: {}\
             ——无法判定应按哪个组的状态判就绪，引用它们的 blocked_by 一律按未就绪处理\
             （不会被 `--auto-unblock` 解除）；凡**按 key 归一的单模块操作**（{}）对它们直接\
             报错而非猜一个宿主，`state batch-transition-done` 记 \
             `skipped.code=broken_partition`。\
             处置：修正 modules 的 member_files 划分，使每个文件只属一个组；\
             **注意宿主清单里可能有该文件自己**——那表示它既登记为独立模块、又被别的组列为\
             成员，二者留一个。（不带 `--module` 的全量 `{REPAIR_GHOST_COMMAND}` 不受影响，\
             它不做 key 归一——但它也清不掉本问题，幽灵引用与坏划分是两回事）",
            listed.join("；"),
            commands
        ));
    }

    Ok(warnings)
}

/// 校验状态文件 schema 版本与当前 CLI 的兼容性。
///
/// 规则（语义化版本，对照 [`STATE_SCHEMA_VERSION`]）：
/// - 空字符串：损坏/缺失，返回 `SchemaValidation`。
/// - 格式非法（无法解析出主版本号）：返回 `SchemaValidation`。
/// - **主版本号 ≠ 当前主版本号**：schema 不兼容（破坏性结构变更），返回 `SchemaValidation`
///   并提示当前 CLI 支持的版本——避免新 CLI 误读旧结构或旧 CLI 误读新字段导致静默错乱。
/// - 主版本号一致（次/修订号任意）：兼容，放行。
fn check_version_compat(version: &str) -> Result<()> {
    if version.is_empty() {
        return Err(MigrateError::SchemaValidation(
            "version 字段为空".to_owned(),
        ));
    }

    let parse_major = |v: &str| v.split('.').next().and_then(|s| s.parse::<u32>().ok());

    let Some(file_major) = parse_major(version) else {
        return Err(MigrateError::SchemaValidation(format!(
            "version 字段格式非法：`{version}`（应为语义化版本，如 `{STATE_SCHEMA_VERSION}`）"
        )));
    };
    // 当前常量来自代码内编译期值，必可解析。
    let current_major =
        parse_major(STATE_SCHEMA_VERSION).expect("STATE_SCHEMA_VERSION 应为合法语义化版本");

    if file_major != current_major {
        // TODO(M2-ERR-01): 错误码细分时改用专属 `SCHEMA_VERSION_UNSUPPORTED`（设计 06 §10.7），
        // 便于 SKILL.md 按码路由升级/回退；当前 MVP 阶段复用 schema_validation kind。
        return Err(MigrateError::SchemaValidation(format!(
            "migration-state.json schema 版本不兼容：文件为 `{version}`（主版本 {file_major}），\
             当前 CLI 支持主版本 {current_major}（`{STATE_SCHEMA_VERSION}`）。\
             跨主版本结构不兼容，请改用匹配版本的 rustmigrate 或重新执行 init"
        )));
    }
    Ok(())
}

/// 前置条件检查：确保进入特定状态前所需数据已就位。
///
/// 硬性前置（不满足返回 `PreconditionFailed`）：
/// - Profile / Plan / Scaffold / SprintLoop：需要 project 信息
/// - Plan / Scaffold / SprintLoop：需要 graph 构建完成
///   （graph build 在 Profile 阶段产出，见 `docs/design/06 § 10.2` analyzer 前置）
///
/// 软警告（见 `validate_state`，非硬前置）：SprintLoop 的 sprint / modules 缺失。
/// Graduate 的模块终态校验待 graduate 命令落地（TODO(M2-ADV-03)）。
fn check_preconditions(state_file: &MigrationStateFile) -> Result<()> {
    match state_file.state {
        ProjectState::Init => {
            // 初始阶段无前置条件
        }
        ProjectState::Profile => {
            require_project(state_file, "profile")?;
        }
        ProjectState::Plan => {
            require_project(state_file, "plan")?;
            require_graph(state_file, "plan")?;
        }
        ProjectState::Scaffold => {
            require_project(state_file, "scaffold")?;
            require_graph(state_file, "scaffold")?;
        }
        ProjectState::SprintLoop => {
            require_project(state_file, "sprint_loop")?;
            require_graph(state_file, "sprint_loop")?;
        }
        ProjectState::Graduate => {
            // TODO(M2-ADV-03): graduate 命令落地时，校验所有模块为终态并对未完成模块告警
        }
    }
    Ok(())
}

/// 要求 project 信息存在，否则返回带阶段名的前置失败。
fn require_project(state_file: &MigrationStateFile, phase: &str) -> Result<()> {
    if state_file.project.is_none() {
        return Err(MigrateError::PreconditionFailed {
            condition: format!("进入 {phase} 阶段需要 project 信息"),
        });
    }
    Ok(())
}

/// 要求 graph 构建已完成（metadata 缺失视为未完成），否则返回带阶段名的前置失败。
fn require_graph(state_file: &MigrationStateFile, phase: &str) -> Result<()> {
    let graph_done = state_file
        .metadata
        .as_ref()
        .map(|m| m.graph_build_completed)
        .unwrap_or(false);
    if !graph_done {
        return Err(MigrateError::PreconditionFailed {
            condition: format!("进入 {phase} 阶段需要 graph 构建完成"),
        });
    }
    Ok(())
}

// === blocked 模块检查与自动解除 ===

/// 单个 blocked 模块的检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockedCheckResult {
    /// 模块 key。
    pub module: String,
    /// 该模块的 `blocked_by` 列表。
    pub blocked_by: Vec<String>,
    /// `blocked_by` 中已进入终态（done/degrade_*）的模块。
    pub resolved: Vec<String>,
    /// `blocked_by` 中尚未终态的模块。
    ///
    /// **含无处归属的引用**（幽灵引用）与宿主歧义的引用：两者都不该让模块变成
    /// 「就绪可解除」，否则 `--auto-unblock` 会在损坏数据上真的改状态。要区分出
    /// 「哪些是幽灵」请用 [`scan_ghost_references`]——本结构不再单列，避免同一概念
    /// 存在两份可能相互漂移的表示（`--check-blocked` 的 `ghost_refs` 曾因两处各算
    /// 一遍而与告警口径不一致）。
    pub unresolved: Vec<String>,
    /// 是否就绪可解除（`unresolved` 为空）。
    pub ready: bool,
}

/// 幽灵 `blocked_by` 引用的处置命令（不含 `rustmigrate` 前缀与可选 `--module`）。
///
/// **告警文案与测试/e2e 的 argv 共用这一个字符串**，故「文案给的命令」与「照做真能跑的命令」
/// 不可能漂移。上一轮为同一功能写的 e2e 是「解析人读文案、再跑手写 argv」，二者毫无绑定，
/// 四个审查视角各自用变异穿透了它（改成 `--keep-progress`/`--bogus-flag` 全部 PASS）；
/// 后来改成按机读字段构造 argv，又因那个字段是**按状态推导**的而整体被推翻（MDR-021）。
/// 现在的处置是**对全部 11 个 status 一视同仁的单一动作**，所以它能是一个常量——这正是
/// repair 不需要任何可达性预言的原因。
pub const REPAIR_GHOST_COMMAND: &str = "state repair --clear-ghost-blocked-by";

/// 「宿主不唯一」时会**直接报错**的单模块操作（子命令路径，不含 `rustmigrate` 前缀与各自的
/// 必填参数）。
///
/// 共同特征是**按 key 归一**：都经 `MigrationStateMachine::canonical_module_key` 把入参解析
/// 到宿主模块，宿主不唯一即拒绝。跨组告警的文案由本常量插值，e2e 亦据它校验「文案列举的每条
/// 都被真跑过」——沿用 [`REPAIR_GHOST_COMMAND`] 的绑定手法（MDR-022 决策 7），使「文案说会报错
/// 的命令」与「实测真报错的命令」不可能各说各话。
///
/// **不含**不带 `--module` 的全量 `state repair`：它不做 key 归一，故不报错（实测 noop）——
/// 那条曾被本告警的初版一并列进去，是靠推断而非实测写下的失实声明。
/// 也不含 `state batch-transition-done`：它逐模块独立处理、不硬错，而是记
/// `skipped.code=broken_partition`。
pub const KEY_NORMALIZING_COMMANDS: [&str; 4] = [
    "state transition",
    "state reset",
    "state recover",
    "state repair --module",
];

/// 一条幽灵引用：`module` 的 `blocked_by` 指向了 `missing`，而后者无处归属。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GhostReference {
    /// 持有该 `blocked_by` 的模块 key。
    pub module: String,
    /// 无法归一到任何模块的被引 key。
    pub missing: String,
    /// 持有方当前状态。
    ///
    /// `status == Blocked` 表示此刻就被永久阻塞（被引依赖永不进终态、`--auto-unblock`
    /// 永不放行）；其余状态表示只是残留引用、当前不阻塞。**仅陈述状态**，不据此推导
    /// 处置命令——按谓词推导「处置方案」的做法已被审查推翻并拆出为后续 PR（见 MDR-021）。
    pub status: ModuleStatus,
}

/// 扫描全部模块的 `blocked_by`，返回无法归一到任何模块的引用。
///
/// 覆盖**全部模块**而非仅 blocked：正常路径下离开 blocked 会清空 `blocked_by`
/// （`machine.rs` 的 `transition_module`），但手工编辑或旧文件可能在非 blocked 模块上
/// 留下残值，一旦该模块再次被标 blocked 就会立刻踩中同一个坑。
///
/// 判据经 [`HostIndex`] 归一，故 composite 组的非代表成员 key 不算幽灵。
///
/// 结果按 (module, missing) 字典序排序：`modules` 是 `HashMap`，不排序则告警文本与
/// JSON 明细的条目顺序在每次运行间漂移。
pub fn scan_ghost_references(state_file: &MigrationStateFile) -> Vec<GhostReference> {
    let mut out: Vec<GhostReference> = Vec::new();
    // 索引建一次：坏 state 的引用全部未命中，逐条全表扫即二次复杂度（MDR-022 待办 5）。
    // 本函数正是 `state repair` 的判据来源，而 repair 面向的就是坏 state。
    let hosts = HostIndex::build(state_file);
    for (name, module) in &state_file.modules {
        let Some(blocked_by) = module.blocked_by.as_ref() else {
            continue;
        };
        for dep in blocked_by {
            if hosts.resolve(dep) == HostResolution::Missing {
                out.push(GhostReference {
                    module: name.clone(),
                    missing: dep.clone(),
                    status: module.status,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.module
            .cmp(&b.module)
            .then_with(|| a.missing.cmp(&b.missing))
    });
    out.dedup();
    out
}

/// 检查所有 blocked 模块的依赖就绪状态。
///
/// 遍历 `modules` 中 `status == Blocked` 的模块，逐个检查其 `blocked_by`
/// 引用的模块是否已进入终态（done/degrade_ffi/degrade_manual/degrade_skip）。
///
/// 引用先经 [`HostIndex`] 归一（composite 组成员 key → 组代表），就绪与否按**归一后**的
/// 模块判定。归一后仍无归属的引用（幽灵引用）与宿主歧义的引用一律计入 `unresolved`、
/// 不判就绪，理由见 [`BlockedCheckResult::unresolved`]；要单独取出哪些是幽灵引用请用
/// [`scan_ghost_references`]。
///
/// 返回每个 blocked 模块的检查结果（含已解决/未解决依赖列表）。
pub fn check_blocked_modules(state_file: &MigrationStateFile) -> Vec<BlockedCheckResult> {
    let mut results = Vec::new();
    // 索引建一次给全部引用复用：逐条全表扫在坏 state（全部引用都未命中）上是二次复杂度，
    // 实测 20000 模块 14.2s（MDR-022 待办 5）。
    let hosts = HostIndex::build(state_file);

    // 收集所有 blocked 模块（排序保证确定性输出）。
    let mut blocked_keys: Vec<&String> = state_file
        .modules
        .iter()
        .filter(|(_, m)| m.status == ModuleStatus::Blocked)
        .map(|(k, _)| k)
        .collect();
    blocked_keys.sort();

    for key in blocked_keys {
        let module = &state_file.modules[key];
        let blocked_by = module.blocked_by.as_ref().cloned().unwrap_or_default();

        let mut resolved = Vec::new();
        let mut unresolved = Vec::new();

        for dep in &blocked_by {
            // 四分：无归属（真幽灵）/ 多宿主（坏划分）/ 归一后终态 / 归一后未终态。
            // 此前「不存在」与「未终态」都经 `unwrap_or(false)` 落进 `unresolved`，两种
            // 相反的处置动作被抹平；而不做归一则会把 composite 组的合法成员 key 误判
            // 成不存在。
            //
            // 注意 `resolved`/`unresolved` 里保留的是**原始** dep 字符串而非归一后的
            // key：这些列表要能与用户 state 里的 `blocked_by` 逐条对上，换成组代表会
            // 让人对不上自己写的是什么。归一只用于判定，不改回显。
            match hosts.resolve(dep) {
                // 幽灵引用同样按未终态处理（等待永远不会结束，但绝不能因此判就绪）。
                // 「哪些是幽灵」由 `scan_ghost_references` 单独提供，此处不重复表示。
                HostResolution::Missing => unresolved.push(dep.clone()),
                // 坏划分：宿主组状态各异，挑错就判错就绪 → 一律按非终态处理，
                // 绝不让它把模块推成 ready（`--auto-unblock` 会据此真的改状态）。
                // 但它不算幽灵（故不进 `scan_ghost_references` 结果），处置动作是修组
                // 划分而非重新同步 state。
                HostResolution::Ambiguous(_) => unresolved.push(dep.clone()),
                HostResolution::Resolved(canonical)
                    if state_file.modules[canonical].status.is_terminal() =>
                {
                    resolved.push(dep.clone())
                }
                HostResolution::Resolved(_) => unresolved.push(dep.clone()),
            }
        }

        let ready = unresolved.is_empty();
        results.push(BlockedCheckResult {
            module: key.clone(),
            blocked_by,
            resolved,
            unresolved,
            ready,
        });
    }

    results
}

/// 自动解除就绪的 blocked 模块：恢复到 `pre_blocked_status`。
///
/// 对 `checks` 中 `ready == true` 的模块，调用
/// `MigrationStateMachine::transition_module` 恢复到其 `pre_blocked_status`
/// （无 `pre_blocked_status` 时默认恢复为 `pending`）。
///
/// `checks` 参数由调用方预先调用 `check_blocked_modules` 获得，避免重复计算。
///
/// 返回成功解除的模块 key 列表。恢复失败的模块通过 `warnings` 报告。
pub fn auto_unblock_modules(
    machine: &mut MigrationStateMachine,
    checks: &[BlockedCheckResult],
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let ready_modules: Vec<(String, ModuleStatus)> = checks
        .iter()
        .filter(|r| r.ready)
        .map(|r| {
            let target = machine
                .state_file()
                .modules
                .get(&r.module)
                .and_then(|m| m.pre_blocked_status)
                .unwrap_or(ModuleStatus::Pending);
            (r.module.clone(), target)
        })
        .collect();

    let mut unblocked = Vec::new();
    for (module, target) in ready_modules {
        match machine.transition_module(
            &module,
            Some(target),
            None,
            Some("blocked_by resolved"),
            false,
        ) {
            Ok(()) => unblocked.push(module),
            Err(e) => warnings.push(format!("自动解除 blocked 模块 `{module}` 失败: {e}")),
        }
    }
    unblocked
}

/// 检测 blocked_by 关系图中的环路（DFS 着色法）。
///
/// 在 blocked 模块之间构建子图：节点为所有 `status == Blocked` 的模块，
/// 边为 `blocked_by` 关系（M blocked_by N 且 N 也是 blocked → 边 M→N）。
/// 用三色 DFS 检测环：白色（未访问）→ 灰色（栈上）→ 黑色（已完成）。
/// 遇到灰色节点即发现环，回溯栈提取环路径。
///
/// 建边前先经 [`HostIndex`] 归一（composite 组成员 key → 组代表），
/// **必须与 `check_blocked_modules` 用同一判据**：若这里按原始字符串建边而那边归一，
/// 「经成员 key 表达的互锁」会成为静默死锁——两侧都判成合法未终态依赖（无幽灵告警），
/// 而成员 key 不在 `blocked_set` 里、边被丢弃（无环告警），模块永久阻塞且零诊断。
///
/// 返回所有检测到的环路径（每条环路径为 Vec<String>）。空 Vec 表示无环。
pub fn detect_blocked_cycles(state_file: &MigrationStateFile) -> Vec<Vec<String>> {
    // 收集所有 blocked 模块的 key 集合。
    let blocked_set: HashSet<&String> = state_file
        .modules
        .iter()
        .filter(|(_, m)| m.status == ModuleStatus::Blocked)
        .map(|(k, _)| k)
        .collect();

    if blocked_set.is_empty() {
        return Vec::new();
    }

    // 索引建一次给全部边复用（MDR-022 待办 5）。
    let hosts = HostIndex::build(state_file);

    // 构建 blocked 子图的邻接表：M → [N...]（M blocked_by N，N 归一后也是 blocked）。
    let mut adj: HashMap<&String, Vec<&String>> = HashMap::new();
    for key in &blocked_set {
        let deps: Vec<&String> = state_file.modules[*key]
            .blocked_by
            .as_ref()
            .map(|bs| {
                bs.iter()
                    .filter_map(|b| match hosts.resolve(b) {
                        HostResolution::Resolved(key) => Some(key),
                        // 坏划分的边不建：宿主歧义时连哪条都是猜。该情形另有专门告警
                        // （`validate_state` 的跨组不变量扫描），不在此处静默择一。
                        HostResolution::Ambiguous(_) | HostResolution::Missing => None,
                    })
                    .filter(|b| blocked_set.contains(b))
                    .collect()
            })
            .unwrap_or_default();
        adj.insert(key, deps);
    }

    // DFS 着色：0=白, 1=灰（栈上）, 2=黑（已完成）。
    let mut color: HashMap<&String, u8> = blocked_set.iter().map(|k| (*k, 0u8)).collect();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    // 排序保证确定性环检测顺序。
    let mut sorted_keys: Vec<&&String> = blocked_set.iter().collect();
    sorted_keys.sort();

    for start in sorted_keys {
        if color[*start] == 0 {
            let mut stack: Vec<&String> = Vec::new();
            dfs_detect_cycle(start, &adj, &mut color, &mut stack, &mut cycles);
        }
    }

    cycles
}

/// DFS 递归检测环（内部函数）。
fn dfs_detect_cycle<'a>(
    node: &'a String,
    adj: &HashMap<&'a String, Vec<&'a String>>,
    color: &mut HashMap<&'a String, u8>,
    stack: &mut Vec<&'a String>,
    cycles: &mut Vec<Vec<String>>,
) {
    color.insert(node, 1); // 灰色：进入栈。
    stack.push(node);

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            match color.get(*neighbor) {
                Some(1) => {
                    // 灰色：发现环，从栈中提取环路径。
                    let cycle_start = stack.iter().position(|n| *n == *neighbor).unwrap();
                    let mut cycle: Vec<String> =
                        stack[cycle_start..].iter().map(|n| (*n).clone()).collect();
                    cycle.push((*neighbor).clone()); // 闭合环。
                    cycles.push(cycle);
                }
                // 白色：继续探索。
                Some(0) => {
                    dfs_detect_cycle(neighbor, adj, color, stack, cycles);
                }
                _ => {} // 黑色或不在 blocked 子图中：跳过。
            }
        }
    }

    stack.pop();
    color.insert(node, 2); // 黑色：完成。
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{SourceLang, Timestamp};
    use crate::types::state::{
        DangerProvenance, MigrationMetadata, ProjectInfo, StateHistoryEntry,
    };
    use std::collections::HashMap;

    /// 辅助：构建从 Init 到目标状态的合法历史链（除末条外均带 exited_at）。
    fn history_chain(target: ProjectState) -> Vec<StateHistoryEntry> {
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let order = [
            ProjectState::Init,
            ProjectState::Profile,
            ProjectState::Plan,
            ProjectState::Scaffold,
            ProjectState::SprintLoop,
            ProjectState::Graduate,
        ];
        let target_idx = order
            .iter()
            .position(|s| *s == target)
            .expect("target 必在 order 内");
        order[..=target_idx]
            .iter()
            .enumerate()
            .map(|(i, s)| StateHistoryEntry {
                state: *s,
                entered_at: now.clone(),
                exited_at: if i == target_idx {
                    None
                } else {
                    Some(now.clone())
                },
            })
            .collect()
    }

    /// 辅助：构建最小合法状态文件（Init 阶段）。
    fn minimal_init_state() -> MigrationStateFile {
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Init,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: now.clone(),
                exited_at: None,
            }],
            project: Some(ProjectInfo {
                name: "test".to_owned(),
                source_language: SourceLang::TypeScript,
                source_commit: None,
                source_loc: 100,
                created_at: now,
            }),
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: Some(MigrationMetadata {
                graph_build_completed: false,
                graph_build_completed_at: None,
                last_error: None,
                lock_token: None,
                version: 0,
                last_modified_by: None,
            }),
        }
    }

    #[test]
    fn test_validate_valid_init_state() {
        let state = minimal_init_state();
        let result = validate_state(&state);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_empty_version() {
        let mut state = minimal_init_state();
        state.version = String::new();
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::SchemaValidation(msg) => {
                assert!(msg.contains("version"));
            }
            other => panic!("期望 SchemaValidation，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_compatible_minor_version() {
        // 同主版本不同次/修订号视为兼容（向后读取）。
        let mut state = minimal_init_state();
        state.version = "1.5.2".to_owned();
        assert!(validate_state(&state).is_ok());
    }

    #[test]
    fn test_validate_incompatible_major_version() {
        // 跨主版本：schema 不兼容，拒绝加载并提示当前支持版本。
        let mut state = minimal_init_state();
        state.version = "2.0.0".to_owned();
        match validate_state(&state).unwrap_err() {
            MigrateError::SchemaValidation(msg) => {
                assert!(msg.contains("不兼容"), "应提示版本不兼容: {msg}");
                assert!(
                    msg.contains(STATE_SCHEMA_VERSION),
                    "应提示当前支持版本: {msg}"
                );
            }
            other => panic!("期望 SchemaValidation，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_malformed_version() {
        // 非语义化版本：无法解析主版本号，拒绝。
        let mut state = minimal_init_state();
        state.version = "not-a-version".to_owned();
        match validate_state(&state).unwrap_err() {
            MigrateError::SchemaValidation(msg) => {
                assert!(msg.contains("格式非法"), "应提示格式非法: {msg}");
            }
            other => panic!("期望 SchemaValidation，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_empty_history() {
        let mut state = minimal_init_state();
        state.state_history.clear();
        let result = validate_state(&state);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_history_tail_mismatch() {
        let mut state = minimal_init_state();
        state.state = ProjectState::Profile;
        // history 仍然是 Init
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::SchemaValidation(msg) => {
                assert!(msg.contains("不一致"));
            }
            other => panic!("期望 SchemaValidation，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_plan_without_project() {
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Plan,
            state_history: history_chain(ProjectState::Plan),
            project: None,
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: None,
        };
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::PreconditionFailed { condition } => {
                assert!(condition.contains("project"));
            }
            other => panic!("期望 PreconditionFailed，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_scaffold_without_graph() {
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Scaffold,
            state_history: history_chain(ProjectState::Scaffold),
            project: Some(ProjectInfo {
                name: "test".to_owned(),
                source_language: SourceLang::TypeScript,
                source_commit: None,
                source_loc: 100,
                created_at: now,
            }),
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: Some(MigrationMetadata {
                graph_build_completed: false,
                graph_build_completed_at: None,
                last_error: None,
                lock_token: None,
                version: 0,
                last_modified_by: None,
            }),
        };
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::PreconditionFailed { condition } => {
                assert!(condition.contains("graph"));
            }
            other => panic!("期望 PreconditionFailed，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_history_illegal_transition() {
        // history 跳级（Init → Plan，跳过 Profile），末尾与当前状态一致但序列非法。
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let mut state = minimal_init_state();
        state.state = ProjectState::Plan;
        state.state_history = vec![
            StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: now.clone(),
                exited_at: Some(now.clone()),
            },
            StateHistoryEntry {
                state: ProjectState::Plan,
                entered_at: now,
                exited_at: None,
            },
        ];
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::SchemaValidation(msg) => {
                assert!(msg.contains("非法"));
                assert!(msg.contains("转换"));
            }
            other => panic!("期望 SchemaValidation，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_plan_without_graph() {
        // Plan 阶段 project 齐全但 graph 未构建 → 前置失败。
        // minimal_init_state 的 metadata.graph_build_completed 默认 false。
        let mut state = minimal_init_state();
        state.state = ProjectState::Plan;
        state.state_history = history_chain(ProjectState::Plan);
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::PreconditionFailed { condition } => {
                assert!(condition.contains("graph"));
                assert!(condition.contains("plan"));
            }
            other => panic!("期望 PreconditionFailed，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_sprint_loop_warnings() {
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::SprintLoop,
            state_history: history_chain(ProjectState::SprintLoop),
            project: Some(ProjectInfo {
                name: "test".to_owned(),
                source_language: SourceLang::TypeScript,
                source_commit: None,
                source_loc: 100,
                created_at: now,
            }),
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: Some(MigrationMetadata {
                graph_build_completed: true,
                graph_build_completed_at: None,
                last_error: None,
                lock_token: None,
                version: 0,
                last_modified_by: None,
            }),
        };
        let result = validate_state(&state);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("modules")));
        assert!(warnings.iter().any(|w| w.contains("sprint")));
    }

    #[test]
    fn test_validate_profile_without_project() {
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Profile,
            state_history: history_chain(ProjectState::Profile),
            project: None,
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: None,
        };
        let result = validate_state(&state);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::PreconditionFailed { condition } => {
                assert!(condition.contains("project"));
            }
            other => panic!("期望 PreconditionFailed，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_rejects_non_init_start() {
        // 伪造从中途状态开始的单元素历史：末尾与当前一致、前置满足，
        // 但首条非 Init，应被拦截（修复前 windows(2) 对单元素不检查会放过）。
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let mut state = minimal_init_state();
        state.state = ProjectState::Plan;
        state.state_history = vec![StateHistoryEntry {
            state: ProjectState::Plan,
            entered_at: now,
            exited_at: None,
        }];
        // 让前置条件全部满足，确保拦截来自历史起点校验而非 precondition。
        state.metadata = Some(MigrationMetadata {
            graph_build_completed: true,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
            version: 0,
            last_modified_by: None,
        });
        let result = validate_state(&state);
        match result.unwrap_err() {
            MigrateError::SchemaValidation(msg) => assert!(msg.contains("init")),
            other => panic!("期望 SchemaValidation(init)，实际: {:?}", other),
        }
    }

    #[test]
    fn test_validate_rejects_broken_exited_chain() {
        // 两条历史但首条缺 exited_at（伪造同时"进行中"），应被拦截。
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let mut state = minimal_init_state();
        state.state = ProjectState::Profile;
        state.state_history = vec![
            StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: now.clone(),
                exited_at: None, // 非末条却无 exited_at
            },
            StateHistoryEntry {
                state: ProjectState::Profile,
                entered_at: now,
                exited_at: None,
            },
        ];
        let result = validate_state(&state);
        match result.unwrap_err() {
            MigrateError::SchemaValidation(msg) => assert!(msg.contains("exited_at")),
            other => panic!("期望 SchemaValidation(exited_at)，实际: {:?}", other),
        }
    }

    // === check_blocked_modules / auto_unblock_modules / detect_blocked_cycles 测试 ===

    use crate::types::state::ModuleState;

    /// 辅助：构造指定状态的最小模块记录。
    fn module_with_status(status: ModuleStatus) -> ModuleState {
        ModuleState {
            status,
            substatus: None,
            sprint: None,
            attempts: Vec::new(),
            test_pass_rate: None,
            coverage: None,
            known_differences: 0,
            tier: None,
            phase_a_version: None,
            phase_a_audit_passed: None,
            blocked_by: None,
            pre_blocked_status: None,
            member_files: None,
            composite_kind: None,
            decomposition_snapshot: None,
            decomposition_frozen: false,
            danger: Vec::new(),
            danger_provenance: DangerProvenance::Unclassified,
        }
    }

    #[test]
    fn test_validate_warns_done_module_without_approval_audit() {
        // MDR-019：done 但无签批审计 → 告警（手工改 JSON / update_module 旁路可观测）。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("naked".to_owned(), module_with_status(ModuleStatus::Done));
        let warnings = validate_state(&state).unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("naked") && w.contains("译后签批审计")),
            "{warnings:?}"
        );

        // 有 approved:human 审计 → 不告警；auto_approved_by_policy 同理。
        for audit in ["approved:human reason=审毕", "auto_approved_by_policy:x"] {
            let mut signed = module_with_status(ModuleStatus::Done);
            signed.attempts.push(crate::types::state::AttemptRecord {
                timestamp: Timestamp::new("2026-07-27T00:00:00Z"),
                result: audit.to_owned(),
                retry_count: 0,
                checkpoint: None,
            });
            state.modules.insert("naked".to_owned(), signed);
            let warnings = validate_state(&state).unwrap();
            assert!(
                !warnings.iter().any(|w| w.contains("译后签批审计")),
                "{audit} 应视为已签批: {warnings:?}"
            );
        }
    }

    #[test]
    fn test_validate_warns_unknown_subagent_call_status() {
        // M4：`--status` 四值域只在 CLI 参数层强校验，读侧此前无约束——旧文件里的
        // 已废弃 `success`/`failed`、手工错拼、绕过 CLI 直调写入的值，反序列化都不报错。
        let mut state = minimal_init_state();
        let call = |status: &str| crate::types::state::SubAgentCall {
            step_index: 1,
            subagent_name: "translator".to_owned(),
            started_at: Timestamp::new("2026-07-28T00:00:00Z"),
            ended_at: None,
            status: status.to_owned(),
            error_message: None,
        };

        // 四个合法值都不该告警。
        state.subagent_calls = ["started", "ok", "error", "timeout"].map(call).to_vec();
        let warnings = validate_state(&state).unwrap();
        assert!(
            !warnings.iter().any(|w| w.contains("subagent_calls")),
            "合法四值不应告警: {warnings:?}"
        );

        // 已废弃值 + 拼写错误 → 告警且逐个列出（去重后字典序）。
        state.subagent_calls = ["success", "failed", "sucess", "ok", "success"]
            .map(call)
            .to_vec();
        let warnings = validate_state(&state).unwrap();
        let hit = warnings
            .iter()
            .find(|w| w.contains("subagent_calls"))
            .unwrap_or_else(|| panic!("应告警非法 status: {warnings:?}"));
        for bad in ["success", "failed", "sucess"] {
            assert!(hit.contains(bad), "告警应列出 {bad}: {hit}");
        }
        // 重复值应去重（避免 N 条记录刷屏）。注意 `success` 是 `sucess` 的子串，
        // 故不能数 `matches("success")`——按带引号的完整条目计数。
        assert_eq!(hit.matches("\"success\"").count(), 1, "重复值应去重: {hit}");
    }

    /// pub core API 绕过 CLI 值域校验后，读侧告警仍须兜住（含存盘 → 读回一轮）。
    ///
    /// 异构交叉审查（codex）指出的敞口：值域收窄只发生在 CLI 参数层，`push_subagent_call`
    /// 是 `pub` 且仍收 `String`，外部 Rust 调用者可写任意值。该分层是既定惯例（同
    /// `ModuleState::substatus`，设计契约审查判 PASS）、本 PR 不改签名，但**兜底必须有
    /// 回归锁**——上一个测试直接构造 `SubAgentCall` 结构体，没走这条真实绕过路径，
    /// 也没验证非法值经 `save` → `load` 往返后仍被告警。
    #[test]
    fn test_validate_warns_on_status_written_via_pub_api_after_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("migration-state.json");

        let mut machine = crate::state::MigrationStateMachine::init_new(
            "proof",
            crate::types::common::SourceLang::TypeScript,
        );
        // 绕过 CLI 直调 pub API 写入非法值。
        machine.push_subagent_call(
            1,
            "translator".to_owned(),
            "totally-invalid".to_owned(),
            Some(Timestamp::new("2026-08-01T00:00:00Z")),
            None,
            None,
        );
        machine.save(&path).unwrap();

        let loaded = crate::state::MigrationStateMachine::load(&path).unwrap();
        assert_eq!(
            loaded.state_file().subagent_calls[0].status,
            "totally-invalid",
            "旧文件/绕过写入的值必须能读回（反序列化不得硬失败），否则 state 变砖"
        );

        let warnings = validate_state(loaded.state_file()).unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("subagent_calls") && w.contains("totally-invalid")),
            "经 pub API 写入并往返后，读侧仍须告警该非法值: {warnings:?}"
        );
    }

    #[test]
    fn test_check_blocked_no_blocked_modules() {
        // 无 blocked 模块：返回空列表。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("a".to_owned(), module_with_status(ModuleStatus::Pending));
        state
            .modules
            .insert("b".to_owned(), module_with_status(ModuleStatus::Done));
        let results = check_blocked_modules(&state);
        assert!(results.is_empty());
    }

    #[test]
    fn test_check_blocked_ready_to_unblock() {
        // blocked_by 全部终态 → ready=true。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("dep".to_owned(), module_with_status(ModuleStatus::Done));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(results[0].ready);
        assert_eq!(results[0].resolved, vec!["dep".to_owned()]);
        assert!(results[0].unresolved.is_empty());
    }

    #[test]
    fn test_check_blocked_still_blocked() {
        // blocked_by 含非终态 → ready=false。
        let mut state = minimal_init_state();
        state.modules.insert(
            "dep".to_owned(),
            module_with_status(ModuleStatus::Translating),
        );
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(!results[0].ready);
        assert_eq!(results[0].unresolved, vec!["dep".to_owned()]);
    }

    #[test]
    fn test_check_blocked_degrade_counts_as_terminal() {
        // blocked_by 指向 degrade_ffi → 视为终态，ready=true。
        let mut state = minimal_init_state();
        state.modules.insert(
            "dep".to_owned(),
            module_with_status(ModuleStatus::DegradeFfi),
        );
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(results[0].ready);
    }

    #[test]
    fn test_check_blocked_missing_dep_not_terminal() {
        // blocked_by 引用不存在的模块 → 视为非终态（安全侧），且能被扫成幽灵引用。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["nonexistent".to_owned()]);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(!results[0].ready);
        assert_eq!(results[0].unresolved, vec!["nonexistent".to_owned()]);
        // 幽灵引用要能被单独识别出来——由 scan_ghost_references 提供。
        let ghosts = scan_ghost_references(&state);
        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0].missing, "nonexistent");
    }

    #[test]
    fn test_check_blocked_separates_missing_from_pending_dep() {
        // 同一模块同时有「真实但未终态」与「幽灵」两类依赖：都进 unresolved，
        // 但只有后者进幽灵扫描结果。两者处置动作相反（等 vs 重新同步 state），
        // 若不分列，编排器无从区分。
        let mut state = minimal_init_state();
        state.modules.insert(
            "real_dep".to_owned(),
            module_with_status(ModuleStatus::Translating),
        );
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["real_dep".to_owned(), "ghost".to_owned()]);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].unresolved,
            vec!["real_dep".to_owned(), "ghost".to_owned()]
        );
        assert!(results[0].resolved.is_empty());
        // 两类只有幽灵那条进扫描结果——处置动作不同，必须能分开。
        let ghosts = scan_ghost_references(&state);
        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0].missing, "ghost");
    }

    #[test]
    fn test_check_blocked_terminal_dep_not_reported_missing() {
        // 反向：依赖真实存在且终态时幽灵扫描结果必须为空（防判据写成「不在 resolved 里
        // 就算幽灵」这类等价变异）。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("dep".to_owned(), module_with_status(ModuleStatus::Done));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert!(scan_ghost_references(&state).is_empty());
        assert!(results[0].ready);
    }

    #[test]
    fn test_composite_member_key_is_not_a_ghost_reference() {
        // 回归：composite 组的**非代表成员** key 不在 `modules` 表里（decompose 折叠后
        // 只登记组代表），但它不是幽灵——实体真实存在、登记在组代表名下。
        // 不做归一就会误报，且给出的「重跑 graph build + populate-modules」对该场景是
        // 无效动作（成员本就不会进 modules 表）。
        let mut state = minimal_init_state();
        let mut group = module_with_status(ModuleStatus::Translating);
        group.member_files = Some(vec![
            "file:emitter.ts".to_owned(),
            "file:handler.ts".to_owned(),
        ]);
        state.modules.insert("file:emitter.ts".to_owned(), group);

        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:handler.ts".to_owned()]);
        state.modules.insert("file:shared.ts".to_owned(), blocked);

        let warnings = validate_state(&state).expect("合法 state");
        // 按被引 key 判定而非「幽灵引用」措辞：这是**否定**断言，若改文案后措辞不再
        // 出现，按措辞匹配会恒真、静默失去区分力。
        assert!(
            !warnings.iter().any(|w| w.contains("file:handler.ts")),
            "组成员 key 不得被误报为幽灵引用: {warnings:?}"
        );
        // 同时直接查扫描结果，不依赖告警文案。
        assert!(
            scan_ghost_references(&state).is_empty(),
            "组成员 key 不得进入幽灵扫描结果"
        );

        let results = check_blocked_modules(&state);
        let target = results
            .iter()
            .find(|r| r.module == "file:shared.ts")
            .unwrap();
        assert!(
            scan_ghost_references(&state).is_empty(),
            "组成员 key 不得被当成幽灵引用"
        );
        // 归一后按**组代表**的状态判定：emitter 组是 translating（非终态）→ 仍阻塞。
        assert_eq!(target.unresolved, vec!["file:handler.ts".to_owned()]);
        assert!(!target.ready);
    }

    #[test]
    fn test_composite_member_ref_resolves_when_group_is_terminal() {
        // 承上：归一的意义不止「不误报」，还要按**组代表**的真实状态判就绪。
        // 组进终态后，引用其成员 key 的模块应能解除——不做归一则永远解除不了。
        let mut state = minimal_init_state();
        let mut group = module_with_status(ModuleStatus::Done);
        group.member_files = Some(vec![
            "file:emitter.ts".to_owned(),
            "file:handler.ts".to_owned(),
        ]);
        state.modules.insert("file:emitter.ts".to_owned(), group);

        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:handler.ts".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("file:shared.ts".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        let target = results
            .iter()
            .find(|r| r.module == "file:shared.ts")
            .unwrap();
        // 回显保留原始 dep 字符串（用户在 state 里写的就是它），不换成组代表。
        assert_eq!(target.resolved, vec!["file:handler.ts".to_owned()]);
        assert!(target.unresolved.is_empty());
        assert!(target.ready, "组已终态，引用其成员的模块应可解除");
    }

    #[test]
    fn test_validate_warns_on_ghost_blocked_by_reference() {
        // 幽灵引用必须告警：此前 validate_state 返 valid:true 零告警，模块永久阻塞
        // 而编排器无从察觉（MDR-021 待办 1）。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:ghost.ts".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), blocked);

        let warnings = validate_state(&state).expect("幽灵引用只告警，不得硬判损坏");
        // 按「点名了被引 key」定位告警，而非匹配「幽灵引用」这类措辞——文案会改，
        // 而「告警里必须能看到是哪个 key」是这条守卫真正要保的性质。
        let hit = warnings
            .iter()
            .find(|w| w.contains("file:ghost.ts"))
            .expect("应就幽灵引用告警");
        // 告警须同时点明引用方与被引 key——只说「存在幽灵引用」定位不到现场。
        assert!(hit.contains("file:a.ts"), "告警缺引用方: {hit}");
        // 处置须给出**真实存在且能跑**的命令。断言绑定到 `REPAIR_GHOST_COMMAND` 常量而非
        // 手写字符串：e2e 用同一个常量构造 argv 真跑（`smoke_state_repair_*`），故文案与
        // 「照做能不能成」不可能各说各话。
        //
        // 这条断言此前写的是 `hit.contains("populate-modules")`，注释声称它保的是「告警须
        // 给出重新同步的处置」——而文案里 `populate-modules` 一直是**否定**用法（「不要用」），
        // 子串断言在两种相反语义下都过。同型的空断言在本功能上被审查抓过一次（MDR-021
        // 第二轮的 `!advice.contains("populate-modules 同步")` 恒真），此处一并订正。
        assert!(
            hit.contains(REPAIR_GHOST_COMMAND),
            "告警须给出处置命令 `{REPAIR_GHOST_COMMAND}`: {hit}"
        );
        // 「不要靠等待」这条反面指引也须在（等待是这类引用最容易踩的错误动作）。
        assert!(
            hit.contains("不要靠等待"),
            "告警须否掉「等待依赖就绪」: {hit}"
        );
    }

    #[test]
    fn test_validate_scans_ghost_refs_on_non_blocked_modules() {
        // 扫描不限 blocked 模块：正常路径离开 blocked 会清空 blocked_by，但手工编辑
        // 或旧文件可能残留；一旦该模块再被标 blocked 就会立刻踩中同一个坑。
        // check_blocked_modules 只看 blocked 模块，故这条只能由 validate_state 兜住。
        let mut state = minimal_init_state();
        let mut translating = module_with_status(ModuleStatus::Translating);
        translating.blocked_by = Some(vec!["file:ghost.ts".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), translating);

        let warnings = validate_state(&state).expect("不得硬判损坏");
        assert!(
            warnings.iter().any(|w| w.contains("file:ghost.ts")),
            "非 blocked 模块上的残留幽灵引用同样须告警: {warnings:?}"
        );
    }

    #[test]
    fn test_validate_no_ghost_warning_when_refs_resolve() {
        // 反向不误报：blocked_by 全部指向已登记模块时不得有幽灵告警，
        // 否则守卫会在正常 state 上长期报噪、最终被忽略。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("dep".to_owned(), module_with_status(ModuleStatus::Done));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        state.modules.insert("target".to_owned(), blocked);

        let warnings = validate_state(&state).expect("合法 state");
        // 直接查扫描结果，不按告警措辞判定：否定断言若依赖措辞，文案一改就恒真；
        // 而按模块名做子串匹配同样不可靠——本例中 `dep` 恰好是签批告警文本的子串。
        assert!(
            scan_ghost_references(&state).is_empty(),
            "合法 blocked_by 不应产生幽灵引用"
        );
        assert!(
            !warnings.iter().any(|w| w.contains("blocked_by 指向")),
            "合法 blocked_by 不应触发幽灵告警: {warnings:?}"
        );
    }

    #[test]
    fn test_cycle_detection_follows_member_key_normalization() {
        // 回归（主审 imp2，归一引入的新盲区）：`check_blocked_modules` 归一了而
        // `detect_blocked_cycles` 按原始字符串建边时，「经成员 key 表达的互锁」会
        // **完全静默**——两侧都判成合法未终态依赖（无幽灵告警），而成员 key 不在
        // blocked_set 里、边被丢弃（无环告警），模块永久阻塞且零诊断。
        // 归一前这至少还会报幽灵告警，故属归一引入的回归。
        let mut state = minimal_init_state();
        let mut group = module_with_status(ModuleStatus::Blocked);
        group.member_files = Some(vec![
            "file:emitter.ts".to_owned(),
            "file:handler.ts".to_owned(),
        ]);
        // 组 blocked_by 普通模块 shared。
        group.blocked_by = Some(vec!["file:shared.ts".to_owned()]);
        state.modules.insert("file:emitter.ts".to_owned(), group);

        // shared blocked_by 组的**成员** key（而非组代表）→ 构成互锁。
        let mut shared = module_with_status(ModuleStatus::Blocked);
        shared.blocked_by = Some(vec!["file:handler.ts".to_owned()]);
        state.modules.insert("file:shared.ts".to_owned(), shared);

        let cycles = detect_blocked_cycles(&state);
        assert!(
            !cycles.is_empty(),
            "经成员 key 表达的互锁必须被环检测看见，否则是零诊断的永久死锁"
        );
        // 环里出现的是归一后的组代表。
        let flat: Vec<&String> = cycles.iter().flatten().collect();
        assert!(
            flat.iter().any(|m| *m == "file:emitter.ts")
                && flat.iter().any(|m| *m == "file:shared.ts"),
            "环路径应含两个互锁模块: {cycles:?}"
        );
    }

    #[test]
    fn test_member_in_multiple_groups_never_judged_ready() {
        // 回归（主审 imp3，归一引入）：同一文件被多个组列为成员时，静默取字典序最小
        // 的宿主会按**错误**的组判就绪。实证：X 同属 done 组与 translating 组时，
        // min() 取到 done 那组 → ready → `--auto-unblock` 真的把模块解除并落盘。
        // 这与「不在损坏数据上改状态」直接相反，只是入口换成坏划分而非幽灵 key。
        let mut state = minimal_init_state();
        let mut done_group = module_with_status(ModuleStatus::Done);
        done_group.member_files = Some(vec!["file:a.ts".to_owned(), "file:X.ts".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), done_group);

        let mut active_group = module_with_status(ModuleStatus::Translating);
        active_group.member_files = Some(vec!["file:z.ts".to_owned(), "file:X.ts".to_owned()]);
        state.modules.insert("file:z.ts".to_owned(), active_group);

        let mut victim = module_with_status(ModuleStatus::Blocked);
        victim.blocked_by = Some(vec!["file:X.ts".to_owned()]);
        victim.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("file:victim.ts".to_owned(), victim);

        let results = check_blocked_modules(&state);
        let target = results
            .iter()
            .find(|r| r.module == "file:victim.ts")
            .unwrap();
        assert!(
            !target.ready,
            "宿主歧义时不得判就绪（宿主组状态各异，挑错即判错）"
        );
        assert_eq!(target.unresolved, vec!["file:X.ts".to_owned()]);
        // 不是幽灵——处置动作是修组划分，不是重新同步 state。
        assert!(
            scan_ghost_references(&state).is_empty(),
            "坏划分不应被当成幽灵引用"
        );

        // 校验命令必须报出这个不变量被破坏（machine.rs 对同一不变量是 release 硬错，
        // 这里不能硬错但绝不能沉默）。
        let warnings = validate_state(&state).expect("旧文件须可读，不硬判损坏");
        let hit = warnings
            .iter()
            .find(|w| w.contains("跨组互斥"))
            .expect("坏划分须告警");
        assert!(hit.contains("file:X.ts"), "告警须点名冲突文件: {hit}");
        assert!(
            hit.contains("file:a.ts") && hit.contains("file:z.ts"),
            "告警须列出全部宿主组: {hit}"
        );
    }

    #[test]
    fn test_registered_module_also_owned_by_group_never_judged_ready() {
        // MDR-023 核心修复：被引 key **既是登记模块、又被别的组列为成员**。归一此前在
        // 「已是登记模块」处早返回，于是这一整类跨组破坏判成合法 `Resolved`。
        //
        // 后果不止漏诊断而是数据损坏——编排器独立复现（`/tmp` 真实 CLI）：`file:shared.ts`
        // 登记为 `done` 而 `g1` 组是 `translating` 时，`validate state` 零跨组告警、
        // `--auto-unblock` 真的解除了 holder（`blocked → translating`）并落盘，而该文件
        // 实际还在 g1 组里翻译中。
        let mut state = minimal_init_state();
        state.modules.insert(
            "file:shared.ts".to_owned(),
            module_with_status(ModuleStatus::Done),
        );
        let mut group = module_with_status(ModuleStatus::Translating);
        group.member_files = Some(vec!["g1".to_owned(), "file:shared.ts".to_owned()]);
        state.modules.insert("g1".to_owned(), group);

        let mut holder = module_with_status(ModuleStatus::Blocked);
        holder.blocked_by = Some(vec!["file:shared.ts".to_owned()]);
        holder.pre_blocked_status = Some(ModuleStatus::Translating);
        state.modules.insert("holder".to_owned(), holder);

        let target = check_blocked_modules(&state)
            .into_iter()
            .find(|r| r.module == "holder")
            .expect("holder 是 blocked 模块");
        assert!(
            !target.ready,
            "宿主不唯一时不得判就绪——`done` 那个宿主不代表组里那份也完成了"
        );
        assert_eq!(target.unresolved, vec!["file:shared.ts".to_owned()]);
        // 不是幽灵：实体存在，坏的是划分，处置是修 member_files 而非重新同步 state。
        assert!(
            scan_ghost_references(&state).is_empty(),
            "坏划分不得被当成幽灵引用"
        );

        let warnings = validate_state(&state).expect("旧文件须可读，不硬判损坏");
        let hit = warnings
            .iter()
            .find(|w| w.contains("跨组互斥"))
            .expect("坏划分须告警");
        assert!(
            hit.contains("file:shared.ts") && hit.contains("g1"),
            "告警须列出全部宿主（含它自己那一份）: {hit}"
        );
    }

    #[test]
    fn test_broken_partition_warns_without_any_blocked_by_reference() {
        // 检出不得依赖「恰好有某个 blocked_by 引用到它」这条路径撞见（MDR-023）：此前跨组
        // 告警是在遍历各模块 `blocked_by` 时顺带发现的，于是破坏存在而无人引用时
        // `validate state` 一声不响，而下一步对该模块的 transition/reset 会硬错——体检说
        // 健康、动手就报错。本用例全程没有任何 `blocked_by`。
        let mut state = minimal_init_state();
        let mut g1 = module_with_status(ModuleStatus::Pending);
        g1.member_files = Some(vec!["g1".to_owned(), "file:both.ts".to_owned()]);
        let mut g2 = module_with_status(ModuleStatus::Pending);
        g2.member_files = Some(vec!["g2".to_owned(), "file:both.ts".to_owned()]);
        state.modules.insert("g1".to_owned(), g1);
        state.modules.insert("g2".to_owned(), g2);
        assert!(
            state.modules.values().all(|m| m.blocked_by.is_none()),
            "本用例的前提是零 blocked_by"
        );

        let warnings = validate_state(&state).expect("旧文件须可读");
        let hit = warnings
            .iter()
            .find(|w| w.contains("跨组互斥"))
            .expect("无人引用的坏划分同样须告警");
        assert!(hit.contains("file:both.ts"), "须点名冲突文件: {hit}");
    }

    #[test]
    fn test_valid_partition_produces_no_broken_partition_warning() {
        // 反向不误报：`populate-modules` 落的 `member_files` **含组代表自身**，若把这份自引用
        // 计为一个宿主，每个正常 composite 组都会被报成破坏（本次修复最容易踩的反向坑）。
        let mut state = minimal_init_state();
        let mut group = module_with_status(ModuleStatus::Translating);
        group.member_files = Some(vec!["grp".to_owned(), "file:helper.ts".to_owned()]);
        state.modules.insert("grp".to_owned(), group);
        state
            .modules
            .insert("solo".to_owned(), module_with_status(ModuleStatus::Pending));

        let warnings = validate_state(&state).expect("合法划分须可读");
        assert!(
            !warnings.iter().any(|w| w.contains("跨组互斥")),
            "合法划分不得报破坏: {warnings:?}"
        );
    }

    #[test]
    fn test_ghost_scan_output_is_deterministically_ordered() {
        // 回归（主审 nit）：`scan_ghost_references` 的排序是告警文本可复现的唯一手段，
        // 但此前无测试构造 ≥2 条去验证输出序——摘掉排序时全部测试仍绿，而症状是
        // 低频 flaky（modules 是 HashMap，迭代序不定），属最难查的一类。
        let mut state = minimal_init_state();
        for (m, dep) in [
            ("file:c.ts", "file:GHOST_2.ts"),
            ("file:a.ts", "file:GHOST_9.ts"),
            ("file:b.ts", "file:GHOST_1.ts"),
        ] {
            let mut blocked = module_with_status(ModuleStatus::Blocked);
            blocked.blocked_by = Some(vec![dep.to_owned()]);
            state.modules.insert(m.to_owned(), blocked);
        }
        // 同一模块内多条引用也须有序。
        let mut multi = module_with_status(ModuleStatus::Blocked);
        multi.blocked_by = Some(vec![
            "file:GHOST_z.ts".to_owned(),
            "file:GHOST_y.ts".to_owned(),
        ]);
        state.modules.insert("file:d.ts".to_owned(), multi);

        // 多跑几轮：单轮偶然有序无法排除 HashMap 迭代序碰巧的情况。
        let expected: Vec<(String, String)> = vec![
            ("file:a.ts".into(), "file:GHOST_9.ts".into()),
            ("file:b.ts".into(), "file:GHOST_1.ts".into()),
            ("file:c.ts".into(), "file:GHOST_2.ts".into()),
            ("file:d.ts".into(), "file:GHOST_y.ts".into()),
            ("file:d.ts".into(), "file:GHOST_z.ts".into()),
        ];
        for _ in 0..8 {
            let got: Vec<(String, String)> = scan_ghost_references(&state)
                .into_iter()
                .map(|g| (g.module, g.missing))
                .collect();
            assert_eq!(got, expected, "输出须按 (module, missing) 字典序稳定");
        }
    }

    #[test]
    fn test_ghost_scan_dedups_repeated_reference() {
        // 同一 blocked_by 里重复列同一个 key：去重后只报一条，否则告警里同一条现两次。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:GHOST.ts".to_owned(), "file:GHOST.ts".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), blocked);

        assert_eq!(scan_ghost_references(&state).len(), 1, "重复引用须去重");
    }

    #[test]
    fn test_ghost_scan_marks_blocked_vs_residual() {
        // 回归（异构交叉 imp3）：告警扫全部模块、`ghost_refs` 却只取 blocked 模块时，
        // 非 blocked 模块的幽灵引用会「warnings 报了但机读字段是空数组」，只消费
        // `data.ghost_refs` 的编排器直接漏掉。两侧现共用本函数。
        //
        // 同时钉住两类的区分：对 pending 模块断言「引用方将永久阻塞」是当下失实的——
        // 它此刻并没有被阻塞。区分现由 `status` 承载（早期是一个 `module_blocked: bool`，
        // 被证明扛不住多个正交维度——见 MDR-021「第三轮四视角」段）。据 `status` 推导
        // **处置命令**的那一层已被审查推翻、拆出为后续 PR，本测试只验检出与分述。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:GHOST_A.ts".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), blocked);
        let mut pending = module_with_status(ModuleStatus::Pending);
        pending.blocked_by = Some(vec!["file:GHOST_B.ts".to_owned()]);
        state.modules.insert("file:b.ts".to_owned(), pending);

        let ghosts = scan_ghost_references(&state);
        assert_eq!(ghosts.len(), 2, "两类模块都须被扫出: {ghosts:?}");
        // 结果按 (module, missing) 排序，故 a 在前。
        assert_eq!(ghosts[0].module, "file:a.ts");
        assert_eq!(ghosts[0].status, ModuleStatus::Blocked, "须回带持有方状态");
        assert_eq!(ghosts[1].module, "file:b.ts");
        assert_eq!(
            ghosts[1].status,
            ModuleStatus::Pending,
            "pending 模块不得被标为已阻塞"
        );

        // 告警文本须分述两类，不能把 pending 说成永久阻塞。
        let warnings = validate_state(&state).expect("不得硬判损坏");
        let w = warnings
            .iter()
            .find(|w| w.contains("file:GHOST_A.ts"))
            .expect("应告警");
        assert!(w.contains("file:GHOST_B.ts"), "两条都应在同一告警内: {w}");
        assert!(w.contains("当前非 blocked"), "须分述残留类: {w}");
    }

    #[test]
    fn test_ghost_warning_quotes_keys_unambiguously() {
        // 回归（异构交叉 nit）：key 来自可被手工编辑的 state，含反引号/换行时，
        // 裸反引号包裹会让告警里的条目边界可被内容伪造。改用 JSON 字面量转义。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:spoof`、`other\nnext".to_owned()]);
        state.modules.insert("file:a.ts".to_owned(), blocked);

        let warnings = validate_state(&state).expect("不得硬判损坏");
        let w = warnings
            .iter()
            .find(|w| w.contains("spoof"))
            .expect("应告警");
        // 换行被转义成字面 \n，不会在告警里断行伪装成新条目。
        assert!(!w.contains('\n'), "告警不得含裸换行: {w:?}");
        assert!(w.contains("\\n"), "换行应转义可见: {w:?}");
    }

    #[test]
    fn test_auto_unblock_refuses_module_with_ghost_reference() {
        // 幽灵引用不得让模块判为「就绪可解除」，否则 `--auto-unblock` 会在损坏数据上
        // 真的改状态（把等不到的依赖当成已满足）。此性质当前由「`RefResolution::Missing`
        // 一律计入 `unresolved`」保证，这里钉死它，防将来「优化」ready 判定时被破坏。
        //
        // **承诺范围仅限本条路径**：`state transition` / `state update --cas-version`
        // 走 `transition_inner`，那里离开 blocked 只校验 `target == pre_blocked_status`、
        // 不校验依赖是否终态，故**能**在不带 --force 的情况下解除幽灵阻塞（异构交叉
        // 审查实证）。那是 pre-existing 行为（对所有 blocked 模块一视同仁，不限幽灵），
        // 收窄它会改变既有转换语义，已记账另议。本测试不为那条路径背书。
        let mut machine = MigrationStateMachine::init_new("test", SourceLang::TypeScript);
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["ghost".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        machine.update_module("target", blocked);

        let checks = check_blocked_modules(machine.state_file());
        assert!(!checks[0].ready, "带幽灵引用的模块不得判为就绪");

        let mut warnings = Vec::new();
        let unblocked = auto_unblock_modules(&mut machine, &checks, &mut warnings);
        assert!(unblocked.is_empty(), "不得自动解除带幽灵引用的模块");
        assert_eq!(
            machine.state_file().modules["target"].status,
            ModuleStatus::Blocked,
            "状态须保持 blocked 不变"
        );
    }

    #[test]
    fn test_check_blocked_empty_blocked_by() {
        // blocked_by 为空列表 → 无依赖，ready=true。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(Vec::new());
        blocked.pre_blocked_status = Some(ModuleStatus::Pending);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(results[0].ready);
    }

    #[test]
    fn test_auto_unblock_restores_pre_blocked_status() {
        // 自动解除：恢复到 pre_blocked_status。
        let mut machine = MigrationStateMachine::init_new("test", SourceLang::TypeScript);
        machine.update_module("dep", module_with_status(ModuleStatus::Done));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Translating);
        machine.update_module("target", blocked);

        let mut warnings = Vec::new();
        let checks = check_blocked_modules(machine.state_file());
        let unblocked = auto_unblock_modules(&mut machine, &checks, &mut warnings);

        assert_eq!(unblocked, vec!["target".to_owned()]);
        assert!(warnings.is_empty());
        assert_eq!(
            machine.state_file().modules["target"].status,
            ModuleStatus::Translating
        );
        assert!(machine.state_file().modules["target"].blocked_by.is_none());
        assert!(machine.state_file().modules["target"]
            .pre_blocked_status
            .is_none());
    }

    #[test]
    fn test_auto_unblock_defaults_to_pending() {
        // pre_blocked_status 缺失时默认恢复为 pending。
        let mut machine = MigrationStateMachine::init_new("test", SourceLang::TypeScript);
        machine.update_module("dep", module_with_status(ModuleStatus::Done));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        // 无 pre_blocked_status。
        machine.update_module("target", blocked);

        let mut warnings = Vec::new();
        let checks = check_blocked_modules(machine.state_file());
        let unblocked = auto_unblock_modules(&mut machine, &checks, &mut warnings);

        assert_eq!(unblocked, vec!["target".to_owned()]);
        assert_eq!(
            machine.state_file().modules["target"].status,
            ModuleStatus::Pending
        );
    }

    #[test]
    fn test_auto_unblock_skips_not_ready() {
        // 依赖未终态的 blocked 模块不被解除。
        let mut machine = MigrationStateMachine::init_new("test", SourceLang::TypeScript);
        machine.update_module("dep", module_with_status(ModuleStatus::Translating));
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["dep".to_owned()]);
        machine.update_module("target", blocked);

        let mut warnings = Vec::new();
        let checks = check_blocked_modules(machine.state_file());
        let unblocked = auto_unblock_modules(&mut machine, &checks, &mut warnings);

        assert!(unblocked.is_empty());
        assert_eq!(
            machine.state_file().modules["target"].status,
            ModuleStatus::Blocked
        );
    }

    #[test]
    fn test_detect_blocked_cycles_no_cycle() {
        // A blocked_by B, B 是 done → 无环。
        let mut state = minimal_init_state();
        state
            .modules
            .insert("b".to_owned(), module_with_status(ModuleStatus::Done));
        let mut a = module_with_status(ModuleStatus::Blocked);
        a.blocked_by = Some(vec!["b".to_owned()]);
        state.modules.insert("a".to_owned(), a);

        let cycles = detect_blocked_cycles(&state);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_detect_blocked_cycles_mutual() {
        // A blocked_by B, B blocked_by A → 互相阻塞环。
        let mut state = minimal_init_state();
        let mut a = module_with_status(ModuleStatus::Blocked);
        a.blocked_by = Some(vec!["b".to_owned()]);
        let mut b = module_with_status(ModuleStatus::Blocked);
        b.blocked_by = Some(vec!["a".to_owned()]);
        state.modules.insert("a".to_owned(), a);
        state.modules.insert("b".to_owned(), b);

        let cycles = detect_blocked_cycles(&state);
        assert!(!cycles.is_empty(), "应检测到互相阻塞环");
    }

    #[test]
    fn test_detect_blocked_cycles_self() {
        // A blocked_by A → 自依赖环。
        let mut state = minimal_init_state();
        let mut a = module_with_status(ModuleStatus::Blocked);
        a.blocked_by = Some(vec!["a".to_owned()]);
        state.modules.insert("a".to_owned(), a);

        let cycles = detect_blocked_cycles(&state);
        assert!(!cycles.is_empty(), "应检测到自依赖环");
    }

    #[test]
    fn test_detect_blocked_cycles_chain() {
        // A→B→C→A 三元环。
        let mut state = minimal_init_state();
        let mut a = module_with_status(ModuleStatus::Blocked);
        a.blocked_by = Some(vec!["b".to_owned()]);
        let mut b = module_with_status(ModuleStatus::Blocked);
        b.blocked_by = Some(vec!["c".to_owned()]);
        let mut c = module_with_status(ModuleStatus::Blocked);
        c.blocked_by = Some(vec!["a".to_owned()]);
        state.modules.insert("a".to_owned(), a);
        state.modules.insert("b".to_owned(), b);
        state.modules.insert("c".to_owned(), c);

        let cycles = detect_blocked_cycles(&state);
        assert!(!cycles.is_empty(), "应检测到三元环");
    }

    #[test]
    fn test_detect_blocked_cycles_ignores_non_blocked() {
        // A blocked_by B，但 B 不是 blocked（是 translating）→ 不形成环。
        let mut state = minimal_init_state();
        let mut a = module_with_status(ModuleStatus::Blocked);
        a.blocked_by = Some(vec!["b".to_owned()]);
        state.modules.insert("a".to_owned(), a);
        state.modules.insert(
            "b".to_owned(),
            module_with_status(ModuleStatus::Translating),
        );

        let cycles = detect_blocked_cycles(&state);
        assert!(cycles.is_empty());
    }
}
