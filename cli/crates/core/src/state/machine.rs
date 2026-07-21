//! 状态机实现。
//!
//! 管理 `migration-state.json` 的生命周期：创建、加载、保存、状态转换。

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::types::common::{SourceLang, Timestamp};
use crate::types::config::PersistenceConfig;
use crate::types::state::{
    AttemptRecord, MigrationMetadata, MigrationStateFile, ModuleState, ModuleStatus, ProjectInfo,
    ProjectState, StateHistoryEntry,
};

/// 状态文件 schema 版本号（init 时写入 `migration-state.json` 的 `version` 字段）。
///
/// 采用语义化版本：**主版本号**用于 schema 不兼容判定（见 `validate::version` 兼容性检查），
/// 同主版本视为可读，跨主版本视为不兼容、拒绝加载。变更 schema 破坏性结构时递增主版本号。
pub const STATE_SCHEMA_VERSION: &str = "1.0.0";

/// 模块 substatus 值：agent 级自检完成（两层 done 协议）。
pub const SUBSTATUS_AGENT_DONE: &str = "agent_done";

/// 迁移状态机，持有并管理 `MigrationStateFile`。
#[derive(Debug, Clone)]
pub struct MigrationStateMachine {
    /// 内部状态文件数据。
    state_file: MigrationStateFile,
    /// 运行时标志（不序列化）：本次 [`load`](Self::load) 是否因主文件损坏而回退到 `.backup`。
    /// 为真表示拿到的是上一次成功保存前的旧状态，最近进度可能丢失，调用方应向用户告警。
    recovered_from_backup: bool,
    /// 持久化配置（运行时注入，不序列化）。控制 save 时是否备份、过期清理策略。
    persistence_config: PersistenceConfig,
}

/// [`record_metrics`](MigrationStateMachine::record_metrics) 的写入结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMetricsOutcome {
    /// 归一后的组代表 module key。
    pub module: String,
    /// 写入完成后的实际测试通过率（部分更新时保留旧值）。
    pub test_pass_rate: Option<String>,
    /// 写入完成后的实际已知差异数。
    pub known_differences: u32,
}

/// [`reset_module`](MigrationStateMachine::reset_module) 的结果——描述这次幂等回退把模块从
/// 什么状态回退到什么状态，以及是否为空操作（已处于干净重译入口，重复 reset 无副作用）。
#[derive(Debug, Clone, PartialEq)]
pub struct ResetOutcome {
    /// 归一后的组代表 module key（入参传非代表成员时为其所属组代表）。
    pub module: String,
    /// 回退前的模块状态。
    pub reset_from: ModuleStatus,
    /// 回退后的模块状态（`pending` 保持不变，其余均为 `translating`）。
    pub reset_to: ModuleStatus,
    /// 是否为幂等空操作（模块已处于干净重译入口，本次未改动任何字段、未追加审计）。
    /// 为真时调用方可省略 save。
    pub was_noop: bool,
    /// 模块的源文件作用域（NodeId）：composite 组 → `member_files`；单文件 → `[module]`。
    /// CLI 层据此构造产物清理指令（CLI 不猜 rust_root 路径删 `.rs`，见 MDR-015）。
    pub member_files: Vec<String>,
}

/// [`recover_module`](MigrationStateMachine::recover_module) 的 stall 恢复策略（M4-ROB-01b）。
///
/// 编排器检测到 agent 静默超时（watchdog stall）后，据 `[orchestration].stall_recovery_policy`
/// 配置 + 自身 retry-round 计数解析出本次策略，显式传入 CLI（CLI 不读 config、不计数，确定性执行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverPolicy {
    /// 重试：回退到干净重译入口（复用 [`reset_module`](MigrationStateMachine::reset_module)
    /// 语义），供编排器重派翻译。
    Retry,
    /// 跳过：置 `paused` 决策点（headless 由既有编排 prose 自动 `degrade_skip`；交互态待人类抉择）。
    Skip,
}

impl RecoverPolicy {
    /// 策略的稳定字符串标识（用于审计记录 / CLI 输出）。
    pub fn as_str(self) -> &'static str {
        match self {
            RecoverPolicy::Retry => "retry",
            RecoverPolicy::Skip => "skip",
        }
    }
}

/// [`recover_module`](MigrationStateMachine::recover_module) 的结果——描述 stall 恢复把模块从
/// 什么状态恢复到什么状态、应用了哪个策略、是否为幂等空操作。
#[derive(Debug, Clone, PartialEq)]
pub struct RecoverOutcome {
    /// 归一后的组代表 module key（入参传非代表成员时为其所属组代表）。
    pub module: String,
    /// 本次应用的恢复策略。
    pub policy: RecoverPolicy,
    /// 恢复前的模块状态。
    pub from: ModuleStatus,
    /// 恢复后的模块状态（retry → `translating`/`pending`；skip → `paused`）。
    pub to: ModuleStatus,
    /// 是否为幂等空操作（重复 recover 无副作用，调用方可省略 save）。
    pub was_noop: bool,
    /// 模块的源文件作用域（NodeId）：composite → `member_files`；单文件 → `[module]`。
    /// retry 时 CLI 据此构造产物清理指令（同 [`ResetOutcome::member_files`]）。
    pub member_files: Vec<String>,
}

/// [`resume_plan`](MigrationStateMachine::resume_plan) 的结果——额度耗尽/中断后续跑的**断点计划**
/// （M4-ROB-01c）。
///
/// 纯查询产物：把当前 state 的模块按状态归桶，供编排器决定「哪些幂等重入、哪些不重跑、下一步做谁」。
/// 本结构**不含任何 mutation**——实际的中途模块重入复用 `state recover --policy retry`
/// （见 [`recover_module`](MigrationStateMachine::recover_module)）。检测（额度逼近）归编排器/harness，
/// CLI 只据已 checkpoint 的 state（ROB-01a 原子持久化）产出计划。见 MDR-017。
#[derive(Debug, Clone, PartialEq)]
pub struct ResumePlan {
    /// 当前 sprint 号（`sprint.current`）；无 sprint 状态时为 `None`。仅供上下文展示。
    pub sprint: Option<u32>,
    /// 被中断的**运行态**模块（translating/compile_fixing/testing/reviewing）——需幂等重入。
    /// 按 module key 字典序排序（`modules` 是 HashMap，排序保输出确定性）。
    pub interrupted: Vec<InterruptedModule>,
    /// `paused` 决策点模块——待人类/降级决策，续跑**不复活**（不给 retry 命令）。字典序。
    pub awaiting_decision: Vec<String>,
    /// `pending` 模块——下一步候选（编排器用 `state deps <M>` 判就绪后推进）。字典序。
    pub next: Vec<String>,
    /// `blocked` 模块——等依赖。字典序。
    pub blocked: Vec<String>,
    /// `done`（真终态）模块数——不重跑，仅计入进度。**不可从上述列表派生**（终态模块不入任何列表）。
    pub done: usize,
    /// `degrade_*`（降级终态：ffi/manual/skip）模块数——不重跑，仅计入进度。同样不可派生。
    pub degraded: usize,
}

impl ResumePlan {
    /// 按需派生进度计数快照（单一真相源：列表长度 + `done`/`degraded`）。
    ///
    /// 不独立存储，避免与列表计数形成可各自变动的第二真相源（见 MDR-017 审查加固）。
    pub fn progress(&self) -> ResumeProgress {
        let in_progress = self.interrupted.len();
        let pending = self.next.len();
        let blocked = self.blocked.len();
        let awaiting_decision = self.awaiting_decision.len();
        ResumeProgress {
            done: self.done,
            degraded: self.degraded,
            in_progress,
            pending,
            blocked,
            awaiting_decision,
            total: self.done + self.degraded + in_progress + pending + blocked + awaiting_decision,
        }
    }
}

/// [`ResumePlan`] 中被中断的单个运行态模块。
#[derive(Debug, Clone, PartialEq)]
pub struct InterruptedModule {
    /// 模块 key（组代表 NodeId）。
    pub module: String,
    /// 中断时的运行态状态（translating/compile_fixing/testing/reviewing）。
    pub status: ModuleStatus,
    /// 源文件作用域（composite → `member_files`；单文件 → `[module]`）。
    /// 编排器 retry 时据此清理部分 `.rs` 产物（同 [`RecoverOutcome::member_files`]）。
    pub member_files: Vec<String>,
}

/// [`ResumePlan`] 的进度计数快照——由 [`ResumePlan::progress`] **按需派生**，非独立存储的第二真相源。
///
/// 四个列表桶计数直接取各 `Vec` 长度，`done`/`degraded` 取 [`ResumePlan`] 的两个不可派生字段，
/// `total` 为六桶之和（恒等于 `modules.len()`）。CLI 层用它拼扁平 JSON `progress` 对象。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResumeProgress {
    /// `done` 模块数（真终态）。
    pub done: usize,
    /// `degrade_*` 模块数（降级终态：ffi/manual/skip）。
    pub degraded: usize,
    /// 运行态模块数（= `interrupted` 长度）。
    pub in_progress: usize,
    /// `pending` 模块数（= `next` 长度）。
    pub pending: usize,
    /// `blocked` 模块数（= `blocked` 长度）。
    pub blocked: usize,
    /// `paused` 决策点模块数（= `awaiting_decision` 长度）。
    pub awaiting_decision: usize,
    /// 全部模块数（= 上述各桶之和）。
    pub total: usize,
}

impl MigrationStateMachine {
    /// 从文件加载状态。
    ///
    /// 如果文件不存在返回 `MigrateError::FileNotFound`。
    ///
    /// **仅当主文件 JSON 解析失败（数据损坏）时**才回退到 `.backup`——这是应对
    /// 崩溃/并发写入残留半截文件的兜底。I/O / 权限等非损坏错误**直接上抛**，
    /// 不被回退掩盖（否则临时 I/O 故障会让调用方静默读到过期状态）。
    /// 主备双双损坏时，返回**主文件**的错误（primary），保留真正的故障现场。
    ///
    /// 注意：回退到 backup 意味着拿到的是**上一次成功保存前**的旧状态，最近一次
    /// 保存的进度可能丢失。load 本身不自愈主文件（损坏文件残留，依赖下次 save 覆盖）。
    /// 回退发生时 [`recovered_from_backup`](Self::recovered_from_backup) 置真，CLI 接线据此
    /// 经统一响应向用户告警「已从 backup 恢复、最近进度可能丢失」。
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(MigrateError::FileNotFound(path.to_path_buf()));
        }
        match Self::load_file(path) {
            Ok(machine) => Ok(machine),
            // 仅数据损坏（JSON 解析失败）才回退 backup；其余错误直接上抛。
            Err(primary @ MigrateError::Json(_)) => {
                let backup = sibling_with_suffix(path, ".backup");
                // backup 不存在时 load_file 返回 Io 错误，与"backup 也损坏"一并落入
                // 兜底：返回 primary 错误，不掩盖主文件故障现场。
                Self::load_file(&backup)
                    .map(Self::mark_recovered)
                    .map_err(|_| primary)
            }
            Err(other) => Err(other),
        }
    }

    /// 标记本次加载来自 backup 回退（供 [`load`](Self::load) 内部使用）。
    fn mark_recovered(mut self) -> Self {
        self.recovered_from_backup = true;
        self
    }

    /// 本次 [`load`](Self::load) 是否因主文件损坏回退到 `.backup`。
    ///
    /// CLI 接线据此向用户告警「已从 backup 恢复、最近进度可能丢失」（经统一响应降级 warning）。
    pub fn recovered_from_backup(&self) -> bool {
        self.recovered_from_backup
    }

    /// 从指定路径读取并反序列化状态文件。
    fn load_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state_file: MigrationStateFile = serde_json::from_str(&content)?;
        // timestamp 格式校验已下沉到 Timestamp 的自定义 Deserialize（反序列化时即拒非法值），
        // 此处无需再手写遍历。
        Ok(Self {
            state_file,
            recovered_from_backup: false,
            persistence_config: PersistenceConfig::default(),
        })
    }

    /// 保存状态到文件（crash-safe）。
    ///
    /// 自动创建父目录；采用 tmp → fsync → 原子 rename，并同步父目录，
    /// 保证进程崩溃或并发写入中断时不会留下半截 JSON。覆盖前先备份 `.backup`，
    /// 供 [`load`](Self::load) 在主文件损坏时回退。
    ///
    /// **恢复后保存的特例**：若本实例来自 backup 回退（[`recovered_from_backup`](Self::recovered_from_backup)
    /// 为真），磁盘上的主文件仍是损坏内容——此时**跳过备份步骤**，避免用损坏的主文件覆盖唯一可用的
    /// `.backup`（否则 rename 前若再崩溃，主备双损、彻底不可恢复）。保留 backup 为回退前的最后有效快照。
    ///
    /// 备份行为受 `persistence_config.backup_on_write` 控制（默认 true，与既有行为一致）。
    /// `persistence_config.retention_days` 有值时，save 后清理超过 N 天的 `.backup` 文件。
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(&self.state_file)?;
        // 仅在 backup_on_write=true 且非恢复模式时备份（恢复模式跳过以防损坏覆盖有效 backup）。
        let do_backup = self.persistence_config.backup_on_write && !self.recovered_from_backup;
        atomic_write(path, content.as_bytes(), do_backup)?;
        // 按 retention_days 清理过期 backup。
        if let Some(days) = self.persistence_config.retention_days {
            cleanup_old_backups(path, days);
        }
        Ok(())
    }

    /// 执行状态转换。
    ///
    /// 校验 `ProjectState::can_transition_to`，合法则更新状态并记录历史，
    /// 否则返回 `MigrateError::InvalidTransition`。
    pub fn transition(&mut self, target: ProjectState) -> Result<()> {
        let current = self.state_file.state;
        if !current.can_transition_to(target) {
            return Err(MigrateError::InvalidTransition {
                from: current.to_string(),
                to: target.to_string(),
            });
        }

        let now = Timestamp::new(chrono::Utc::now().to_rfc3339());

        // 关闭当前状态的历史条目
        if let Some(last) = self.state_file.state_history.last_mut() {
            if last.exited_at.is_none() {
                last.exited_at = Some(now.clone());
            }
        }

        // 添加新状态的历史条目
        self.state_file.state_history.push(StateHistoryEntry {
            state: target,
            entered_at: now,
            exited_at: None,
        });

        self.state_file.state = target;
        Ok(())
    }

    /// 创建新的初始状态文件。
    pub fn init_new(project_name: &str, source_lang: SourceLang) -> Self {
        let now = Timestamp::new(chrono::Utc::now().to_rfc3339());

        let state_file = MigrationStateFile {
            version: STATE_SCHEMA_VERSION.to_owned(),
            state: ProjectState::Init,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: now.clone(),
                exited_at: None,
            }],
            project: Some(ProjectInfo {
                name: project_name.to_owned(),
                source_language: source_lang,
                source_commit: None,
                source_loc: 0,
                created_at: now,
            }),
            sprint: None,
            modules: std::collections::HashMap::new(),
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

        Self {
            state_file,
            recovered_from_backup: false,
            persistence_config: PersistenceConfig::default(),
        }
    }

    /// 注入持久化配置（运行时从 `.rustmigrate.toml` 读取后设置）。
    ///
    /// 控制 save 时是否生成 `.backup` 以及过期清理策略。
    /// 未调用此方法时使用默认配置（backup_on_write=true, retention_days=None）。
    pub fn set_persistence_config(&mut self, config: PersistenceConfig) {
        self.persistence_config = config;
    }

    /// 返回当前项目状态。
    pub fn current_state(&self) -> ProjectState {
        self.state_file.state
    }

    /// 获取内部状态文件的不可变引用。
    pub fn state_file(&self) -> &MigrationStateFile {
        &self.state_file
    }

    /// 标记 graph 构建完成。
    pub fn set_graph_build_completed(&mut self) {
        let now = Timestamp::new(chrono::Utc::now().to_rfc3339());
        let metadata = self.state_file.metadata.get_or_insert(MigrationMetadata {
            graph_build_completed: false,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
            version: 0,
            last_modified_by: None,
        });
        metadata.graph_build_completed = true;
        metadata.graph_build_completed_at = Some(now);
    }

    /// 登记/覆盖模块的完整状态记录（**不校验**状态转换合法性）。
    ///
    /// 仅用于首次登记模块或整体重建场景。运行时的状态流转应走
    /// [`transition_module`](Self::transition_module)，以免把 `done` 等终态非法改回 `pending`、
    /// 破坏断点续传语义。
    pub fn update_module(&mut self, name: &str, module: ModuleState) {
        self.state_file.modules.insert(name.to_owned(), module);
    }

    /// 清理"孤儿"模块：删除 key 不在 `live_keys` 中的模块条目，返回被删除的 key 列表。
    ///
    /// 用于 `populate-modules` 重填场景——源码图删文件后，上一轮 populate 登记的模块会变成
    /// 状态中存在、源码图已无对应节点的"孤儿"。整体重填只新增/覆盖图内节点，不会主动删除这些
    /// 残留，导致后续 `state report` / 依赖门禁把不存在的模块计入进度。本方法在重填前剔除它们，
    /// 保持 `modules` 与当前源码图迁移序列一致。
    ///
    /// 仅由调用方在确认全部模块仍为 `pending`（无活跃进度，断点续传安全）后调用。
    pub fn retain_modules(&mut self, live_keys: &std::collections::HashSet<String>) -> Vec<String> {
        let orphans: Vec<String> = self
            .state_file
            .modules
            .keys()
            .filter(|k| !live_keys.contains(*k))
            .cloned()
            .collect();
        for key in &orphans {
            self.state_file.modules.remove(key);
        }
        orphans
    }

    /// 执行模块级状态转换（带合法性校验、substatus/reason 落盘、blocked 恢复副作用）。
    ///
    /// 严格对齐 `docs/design/09-appendix-schemas.md` § 合法状态转换：
    ///
    /// - `to == Some(target)`：一律校验 [`ModuleStatus::can_transition_to`]（矩阵无自环，
    ///   故同态/终态 `--to` 也会按矩阵返回 `MigrateError::InvalidTransition`）；合法则更新
    ///   `status` 并按转换边执行副作用：
    ///   - 进入 `blocked`：记录 `pre_blocked_status = from`。
    ///   - 离开 `blocked`：须恢复到 `pre_blocked_status`（设计行 207/218）。已记录时强校验
    ///     `target == pre_blocked_status`，否则报 `InvalidTransition`；随后清除
    ///     `blocked_by` 与 `pre_blocked_status`。
    ///   - `degrade_* → translating`（设计行 379-381 `--force` 恢复）：**须 `force == true`**，
    ///     否则返回 `MigrateError::Config`（降级是人类决策，禁止脚本静默绕过）；通过后
    ///     清除 `substatus`、清空 `attempts`（重置重试计数，重新进入翻译循环）。
    /// - `to == None`：仅更新 substatus（status 不变），对应设计 Step 2/4 的 Phase 进度记录
    ///   （行 461/485 `state transition --module <M> --substatus <s>`）。
    /// - `substatus == Some(s)`：显式覆盖 `substatus`（设置在转换副作用之后，故可在
    ///   恢复转换的同时指定新的 substatus）。
    /// - `reason == Some(r)`：向 `attempts` 追加一条审计记录（模块级唯一 append-only
    ///   时间序列），`result` 形如 `transition:from→to reason=r`，供状态报告/排查回溯。
    /// - `force`：仅对 `degrade_* → translating` 恢复有意义（设计 `--force`），其余转换忽略。
    ///
    /// `name` 归一：容忍传入 composite 组的非代表成员（反查 `member_files` 映射回组代表），
    /// 与 `state deps` 的组感知一致。归一后仍找不到则返回 `MigrateError::Config`。
    pub fn transition_module(
        &mut self,
        name: &str,
        to: Option<ModuleStatus>,
        substatus: Option<&str>,
        reason: Option<&str>,
        force: bool,
    ) -> Result<()> {
        // 归一 module key（组非代表成员 → 组代表，见 canonical_module_key）。
        let canonical = self.canonical_module_key(name)?;
        let module = self
            .state_file
            .modules
            .get_mut(&canonical)
            .expect("canonical 已校验存在");
        let from = module.status;

        if let Some(target) = to {
            // 显式 --to 一律校验合法转换矩阵（无自环：target==from 也按矩阵判，
            // 故对终态/同态 --to 会正确报 InvalidTransition；幂等的「仅更新 substatus」
            // 走 to==None 路径，不经此处。设计行 501（Step 5）的「已是 testing 则跳过」由
            // 上游 SKILL 不发起该调用保证，CLI 不需支持同态 --to）。
            if !from.can_transition_to(target) {
                return Err(MigrateError::InvalidTransition {
                    from: from.to_string(),
                    to: target.to_string(),
                });
            }
            // degrade_* → translating 恢复须 --force（设计行 379-381：降级恢复是人类决策）。
            if from.is_degraded() && target == ModuleStatus::Translating && !force {
                return Err(MigrateError::Config(format!(
                    "{from} → translating 恢复需 --force（降级恢复须人类确认，见设计 § Step 0.3）"
                )));
            }
            // 进入 blocked：记录恢复锚点。
            if target == ModuleStatus::Blocked {
                module.pre_blocked_status = Some(from);
            }
            // 离开 blocked：须恢复到进入前状态（设计行 207/218）。已记录 pre_blocked_status
            // 时强校验 target == pre_blocked_status，避免恢复到错误状态丢失断点续传锚点；
            // 未记录时（如直接 update_module 造的 blocked）退化为只校验 blockable。
            if from == ModuleStatus::Blocked {
                if let Some(pre) = module.pre_blocked_status {
                    if target != pre {
                        return Err(MigrateError::InvalidTransition {
                            from: from.to_string(),
                            to: format!("{target}（blocked 须恢复到 pre_blocked_status={pre}）"),
                        });
                    }
                }
                module.blocked_by = None;
                module.pre_blocked_status = None;
            }
            // degrade_* → translating（--force 恢复）：清除 substatus + 重置 attempts。
            if from.is_degraded() && target == ModuleStatus::Translating {
                module.substatus = None;
                module.attempts.clear();
            }
            module.status = target;
            // graduate 到 Done：清空 testing/review 阶段残留的 substatus（如
            // phase_b_optimization_in_progress / incomplete_*），避免污染 review 仪表板。
            // 仅清 Done；degrade_*（DegradeFfi/Manual/Skip）的 substatus 含降级原因须保留。
            // 清空在前、显式 substatus 覆盖在后，故 done 时显式传 substatus 仍能设上。
            if target == ModuleStatus::Done {
                module.substatus = None;
            }
        }

        // 显式 substatus 覆盖（在转换副作用之后，允许恢复转换同时指定新 substatus）。
        if let Some(s) = substatus {
            module.substatus = Some(s.to_owned());
        }

        // reason 审计落盘：append 到 attempts。
        if let Some(r) = reason {
            let now = Timestamp::new(chrono::Utc::now().to_rfc3339());
            module.attempts.push(AttemptRecord {
                timestamp: now,
                result: format!("transition:{from}→{} reason={r}", module.status),
                retry_count: 0,
                checkpoint: None,
            });
        }
        Ok(())
    }

    /// 记录 verifier 产出的模块质量度量（M4-QUAL-05）。
    ///
    /// 各参数为 `Some` 时才覆盖对应字段，允许差异测试重跑后只更新单项结果。
    /// 仅运行态（translating/compile_fixing/testing/reviewing）可写；pending、终态、暂停/阻塞、
    /// 降级态及项目 graduate 均拒绝。每次成功写入都会追加审计记录。
    /// module key 归一同 [`transition_module`](Self::transition_module)。
    pub fn record_metrics(
        &mut self,
        name: &str,
        test_pass_rate: Option<&str>,
        known_differences: Option<u32>,
    ) -> Result<RecordMetricsOutcome> {
        if let Some(rate) = test_pass_rate {
            if crate::stats::quality::parse_test_pass_rate(rate).is_none() {
                return Err(MigrateError::Config(format!(
                    "非法 test_pass_rate: {rate}（支持 85% / 0.85 / 85 / 85/100，且须落在 [0,1]）"
                )));
            }
        }

        if self.state_file.state == ProjectState::Graduate {
            return Err(MigrateError::Config(
                "项目已 graduate，禁止改写模块质量度量".to_string(),
            ));
        }

        let canonical = self.canonical_module_key(name)?;
        let module = self
            .state_file
            .modules
            .get_mut(&canonical)
            .expect("canonical 已校验存在");
        if !matches!(
            module.status,
            ModuleStatus::Translating
                | ModuleStatus::CompileFixing
                | ModuleStatus::Testing
                | ModuleStatus::Reviewing
        ) {
            return Err(MigrateError::Config(format!(
                "模块 {} 当前状态 {} 不允许写质量度量（仅运行态可写）",
                canonical, module.status
            )));
        }

        if let Some(rate) = test_pass_rate {
            module.test_pass_rate = Some(rate.to_owned());
        }
        if let Some(count) = known_differences {
            module.known_differences = count;
        }
        module.attempts.push(AttemptRecord {
            timestamp: Timestamp::new(chrono::Utc::now().to_rfc3339()),
            result: format!(
                "metrics:test_pass_rate={} known_differences={}",
                module.test_pass_rate.as_deref().unwrap_or("null"),
                module.known_differences
            ),
            retry_count: 0,
            checkpoint: None,
        });
        Ok(RecordMetricsOutcome {
            module: canonical,
            test_pass_rate: module.test_pass_rate.clone(),
            known_differences: module.known_differences,
        })
    }

    /// 归一 module key：调用方通常传组代表 key，但 run/reset 阶段也可能对折叠组的非代表成员
    /// （如 `file:types.ts`）发起操作——反查其所属组代表后按组处理，避免硬失败破坏 composite
    /// 组状态推进（与 `cmd_state_deps` 的归一逻辑对称）。命中直接返回；查无则 `模块不存在`。
    ///
    /// 依赖不变量：`member_files` 是文件节点的**划分**（跨组互斥，每个文件至多属一个组，见
    /// populate-modules 落盘）。该不变量成立时 `find` 命中唯一、归一确定；若被破坏（同一文件
    /// 出现在多组），`find` 取 HashMap 迭代序首个为非确定——debug 下断言钉住。
    fn canonical_module_key(&self, name: &str) -> Result<String> {
        if self.state_file.modules.contains_key(name) {
            return Ok(name.to_string());
        }
        // 反查非代表成员所属组。不变量破坏时（同一文件属多组）**release 也硬错**——
        // 原仅 debug_assert 钉住、release 静默取 HashMap 迭代序首个（非确定），复用到破坏性的
        // reset 会清空**错误模块**的进度字段（数据破坏，非仅状态偏差），故升级为运行时错误。
        let matches: Vec<&String> = self
            .state_file
            .modules
            .iter()
            .filter(|(_, m)| {
                m.member_files
                    .as_ref()
                    .is_some_and(|mf| mf.iter().any(|f| f == name))
            })
            .map(|(k, _)| k)
            .collect();
        match matches.as_slice() {
            [] => Err(MigrateError::Config(format!("模块不存在: {name}"))),
            [one] => Ok((*one).clone()),
            _ => Err(MigrateError::Config(format!(
                "member_files 跨组互斥不变量被破坏：{name} 同属多个组"
            ))),
        }
    }

    /// 幂等回退失败/中途模块到干净的重译入口（M4-ROB-01a：checkpoint 硬化 + 幂等重试）。
    ///
    /// 把模块状态回退到 `translating` 并清除全部「尝试进度」字段（`substatus` /
    /// `phase_a_version` / `phase_a_audit_passed` / `test_pass_rate` / `coverage` /
    /// `known_differences` / blocked 锚点），使断点续传路由（run.md § 断点续传路由）从
    /// Phase A 起点干净重跑。**保留** `attempts`（审计历史，追加一条 `reset` 记录）、`tier`、
    /// `member_files` / `composite_kind` / `decomposition_*` / `danger`（结构性冻结字段）——
    /// 回退是「重试」而非「重新拆解」。
    ///
    /// **幂等**：模块已处于干净重译入口（`translating`/`pending` 且全部进度字段为空）时为
    /// 空操作（`was_noop=true`），**不追加审计记录**——保证 `reset;reset` 与 `reset` 状态一致
    /// （调用方可据 `was_noop` 省略 save）。
    ///
    /// **终态 / 锚点 / 决策点守护**：`done`（唯一真终态）、`blocked`（依赖锚点）、`paused`
    /// （自动重试耗尽待人类抉择）、`degrade_*`（人类降级决策）须 `force=true` 才可回退，否则报错
    /// ——防止误清断点续传锚点 / 静默重迁已完成模块 / 绕过降级抉择。**项目级**：`graduate`
    /// （毕业终态）下一律拒绝（含 `--force`），避免制造「项目终态 + 非终态模块」矛盾。
    ///
    /// **产物清理不在此**：CLI 不写 `rust_root` 下的 `.rs`、也不猜路径删（见 MDR-015）；本方法只做
    /// 确定性的状态回退，`rust_root` 部分产物的清理由 CLI 层据 member 作用域输出指令、编排器执行。
    /// module key 归一同 [`transition_module`](Self::transition_module)。
    pub fn reset_module(&mut self, name: &str, force: bool) -> Result<ResetOutcome> {
        // 项目级守护（先于 --force，force 不可绕过）：`graduate`（毕业终态）下把 done 模块回退成
        // 非终态会制造「项目终态 + 非终态模块」矛盾，且状态机无 `graduate → sprint_loop` 回退路径
        // （难以恢复）。拒绝——如需重迁已毕业项目的模块，须先重开迁移。
        if self.state_file.state == ProjectState::Graduate {
            return Err(MigrateError::Config(
                "项目已毕业（graduate），reset 会制造「项目终态 + 非终态模块」矛盾且无合法回退路径；\
                 如需重迁请先重开迁移"
                    .to_string(),
            ));
        }
        let canonical = self.canonical_module_key(name)?;
        let module = self
            .state_file
            .modules
            .get_mut(&canonical)
            .expect("canonical 已校验存在");
        let from = module.status;
        // 源作用域（清理指令用）：composite → member_files；单文件 → [canonical]。
        let member_files = module
            .member_files
            .clone()
            .unwrap_or_else(|| vec![canonical.clone()]);

        // 终态 / 锚点 / 决策点守护：done / blocked / paused / degrade_* 须 --force。
        if !force {
            let guard = match from {
                ModuleStatus::Done => Some("done 是终态，重迁已完成模块须 --force（人类确认）"),
                ModuleStatus::Blocked => {
                    Some("模块阻塞中（等依赖），reset 会清除阻塞锚点，须 --force")
                }
                // paused = 自动重试耗尽、待人类在「重试 vs 降级」间抉择的决策点（决策地位同
                // degrade_*）。裸 reset 会静默塞回重试循环、绕过该抉择（ROB-01b watchdog 程序化
                // 调 reset 时尤甚），故须 --force 显式确认「就是要重试」。
                ModuleStatus::Paused => {
                    Some("模块暂停中（自动重试耗尽待人类抉择），reset 会绕过降级决策点，须 --force")
                }
                s if s.is_degraded() => Some("降级恢复是人类决策（见设计 § Step 0.3），须 --force"),
                _ => None,
            };
            if let Some(msg) = guard {
                return Err(MigrateError::Config(format!(
                    "{from} 模块 reset 需 --force：{msg}"
                )));
            }
        }

        // 幂等判定：已处于干净重译入口 → 空操作，不改任何字段、不追加审计。
        let already_clean = matches!(from, ModuleStatus::Translating | ModuleStatus::Pending)
            && module.substatus.is_none()
            && module.phase_a_version.is_none()
            && module.phase_a_audit_passed.is_none()
            && module.test_pass_rate.is_none()
            && module.coverage.is_none()
            && module.known_differences == 0
            && module.blocked_by.is_none()
            && module.pre_blocked_status.is_none();
        if already_clean {
            return Ok(ResetOutcome {
                module: canonical,
                reset_from: from,
                reset_to: from,
                was_noop: true,
                member_files,
            });
        }

        // 回退：清全部进度字段，status → translating。
        // `pending` 保持 `pending`（尚未起步、无产物，无需前移到 translating）。
        let to = if from == ModuleStatus::Pending {
            ModuleStatus::Pending
        } else {
            ModuleStatus::Translating
        };
        module.status = to;
        module.substatus = None;
        module.phase_a_version = None;
        module.phase_a_audit_passed = None;
        module.test_pass_rate = None;
        module.coverage = None;
        module.known_differences = 0;
        module.blocked_by = None;
        module.pre_blocked_status = None;

        // 审计：append reset 记录（保留既有 attempts 历史）。
        module.attempts.push(AttemptRecord {
            timestamp: Timestamp::new(chrono::Utc::now().to_rfc3339()),
            result: format!("reset:{from}→{to}"),
            retry_count: 0,
            checkpoint: None,
        });

        Ok(ResetOutcome {
            module: canonical,
            reset_from: from,
            reset_to: to,
            was_noop: false,
            member_files,
        })
    }

    /// 从 watchdog stall（agent 静默超时）确定性、幂等地恢复单个模块（M4-ROB-01b）。
    ///
    /// 编排器（run.md）负责**检测** stall（后台命令 stdout 静默超 `stall_timeout_secs`——CLI
    /// 无法观测子进程 stdout）并据 `stall_recovery_policy` 解析出 `policy`；本方法据 `policy`
    /// 执行确定性回退，`reason` 记入模块 `attempts` 审计（append-only）。见 MDR-016。
    ///
    /// - [`RecoverPolicy::Retry`]：委派 [`reset_module`](Self::reset_module)`(force=true)`——stall
    ///   时模块常在 `paused`/进行态，force 跨守护回退到干净重译入口，复用其**幂等**
    ///   （`was_noop`）、进度清理、`member_files` 作用域。非 noop 时额外追加一条 `stall-recover:retry`
    ///   审计；noop 则整体 noop（保 `recover;recover == recover`）。
    /// - [`RecoverPolicy::Skip`]：**直接**置 `status → paused`（决策点，headless 由既有编排 prose
    ///   自动 `degrade_skip`）并清 `substatus`（活跃态瞬态标记，挂 paused 上语义不符；进度字段保留
    ///   供降级分析）。**绕过 `can_transition_to` 矩阵**——stall 可发生在 `translating`（Phase A），
    ///   而 `translating → paused` 不在矩阵（见 `ModuleStatus::can_transition_to`），故仿 `reset_module`
    ///   破坏性直设。已是 `paused` → 幂等 noop。
    ///
    /// **守护**（先于策略，无 `--force` 逃生口）：仅放行**可能 stall 的运行态**
    /// （`translating`/`compile_fixing`/`testing`/`reviewing`）**+ stall 落点 `paused`**；`pending`
    /// （未起步、无运行 agent）/ `done`（终态）/ `blocked`（等依赖、无运行 agent）/ `degrade_*`
    /// （**人类降级决策，recover 不得撤销**——否则 retry 绕 `--force` 撤销降级）一律拒绝；项目
    /// `graduate` 态拒绝（同 reset）。**不做**下游 `blocked_by` 传播（沿用 workflow.md 既有机制，
    /// 见 MDR-016）。module key 归一同 [`transition_module`](Self::transition_module)。
    pub fn recover_module(
        &mut self,
        name: &str,
        policy: RecoverPolicy,
        reason: Option<&str>,
    ) -> Result<RecoverOutcome> {
        // 项目级守护（先于策略）：graduate 下把模块回退成非终态制造「项目终态 + 非终态模块」矛盾。
        if self.state_file.state == ProjectState::Graduate {
            return Err(MigrateError::Config(
                "项目已毕业（graduate），stall 恢复会制造「项目终态 + 非终态模块」矛盾且无合法回退路径；\
                 如需重迁请先重开迁移"
                    .to_string(),
            ));
        }
        let canonical = self.canonical_module_key(name)?;
        let module = self
            .state_file
            .modules
            .get_mut(&canonical)
            .expect("canonical 已校验存在");
        let from = module.status;
        let member_files = module
            .member_files
            .clone()
            .unwrap_or_else(|| vec![canonical.clone()]);

        // 守护：recover 仅适用于「可能 stall 的运行态 + stall 落点 paused」。其余态无运行中
        // agent（pending/blocked）或是人类决策终态语义（done/degrade_*）→ 拒绝。全枚举显式 match
        // （未来新增状态编译器强制处理），无 `--force` 逃生口：recover 是程序化 stall 入口，误用暴露为错。
        match from {
            // 放行：有运行中 agent 可能 stall 的活跃态 + stall 后重试耗尽的常见落点 paused。
            ModuleStatus::Translating
            | ModuleStatus::CompileFixing
            | ModuleStatus::Testing
            | ModuleStatus::Reviewing
            | ModuleStatus::Paused => {}
            ModuleStatus::Pending => {
                return Err(MigrateError::Config(
                    "pending 尚未起步、无运行中 agent，非 stall 态；stall 恢复不适用".to_string(),
                ));
            }
            ModuleStatus::Done => {
                return Err(MigrateError::Config(
                    "done 是终态、不会 stall；如需重迁已完成模块用 state reset --force".to_string(),
                ));
            }
            ModuleStatus::Blocked => {
                return Err(MigrateError::Config(
                    "模块阻塞中（等依赖、无运行中 agent），非 stall 态；stall 恢复不适用"
                        .to_string(),
                ));
            }
            // degrade_* 是人类降级决策终态语义——watchdog 程序化 recover **不得撤销**（否则
            // `retry` 会 `degrade_* → translating` 绕过 `--force` 人类确认；`retry;skip` 更会把依赖侧
            // 已视终态的模块变回非终态）。如确需恢复已降级模块，走 `state reset --force`（人类显式）。
            ModuleStatus::DegradeFfi | ModuleStatus::DegradeManual | ModuleStatus::DegradeSkip => {
                return Err(MigrateError::Config(
                    "模块已降级（degrade_*，人类降级决策），recover 不撤销；如需恢复重译用 state reset --force"
                        .to_string(),
                ));
            }
        }

        match policy {
            RecoverPolicy::Retry => {
                // 复用 reset_module（force=true）：stall 时模块常在 paused/进行态，须跨守护回退。
                let reset = self.reset_module(&canonical, true)?;
                // 非 noop 追加 stall 审计（noop 整体 noop，双调用安全）。
                if !reset.was_noop {
                    self.push_recover_audit(&reset.module, RecoverPolicy::Retry, reason);
                }
                Ok(RecoverOutcome {
                    module: reset.module,
                    policy,
                    from: reset.reset_from,
                    to: reset.reset_to,
                    was_noop: reset.was_noop,
                    member_files: reset.member_files,
                })
            }
            RecoverPolicy::Skip => {
                // 幂等：已 paused（唯一放行的非活跃态，degrade_* 已被守护拒绝）→ 已在决策点，noop。
                if from == ModuleStatus::Paused {
                    return Ok(RecoverOutcome {
                        module: canonical,
                        policy,
                        from,
                        to: from,
                        was_noop: true,
                        member_files,
                    });
                }
                // 直接置 paused（绕过转换矩阵，理由见方法级 doc）。清 `substatus`——它是活跃态的
                // 瞬态阶段标记（如 `phase_a_complete_awaiting_review`），挂到 paused 上语义不符；
                // stall 原因由 attempts 的 `stall-recover:skip` 承载。**保留**其他进度字段
                // （phase_a_version/test_pass_rate/coverage/known_differences）供后续降级分析读取
                // ——skip≠reset（reset 是完全回退重译才清进度）。
                module.status = ModuleStatus::Paused;
                module.substatus = None;
                self.push_recover_audit(&canonical, RecoverPolicy::Skip, reason);
                Ok(RecoverOutcome {
                    module: canonical,
                    policy,
                    from,
                    to: ModuleStatus::Paused,
                    was_noop: false,
                    member_files,
                })
            }
        }
    }

    /// 生成额度耗尽/中断后续跑的**断点计划**（M4-ROB-01c）——纯查询、无 mutation、不加载 graph。
    ///
    /// 遍历全部模块按状态归 5 桶（见 [`ResumePlan`]）：运行态→`interrupted`（需 recover retry
    /// 幂等重入）、`paused`→`awaiting_decision`（待决策，续跑不复活）、`pending`→`next`、
    /// `blocked`→`blocked`、终态（done/degrade_*）仅计入 progress（**不重跑**）。各列表按 module
    /// key 字典序排序（`modules` 是 HashMap，排序保输出确定性）。
    ///
    /// **关键**：运行态与 `paused` 分列——运行态是被额度打断的进行中工作、应 retry；`paused` 是
    /// 前次 skip 留下的人类决策点，续跑重入不得把它复活重译（否则绕过降级抉择，见 MDR-017）。
    pub fn resume_plan(&self) -> ResumePlan {
        let mut interrupted: Vec<InterruptedModule> = Vec::new();
        let mut awaiting_decision: Vec<String> = Vec::new();
        let mut next: Vec<String> = Vec::new();
        let mut blocked: Vec<String> = Vec::new();
        let mut done = 0usize;
        let mut degraded = 0usize;

        for (key, m) in &self.state_file.modules {
            match m.status {
                ModuleStatus::Translating
                | ModuleStatus::CompileFixing
                | ModuleStatus::Testing
                | ModuleStatus::Reviewing => {
                    let member_files = m.member_files.clone().unwrap_or_else(|| vec![key.clone()]);
                    interrupted.push(InterruptedModule {
                        module: key.clone(),
                        status: m.status,
                        member_files,
                    });
                }
                ModuleStatus::Paused => awaiting_decision.push(key.clone()),
                ModuleStatus::Pending => next.push(key.clone()),
                ModuleStatus::Blocked => blocked.push(key.clone()),
                ModuleStatus::Done => done += 1,
                ModuleStatus::DegradeFfi
                | ModuleStatus::DegradeManual
                | ModuleStatus::DegradeSkip => degraded += 1,
            }
        }

        interrupted.sort_by(|a, b| a.module.cmp(&b.module));
        awaiting_decision.sort();
        next.sort();
        blocked.sort();

        ResumePlan {
            sprint: self.state_file.sprint.as_ref().map(|s| s.current),
            interrupted,
            awaiting_decision,
            next,
            blocked,
            done,
            degraded,
        }
    }

    /// 向指定模块 `attempts` 追加一条 `stall-recover:<policy>[ reason=<r>]` 审计记录。
    fn push_recover_audit(&mut self, canonical: &str, policy: RecoverPolicy, reason: Option<&str>) {
        let module = self
            .state_file
            .modules
            .get_mut(canonical)
            .expect("canonical 已校验存在");
        let suffix = reason.map(|r| format!(" reason={r}")).unwrap_or_default();
        module.attempts.push(AttemptRecord {
            timestamp: Timestamp::new(chrono::Utc::now().to_rfc3339()),
            result: format!("stall-recover:{}{suffix}", policy.as_str()),
            retry_count: 0,
            checkpoint: None,
        });
    }

    /// 追加一条 SubAgent 调用记录到顶层 `subagent_calls` 数组（append-only，不去重）。
    ///
    /// 对齐 `docs/design/09-appendix-schemas.md § subagent_calls 字段说明`：每次 SubAgent
    /// 调用（含重试）追加一条 `{step_index, subagent_name, started_at, ended_at, status,
    /// error_message}`，用于诊断卡死与统计重试次数。本方法只负责入库，时间戳/状态由调用方
    /// 构造好的 [`SubAgentCall`] 决定（不做任何校验或合并）。
    ///
    /// `started_at` 为 `None` 时取当前 UTC 时间（schema 中该字段必填，给出合理缺省以便
    /// 编排器在调用开始时即可记录），返回追加后数组的长度。
    pub fn push_subagent_call(
        &mut self,
        step_index: u32,
        subagent_name: String,
        status: String,
        started_at: Option<Timestamp>,
        ended_at: Option<Timestamp>,
        error_message: Option<String>,
    ) -> usize {
        let started_at =
            started_at.unwrap_or_else(|| Timestamp::new(chrono::Utc::now().to_rfc3339()));
        self.state_file
            .subagent_calls
            .push(crate::types::state::SubAgentCall {
                step_index,
                subagent_name,
                started_at,
                ended_at,
                status,
                error_message,
            });
        self.state_file.subagent_calls.len()
    }

    /// 设置 sprint 信息。
    pub fn set_sprint(&mut self, sprint: crate::types::state::SprintState) {
        self.state_file.sprint = Some(sprint);
    }

    /// 尝试推进 sprint：如果当前 sprint 所有模块均已终态，推进到下一 sprint。
    ///
    /// 返回值：
    /// - `Advanced(new_sprint)` — 推进成功，已切到新 sprint
    /// - `AllCompleted` — 最后一个 sprint 已完成（history 已关闭，需调用方 save）
    /// - `NotReady` — 当前 sprint 尚有非终态模块，或无 sprint 信息
    pub fn try_advance_sprint(&mut self) -> SprintAdvanceResult {
        let sprint_state = match self.state_file.sprint.as_mut() {
            Some(s) => s,
            None => return SprintAdvanceResult::NotReady,
        };
        let current = sprint_state.current;

        let current_modules: Vec<(String, &ModuleState)> = self
            .state_file
            .modules
            .iter()
            .filter(|(_, m)| m.sprint == Some(current))
            .map(|(k, v)| (k.clone(), v))
            .collect();

        if !current_modules.is_empty()
            && !current_modules.iter().all(|(_, m)| m.status.is_terminal())
        {
            return SprintAdvanceResult::NotReady;
        }

        let now = Timestamp::now();

        // 关闭当前 sprint history 条目。
        if let Some(entry) = sprint_state
            .history
            .iter_mut()
            .find(|e| e.id == current && e.completed_at.is_none())
        {
            entry.completed_at = Some(now.clone());
            entry.completed_modules = current_modules
                .iter()
                .filter(|(_, m)| m.status.is_terminal())
                .map(|(k, _)| k.clone())
                .collect();
        }

        let new_sprint = current + 1;

        let next_targets: Vec<String> = self
            .state_file
            .modules
            .iter()
            .filter(|(_, m)| m.sprint == Some(new_sprint))
            .map(|(k, _)| k.clone())
            .collect();

        if next_targets.is_empty() {
            return SprintAdvanceResult::AllCompleted;
        }

        sprint_state.current = new_sprint;
        sprint_state.history.push(crate::types::state::SprintEntry {
            id: new_sprint,
            started_at: now,
            completed_at: None,
            target_modules: next_targets,
            completed_modules: Vec::new(),
            notes: None,
            porting_md_version: None,
        });

        SprintAdvanceResult::Advanced(new_sprint)
    }

    /// 检查模块 substatus 是否为 `agent_done`（并行翻译两层 done 协议）。
    ///
    /// 并行翻译中，agent 在 worktree 内自检通过后标 `agent_done`（substatus，非终态）；
    /// 只有编排器整组 `cargo check`/`cargo test` 通过后才升最终 `done`。
    /// 本方法供编排器查询哪些模块已完成 agent 级自检、等待整组验证。
    ///
    /// 模块不存在返回 `false`。
    pub fn is_agent_done(&self, name: &str) -> bool {
        self.state_file
            .modules
            .get(name)
            .is_some_and(|m| m.substatus.as_deref() == Some(SUBSTATUS_AGENT_DONE))
    }

    /// 批量将 `agent_done` 模块转为 `done`（整组 check 通过后调用）。
    ///
    /// 对每个模块独立调用 `transition_module`（`reviewing → done`），一个失败不影响其他。
    /// 返回实际成功转换的模块名列表；失败的模块保持原状态，错误记入 `attempts`。
    ///
    /// 前置约束：调用方应确保传入模块当前 status 为 `reviewing`、substatus 为 `agent_done`。
    /// 不满足前置的模块会在 `transition_module` 中被矩阵拒绝，计入失败而非 panic。
    pub fn batch_transition_done(&mut self, modules: &[String]) -> Result<Vec<String>> {
        let mut succeeded = Vec::new();
        for name in modules {
            // 先检查 substatus 是否为 agent_done（防止误操作非 agent_done 模块）。
            let is_agent = self.is_agent_done(name);
            if !is_agent {
                // 非 agent_done 模块：记录失败原因到 attempts，跳过。
                let _ = self.transition_module(
                    name,
                    None,
                    None,
                    Some("batch_transition_done: substatus 非 agent_done，跳过"),
                    false,
                );
                continue;
            }
            // 尝试 reviewing → done 转换。
            match self.transition_module(name, Some(ModuleStatus::Done), None, None, false) {
                Ok(()) => succeeded.push(name.clone()),
                Err(_) => {
                    // 转换失败（如 status 不是 reviewing）：记录失败原因，继续其他模块。
                    let _ = self.transition_module(
                        name,
                        None,
                        None,
                        Some("batch_transition_done: reviewing→done 转换失败"),
                        false,
                    );
                }
            }
        }
        Ok(succeeded)
    }

    /// 设置最后错误信息。
    pub fn set_last_error(&mut self, error: Option<String>) {
        let metadata = self.state_file.metadata.get_or_insert(MigrationMetadata {
            graph_build_completed: false,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
            version: 0,
            last_modified_by: None,
        });
        metadata.last_error = error;
    }

    /// 读取当前 `metadata.version`（乐观锁版本号）。
    ///
    /// 无 metadata 时返回 0（向后兼容旧状态文件）。
    pub fn metadata_version(&self) -> u64 {
        self.state_file
            .metadata
            .as_ref()
            .map(|m| m.version)
            .unwrap_or(0)
    }

    /// 乐观锁状态更新（CAS：Compare-And-Swap）。
    ///
    /// 读取当前 `metadata.version`，与 `cas_version` 比较：
    /// - **不匹配**：返回 `MigrateError::LockConflict`（带当前版本号信息），不修改任何状态。
    /// - **匹配**：执行模块状态转换（同 [`transition_module`]）并递增 `metadata.version`。
    ///
    /// 返回 `(previous_status, new_version)` 供调用方构造输出。
    pub fn update_with_cas(
        &mut self,
        module: &str,
        status: ModuleStatus,
        cas_version: u64,
        substatus: Option<&str>,
        reason: Option<&str>,
    ) -> Result<(ModuleStatus, u64)> {
        // CAS 检查：读取当前版本，不匹配则拒绝。
        let current_version = self.metadata_version();
        if cas_version != current_version {
            return Err(MigrateError::LockConflict(format!(
                "版本冲突: 期望 {cas_version}, 当前 {current_version}"
            )));
        }

        // 模块存在性检查 + 获取旧状态。
        let previous_status = self
            .state_file
            .modules
            .get(module)
            .ok_or_else(|| MigrateError::Config(format!("模块不存在: {module}")))?
            .status;

        // 执行状态转换（复用 transition_module 的合法性校验）。
        // CAS 更新不支持 --force（降级恢复由 transition 命令走）。
        self.transition_module(module, Some(status), substatus, reason, false)?;

        // 递增版本号。
        let metadata = self.state_file.metadata.get_or_insert(MigrationMetadata {
            graph_build_completed: false,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
            version: 0,
            last_modified_by: None,
        });
        metadata.version += 1;
        let new_version = metadata.version;

        Ok((previous_status, new_version))
    }
}

/// sprint 推进结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprintAdvanceResult {
    /// 推进到新 sprint。
    Advanced(u32),
    /// 最后一个 sprint 已全部完成（history 已关闭，需调用方 save）。
    AllCompleted,
    /// 当前 sprint 尚有非终态模块，无法推进。
    NotReady,
}

/// 原子写入：（按 `backup_existing`）覆盖前备份 `.backup`，写入 `.tmp` 并 fsync，再 rename 到目标，最后同步父目录。
///
/// 保证崩溃/并发中断时目标文件要么是旧内容要么是完整新内容，绝不出现半截 JSON。
/// `backup_existing=false` 时跳过备份——仅用于"已知现有主文件损坏"的恢复保存场景（见 [`save`]），
/// 防止用损坏内容覆盖最后的有效 `.backup`。
fn atomic_write(path: &Path, bytes: &[u8], backup_existing: bool) -> Result<()> {
    if backup_existing && path.exists() {
        let backup = sibling_with_suffix(path, ".backup");
        std::fs::copy(path, &backup)?;
    }
    let tmp = sibling_with_suffix(path, ".tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    // 同步父目录，确保 rename 元数据落盘（best-effort，失败不影响数据完整性）。
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
    Ok(())
}

/// 在同目录下生成**隐藏**兄弟路径 `.<原文件名><后缀>`，与设计
/// `docs/design/06-plugin-structure.md` crash-safe 约定的隐藏 tmp/backup 命名一致
/// （如 `migration-state.json` → `.migration-state.json.backup`）。
/// 隐藏文件避免污染目录列表/被工具误扫；已带前导点的输入不再叠加。
fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let original = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    let mut name = std::ffi::OsString::new();
    if !original.to_string_lossy().starts_with('.') {
        name.push(".");
    }
    name.push(&original);
    name.push(suffix);
    path.with_file_name(name)
}

/// 清理超过 `retention_days` 天的 `.backup` 文件（best-effort，失败静默忽略）。
///
/// 当前每个状态文件只有一个 `.backup`，检查其修改时间是否超过保留期，超期则删除。
/// save 时顺带执行，不独立定时。
fn cleanup_old_backups(path: &Path, retention_days: u32) {
    let backup = sibling_with_suffix(path, ".backup");
    if !backup.exists() {
        return;
    }
    let Ok(metadata) = std::fs::metadata(&backup) else {
        return;
    };
    let Ok(modified) = metadata.modified() else {
        return;
    };
    let retention = std::time::Duration::from_secs(u64::from(retention_days) * 86400);
    if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
        if age > retention {
            let _ = std::fs::remove_file(&backup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::state::{SprintEntry, SprintState};
    use tempfile::NamedTempFile;

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
        }
    }

    /// 辅助：创建一个初始状态机。
    fn new_machine() -> MigrationStateMachine {
        MigrationStateMachine::init_new("test-project", SourceLang::TypeScript)
    }

    #[test]
    fn test_init_new_correct_initial_state() {
        let m = new_machine();
        assert_eq!(m.current_state(), ProjectState::Init);
        assert_eq!(m.state_file().version, "1.0.0");
        assert_eq!(m.state_file().state_history.len(), 1);
        let project = m.state_file().project.as_ref().expect("应有 project");
        assert_eq!(project.name, "test-project");
        assert_eq!(project.source_language, SourceLang::TypeScript);
    }

    #[test]
    fn test_retain_modules_removes_orphans() {
        let mut m = new_machine();
        m.update_module("file:a", module_with_status(ModuleStatus::Pending));
        m.update_module("file:b", module_with_status(ModuleStatus::Pending));
        m.update_module("file:c", module_with_status(ModuleStatus::Pending));

        // 仅保留 a、b；c 应作为孤儿被删除并返回。
        let live: std::collections::HashSet<String> = ["file:a".to_owned(), "file:b".to_owned()]
            .into_iter()
            .collect();
        let orphans = m.retain_modules(&live);

        assert_eq!(orphans, vec!["file:c".to_owned()]);
        assert_eq!(m.state_file().modules.len(), 2);
        assert!(m.state_file().modules.contains_key("file:a"));
        assert!(m.state_file().modules.contains_key("file:b"));
        assert!(!m.state_file().modules.contains_key("file:c"));
    }

    #[test]
    fn test_retain_modules_no_orphans() {
        let mut m = new_machine();
        m.update_module("file:a", module_with_status(ModuleStatus::Pending));
        let live: std::collections::HashSet<String> = ["file:a".to_owned()].into_iter().collect();
        // 全部存活：无孤儿、不删除。
        assert!(m.retain_modules(&live).is_empty());
        assert_eq!(m.state_file().modules.len(), 1);
    }

    #[test]
    fn test_valid_transition_init_to_profile() {
        let mut m = new_machine();
        assert!(m.transition(ProjectState::Profile).is_ok());
        assert_eq!(m.current_state(), ProjectState::Profile);
        // 历史应有 2 条记录
        assert_eq!(m.state_file().state_history.len(), 2);
        // 第一条应有 exited_at
        assert!(m.state_file().state_history[0].exited_at.is_some());
        // 第二条 exited_at 应为 None
        assert!(m.state_file().state_history[1].exited_at.is_none());
    }

    #[test]
    fn test_full_chain_transition() {
        let mut m = new_machine();
        let steps = [
            ProjectState::Profile,
            ProjectState::Plan,
            ProjectState::Scaffold,
            ProjectState::SprintLoop,
            ProjectState::Graduate,
        ];
        for step in &steps {
            assert!(m.transition(*step).is_ok(), "转换到 {} 应成功", step);
        }
        assert_eq!(m.current_state(), ProjectState::Graduate);
        assert_eq!(m.state_file().state_history.len(), 6);
    }

    #[test]
    fn test_invalid_transition_init_to_plan() {
        let mut m = new_machine();
        let result = m.transition(ProjectState::Plan);
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::InvalidTransition { from, to } => {
                assert_eq!(from, "init");
                assert_eq!(to, "plan");
            }
            other => panic!("期望 InvalidTransition，实际: {:?}", other),
        }
        // 状态不应改变
        assert_eq!(m.current_state(), ProjectState::Init);
    }

    #[test]
    fn test_invalid_transition_skip_phase() {
        let mut m = new_machine();
        assert!(m.transition(ProjectState::Scaffold).is_err());
        assert!(m.transition(ProjectState::SprintLoop).is_err());
        assert!(m.transition(ProjectState::Graduate).is_err());
    }

    #[test]
    fn test_invalid_transition_backward() {
        let mut m = new_machine();
        m.transition(ProjectState::Profile).unwrap();
        assert!(m.transition(ProjectState::Init).is_err());
    }

    #[test]
    fn test_save_and_load() {
        let m = new_machine();
        let tmp = NamedTempFile::new().expect("创建临时文件失败");
        let path = tmp.path().to_owned();
        m.save(&path).expect("保存失败");

        // 验证文件内容可解析
        let loaded = MigrationStateMachine::load(&path).expect("加载失败");
        assert_eq!(loaded.current_state(), ProjectState::Init);
        assert_eq!(
            loaded.state_file().project.as_ref().unwrap().name,
            "test-project"
        );
        // 清理
        drop(tmp);
    }

    #[test]
    fn test_save_transition_reload() {
        let mut m = new_machine();
        m.transition(ProjectState::Profile).unwrap();
        let tmp = NamedTempFile::new().expect("创建临时文件失败");
        let path = tmp.path().to_owned();
        m.save(&path).unwrap();

        let loaded = MigrationStateMachine::load(&path).unwrap();
        assert_eq!(loaded.current_state(), ProjectState::Profile);
        assert_eq!(loaded.state_file().state_history.len(), 2);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = MigrationStateMachine::load(Path::new("/tmp/不存在的文件.json"));
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::FileNotFound(p) => {
                assert!(p.to_string_lossy().contains("不存在的文件"));
            }
            other => panic!("期望 FileNotFound，实际: {:?}", other),
        }
    }

    #[test]
    fn test_load_invalid_json() {
        let mut tmp = NamedTempFile::new().expect("创建临时文件失败");
        tmp.write_all(b"not json").unwrap();
        tmp.flush().unwrap();
        let result = MigrationStateMachine::load(tmp.path());
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrateError::Json(_) => {}
            other => panic!("期望 Json 错误，实际: {:?}", other),
        }
    }

    #[test]
    fn test_save_creates_parent_dir() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let nested = dir.path().join("sub").join("dir").join("state.json");
        let m = new_machine();
        assert!(m.save(&nested).is_ok());
        assert!(nested.exists());
    }

    #[test]
    fn test_transition_module_valid() {
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        assert!(m
            .transition_module("a", Some(ModuleStatus::Translating), None, None, false)
            .is_ok());
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::Translating
        );
    }

    #[test]
    fn test_transition_module_rejects_terminal_regression() {
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Done));
        // done 是终态，不可改回 pending（断点续传保护）。
        let err = m
            .transition_module("a", Some(ModuleStatus::Pending), None, None, false)
            .unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Done);
    }

    #[test]
    fn test_transition_module_missing() {
        let mut m = new_machine();
        let err = m
            .transition_module("ghost", Some(ModuleStatus::Translating), None, None, false)
            .unwrap_err();
        assert!(matches!(err, MigrateError::Config(_)));
    }

    #[test]
    fn test_transition_module_normalizes_non_representative_member() {
        // composite 组以代表 key 持状态；对折叠组的非代表成员发 transition 应归一到组代表
        // （与 cmd_state_deps 的组感知对称），而非报「模块不存在」破坏组状态推进。
        let mut m = new_machine();
        let mut grp = module_with_status(ModuleStatus::Pending);
        grp.member_files = Some(vec!["grp".to_string(), "file:helper.ts".to_string()]);
        m.update_module("grp", grp);

        // 传非代表成员 key：应归一到组代表 "grp"。
        assert!(m
            .transition_module(
                "file:helper.ts",
                Some(ModuleStatus::Translating),
                None,
                None,
                false
            )
            .is_ok());
        // 组代表状态被推进，且未意外创建独立的成员模块。
        assert_eq!(
            m.state_file().modules["grp"].status,
            ModuleStatus::Translating
        );
        assert!(!m.state_file().modules.contains_key("file:helper.ts"));
    }

    #[test]
    fn test_transition_module_substatus_only_keeps_status() {
        // to == None：仅更新 substatus，status 不变（设计行 461/485 Phase 进度记录）。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Translating));
        assert!(m
            .transition_module(
                "a",
                None,
                Some("phase_a_complete_awaiting_review"),
                None,
                false
            )
            .is_ok());
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::Translating);
        assert_eq!(
            module.substatus.as_deref(),
            Some("phase_a_complete_awaiting_review")
        );
    }

    #[test]
    fn test_transition_module_reason_appended_to_attempts() {
        // reason 落盘到 attempts 作为审计记录。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        m.transition_module(
            "a",
            Some(ModuleStatus::Translating),
            None,
            Some("kick off"),
            false,
        )
        .unwrap();
        let attempts = &m.state_file().modules["a"].attempts;
        assert_eq!(attempts.len(), 1);
        assert!(attempts[0].result.contains("pending"));
        assert!(attempts[0].result.contains("translating"));
        assert!(attempts[0].result.contains("kick off"));
    }

    #[test]
    fn test_transition_module_enter_blocked_records_pre_status() {
        // 进入 blocked 记录 pre_blocked_status。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Translating));
        m.transition_module("a", Some(ModuleStatus::Blocked), None, None, false)
            .unwrap();
        assert_eq!(
            m.state_file().modules["a"].pre_blocked_status,
            Some(ModuleStatus::Translating)
        );
    }

    #[test]
    fn test_transition_module_leave_blocked_clears_metadata() {
        // 离开 blocked 清除 blocked_by 与 pre_blocked_status（恢复到 pre_blocked_status）。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Blocked);
        module.blocked_by = Some(vec!["core/parser".to_owned()]);
        module.pre_blocked_status = Some(ModuleStatus::Translating);
        m.update_module("a", module);
        m.transition_module("a", Some(ModuleStatus::Translating), None, None, false)
            .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::Translating);
        assert!(module.blocked_by.is_none());
        assert!(module.pre_blocked_status.is_none());
    }

    #[test]
    fn test_transition_module_degrade_force_resets_attempts() {
        // degrade_* → translating 清除 substatus + 清空 attempts。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::DegradeManual);
        module.substatus = Some("async_too_complex".to_owned());
        module.attempts.push(AttemptRecord {
            timestamp: Timestamp::new("2026-06-14T00:00:00Z"),
            result: "fail".to_owned(),
            retry_count: 3,
            checkpoint: None,
        });
        m.update_module("a", module);
        m.transition_module("a", Some(ModuleStatus::Translating), None, None, true)
            .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::Translating);
        assert!(module.substatus.is_none());
        assert!(module.attempts.is_empty());
    }

    #[test]
    fn test_transition_module_to_done_clears_substatus() {
        // graduate 到 Done 时清空 testing/review 阶段残留的 substatus。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Reviewing);
        module.substatus = Some("phase_b_optimization_in_progress".to_owned());
        m.update_module("a", module);
        m.transition_module("a", Some(ModuleStatus::Done), None, None, false)
            .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::Done);
        assert!(module.substatus.is_none());
    }

    #[test]
    fn test_transition_module_to_done_explicit_substatus_wins() {
        // done 时显式传 substatus：清空在前、显式覆盖在后，最终应保留显式值。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Reviewing);
        module.substatus = Some("incomplete_stub".to_owned());
        m.update_module("a", module);
        m.transition_module(
            "a",
            Some(ModuleStatus::Done),
            Some("graduated_with_note"),
            None,
            false,
        )
        .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::Done);
        assert_eq!(module.substatus.as_deref(), Some("graduated_with_note"));
    }

    #[test]
    fn test_transition_degrade_preserves_substatus() {
        // degrade_* 的 substatus 含降级原因,Done 的清空逻辑不得误清。
        // 回归防护：若 `if target == Done` 误写为 `if target.is_terminal()`
        // (is_terminal 含 DegradeFfi/Manual/Skip),降级原因会被静默清空。
        let mut m = new_machine();
        let module = module_with_status(ModuleStatus::CompileFixing);
        m.update_module("a", module);
        m.transition_module("a", Some(ModuleStatus::Paused), None, None, false)
            .unwrap();
        m.transition_module(
            "a",
            Some(ModuleStatus::DegradeManual),
            Some("async_too_complex"),
            None,
            false,
        )
        .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.status, ModuleStatus::DegradeManual);
        assert_eq!(
            module.substatus.as_deref(),
            Some("async_too_complex"),
            "degrade_* 的 substatus(降级原因)必须保留"
        );
    }

    #[test]
    fn test_transition_module_leave_blocked_wrong_target_rejected() {
        // 离开 blocked 必须恢复到 pre_blocked_status，恢复到其他 blockable 态应报错。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Blocked);
        module.pre_blocked_status = Some(ModuleStatus::Testing);
        m.update_module("a", module);
        // 恢复到 translating（≠ pre_blocked_status=testing）应被拒。
        let err = m
            .transition_module("a", Some(ModuleStatus::Translating), None, None, false)
            .unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
        // 状态保持 blocked、锚点未被清除。
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Blocked);
        assert_eq!(
            m.state_file().modules["a"].pre_blocked_status,
            Some(ModuleStatus::Testing)
        );
        // 恢复到正确的 pre_blocked_status 成功。
        assert!(m
            .transition_module("a", Some(ModuleStatus::Testing), None, None, false)
            .is_ok());
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Testing);
    }

    #[test]
    fn test_transition_module_homomorphic_to_rejected() {
        // 显式 --to == 当前态：矩阵无自环，应报 InvalidTransition（保护终态/避免伪审计）。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Done));
        let err = m
            .transition_module("a", Some(ModuleStatus::Done), None, Some("noop"), false)
            .unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
        // done 模块未被追加伪审计记录。
        assert!(m.state_file().modules["a"].attempts.is_empty());
    }

    #[test]
    fn test_transition_module_leave_blocked_without_anchor() {
        // pre_blocked_status 缺失（如外部工具直接注入 blocked）：退化为只校验 blockable。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Blocked));
        // 恢复到任意 blockable 态（translating）成功。
        assert!(m
            .transition_module("a", Some(ModuleStatus::Translating), None, None, false)
            .is_ok());
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::Translating
        );

        // 恢复到非 blockable 态（done）应被矩阵拒绝。
        let mut m2 = new_machine();
        m2.update_module("b", module_with_status(ModuleStatus::Blocked));
        let err = m2
            .transition_module("b", Some(ModuleStatus::Done), None, None, false)
            .unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
    }

    #[test]
    fn test_transition_module_paused_paths() {
        // paused 是失败汇聚点 + 降级唯一入口，覆盖进入/降级/恢复三条边。
        // compile_fixing → paused（进入）。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::CompileFixing));
        assert!(m
            .transition_module("a", Some(ModuleStatus::Paused), None, None, false)
            .is_ok());
        // paused → degrade_manual（降级决策），不误触发 degrade 重置副作用。
        assert!(m
            .transition_module("a", Some(ModuleStatus::DegradeManual), None, None, false)
            .is_ok());
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::DegradeManual
        );

        // paused → translating（人类选择重试），非 degrade 来源不需 force。
        let mut m2 = new_machine();
        m2.update_module("b", module_with_status(ModuleStatus::Paused));
        assert!(m2
            .transition_module("b", Some(ModuleStatus::Translating), None, None, false)
            .is_ok());
        assert_eq!(
            m2.state_file().modules["b"].status,
            ModuleStatus::Translating
        );
    }

    #[test]
    fn test_load_falls_back_to_backup_on_corruption() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let m = new_machine();
        m.save(&path).expect("首次保存失败");
        // 二次保存会把首版内容备份到 .backup
        let mut m2 = m.clone();
        m2.transition(ProjectState::Profile).unwrap();
        m2.save(&path).expect("二次保存失败");

        // 模拟主文件被半截写入损坏
        std::fs::write(&path, b"{ broken json").unwrap();
        let loaded = MigrationStateMachine::load(&path).expect("应从 backup 恢复");
        // backup 是首次保存的 Init 状态
        assert_eq!(loaded.current_state(), ProjectState::Init);
        // 回退发生时标志置真，供 CLI 向用户告警进度可能丢失。
        assert!(
            loaded.recovered_from_backup(),
            "回退 backup 应置 recovered 标志"
        );
    }

    #[test]
    fn test_recovered_save_preserves_good_backup() {
        // 主文件损坏 → 从 backup 恢复 → 保存，不得用损坏 primary 覆盖有效 backup。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        let m = new_machine(); // Init
        m.save(&path).expect("首次保存");
        let mut m2 = m.clone();
        m2.transition(ProjectState::Profile).unwrap();
        m2.save(&path).expect("二次保存"); // backup 现为 Init 状态

        std::fs::write(&path, b"{ broken json").unwrap(); // 损坏主文件
        let mut recovered = MigrationStateMachine::load(&path).expect("从 backup 恢复");
        assert!(recovered.recovered_from_backup());
        assert_eq!(recovered.current_state(), ProjectState::Init); // backup 为 Init

        // 恢复后推进并保存（Init→Profile 合法）。
        recovered.transition(ProjectState::Profile).unwrap();
        recovered.save(&path).expect("恢复后保存");

        // 主文件现为有效新状态。
        let reloaded = MigrationStateMachine::load(&path).expect("重载主文件");
        assert_eq!(reloaded.current_state(), ProjectState::Profile);
        assert!(
            !reloaded.recovered_from_backup(),
            "重载主文件不应再标记 recovered"
        );

        // backup 必须仍是回退前的有效快照（可解析），而非被损坏 primary 覆盖。
        let backup_content = std::fs::read_to_string(&backup).expect("backup 应存在");
        serde_json::from_str::<MigrationStateFile>(&backup_content)
            .expect("backup 应仍是有效 JSON，未被损坏主文件覆盖");
    }

    #[test]
    fn test_normal_load_not_marked_recovered() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        new_machine().save(&path).expect("保存失败");
        let loaded = MigrationStateMachine::load(&path).expect("加载失败");
        assert!(
            !loaded.recovered_from_backup(),
            "正常加载不应标记 recovered"
        );
    }

    #[test]
    fn test_atomic_write_leaves_no_tmp() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("state.json");
        let m = new_machine();
        m.save(&path).unwrap();
        assert!(
            !sibling_with_suffix(&path, ".tmp").exists(),
            "不应残留 .tmp"
        );
    }

    #[test]
    fn test_load_both_corrupt_returns_primary_error() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let m = new_machine();
        m.save(&path).unwrap();
        m.save(&path).unwrap(); // 二次保存生成 .backup
                                // 主备双双损坏：应返回主文件（primary）的 Json 错误，不掩盖
        std::fs::write(&path, b"{ broken main").unwrap();
        std::fs::write(sibling_with_suffix(&path, ".backup"), b"{ broken backup").unwrap();
        match MigrationStateMachine::load(&path).unwrap_err() {
            MigrateError::Json(_) => {}
            other => panic!("期望主文件 Json 错误，实际: {other:?}"),
        }
    }

    #[test]
    fn test_backup_and_tmp_are_hidden_files() {
        // 对齐设计：tmp/backup 为前导点隐藏文件 `.migration-state.json.{tmp,backup}`。
        let path = std::path::Path::new("/tmp/migration-state.json");
        let backup = sibling_with_suffix(path, ".backup");
        let tmp = sibling_with_suffix(path, ".tmp");
        assert_eq!(
            backup.file_name().unwrap().to_string_lossy(),
            ".migration-state.json.backup"
        );
        assert_eq!(
            tmp.file_name().unwrap().to_string_lossy(),
            ".migration-state.json.tmp"
        );
    }

    #[test]
    fn test_transition_module_force_recovery_from_degrade() {
        // degrade_* 经 --force 恢复到 translating（设计恢复边）。
        for st in [
            ModuleStatus::DegradeFfi,
            ModuleStatus::DegradeManual,
            ModuleStatus::DegradeSkip,
        ] {
            let mut m = new_machine();
            m.update_module("a", module_with_status(st));
            assert!(
                m.transition_module("a", Some(ModuleStatus::Translating), None, None, true)
                    .is_ok(),
                "{st} 应允许恢复到 translating"
            );
            // 不带 force 应被拒（降级恢复须人类确认）。
            let mut m2 = new_machine();
            m2.update_module("b", module_with_status(st));
            assert!(
                matches!(
                    m2.transition_module("b", Some(ModuleStatus::Translating), None, None, false),
                    Err(MigrateError::Config(_))
                ),
                "{st} 不带 force 恢复应报 Config 错误"
            );
        }
    }

    // === M4-ROB-01a：reset_module（幂等回退失败/中途模块）===

    /// 辅助：构造一个「翻译中途、带各类进度字段」的模块（模拟失败/中断现场）。
    fn dirty_module(status: ModuleStatus) -> ModuleState {
        let mut m = module_with_status(status);
        m.substatus = Some("phase_a_in_progress".to_string());
        m.phase_a_version = Some("hash-abc".to_string());
        m.phase_a_audit_passed = Some(true);
        m.test_pass_rate = Some("0.5".to_string());
        m.coverage = Some(42);
        m.known_differences = 3;
        m.attempts.push(AttemptRecord {
            timestamp: Timestamp::new("2026-07-05T00:00:00Z".to_string()),
            result: "编译失败".to_string(),
            retry_count: 1,
            checkpoint: None,
        });
        m
    }

    #[test]
    fn test_reset_module_rolls_back_progress_fields() {
        // 中途失败模块 reset → translating + 全部进度字段清空，attempts 保留并追加 reset 记录，
        // 结构冻结字段（tier）不动。
        let mut m = new_machine();
        let mut dirty = dirty_module(ModuleStatus::CompileFixing);
        dirty.tier = Some(crate::types::state::ModuleTier::Standard);
        m.update_module("a", dirty);

        let out = m.reset_module("a", false).expect("非终态 reset 应成功");
        assert_eq!(out.reset_from, ModuleStatus::CompileFixing);
        assert_eq!(out.reset_to, ModuleStatus::Translating);
        assert!(!out.was_noop);
        assert_eq!(out.module, "a");
        assert_eq!(out.member_files, vec!["a".to_string()]); // 单文件 → [module]

        let md = &m.state_file().modules["a"];
        assert_eq!(md.status, ModuleStatus::Translating);
        assert!(md.substatus.is_none());
        assert!(md.phase_a_version.is_none());
        assert!(md.phase_a_audit_passed.is_none());
        assert!(md.test_pass_rate.is_none());
        assert!(md.coverage.is_none());
        assert_eq!(md.known_differences, 0);
        // attempts：既有 1 条 + reset 审计 1 条 = 2；tier 冻结字段保留。
        assert_eq!(md.attempts.len(), 2);
        assert_eq!(md.attempts[1].result, "reset:compile_fixing→translating");
        assert_eq!(md.tier, Some(crate::types::state::ModuleTier::Standard));
    }

    #[test]
    fn test_reset_module_idempotent_noop() {
        // 已在干净重译入口（translating/null）→ 空操作：was_noop=true、不追加审计、字段不变。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Translating));

        let out = m.reset_module("a", false).expect("干净入口 reset 应成功");
        assert!(out.was_noop);
        assert_eq!(out.reset_from, ModuleStatus::Translating);
        assert_eq!(out.reset_to, ModuleStatus::Translating);
        assert!(m.state_file().modules["a"].attempts.is_empty());

        // reset;reset 收敛：先 reset 脏模块，再 reset 应为 noop，两次后状态与一次一致。
        let mut m2 = new_machine();
        m2.update_module("b", dirty_module(ModuleStatus::Testing));
        let first = m2.reset_module("b", false).unwrap();
        assert!(!first.was_noop);
        let after_first = m2.state_file().modules["b"].clone();
        let second = m2.reset_module("b", false).unwrap();
        assert!(second.was_noop, "第二次 reset 应为幂等空操作");
        assert_eq!(
            &after_first,
            &m2.state_file().modules["b"],
            "reset;reset 状态应与 reset 一致（无多余审计）"
        );
    }

    #[test]
    fn test_reset_module_pending_stays_pending_noop() {
        // pending 尚未起步、无产物：reset 为 noop，保持 pending（不前移 translating）。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        let out = m.reset_module("a", false).expect("pending reset 应成功");
        assert!(out.was_noop);
        assert_eq!(out.reset_to, ModuleStatus::Pending);
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Pending);
    }

    #[test]
    fn test_reset_module_guards_terminal_and_anchor_without_force() {
        // done / blocked / paused / degrade_* 不带 --force 应报 Config 错误（守护终态/锚点/决策点）。
        for st in [
            ModuleStatus::Done,
            ModuleStatus::Blocked,
            ModuleStatus::Paused,
            ModuleStatus::DegradeFfi,
            ModuleStatus::DegradeManual,
            ModuleStatus::DegradeSkip,
        ] {
            let mut m = new_machine();
            m.update_module("a", module_with_status(st));
            assert!(
                matches!(m.reset_module("a", false), Err(MigrateError::Config(_))),
                "{st} 不带 force reset 应报 Config 错误"
            );
            // 状态未被改动（守护生效）。
            assert_eq!(m.state_file().modules["a"].status, st);
        }
    }

    #[test]
    fn test_reset_module_paused_force_recovers() {
        // paused + --force：允许回退到 translating（人类显式确认「就是要重试」）。
        let mut m = new_machine();
        m.update_module("a", dirty_module(ModuleStatus::Paused));
        let out = m.reset_module("a", true).expect("paused + force 应成功");
        assert_eq!(out.reset_from, ModuleStatus::Paused);
        assert_eq!(out.reset_to, ModuleStatus::Translating);
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::Translating
        );
    }

    #[test]
    fn test_reset_module_rejected_in_graduate_even_with_force() {
        // 项目级守护：graduate 下 reset 一律拒绝（含 --force），避免制造矛盾终态。
        let mut m = new_machine();
        // 推进到 graduate（init→…→graduate）。
        for st in [
            ProjectState::Profile,
            ProjectState::Plan,
            ProjectState::Scaffold,
            ProjectState::SprintLoop,
            ProjectState::Graduate,
        ] {
            m.transition(st).unwrap();
        }
        m.update_module("a", module_with_status(ModuleStatus::Done));
        // 即使带 --force 也拒绝。
        assert!(
            matches!(m.reset_module("a", true), Err(MigrateError::Config(_))),
            "graduate 下 reset --force 应报 Config 错误"
        );
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Done);
    }

    #[test]
    fn test_record_metrics_updates_values_without_state_side_effects() {
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Testing);
        module.attempts.push(AttemptRecord {
            timestamp: Timestamp::from("2026-07-21T00:00:00Z"),
            result: "existing".to_string(),
            retry_count: 0,
            checkpoint: None,
        });
        m.update_module("a", module);

        m.record_metrics("a", Some("276/276"), Some(0))
            .expect("记录度量应成功");
        let recorded = &m.state_file().modules["a"];
        assert_eq!(recorded.test_pass_rate.as_deref(), Some("276/276"));
        assert_eq!(recorded.known_differences, 0);
        assert_eq!(recorded.status, ModuleStatus::Testing);
        assert_eq!(recorded.attempts.len(), 2);
        assert_eq!(recorded.attempts[0].result, "existing");
        assert!(recorded.attempts[1].result.starts_with("metrics:"));
    }

    #[test]
    fn test_record_metrics_supports_partial_updates() {
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Testing);
        module.test_pass_rate = Some("90%".to_string());
        module.known_differences = 2;
        m.update_module("a", module);

        let outcome = m.record_metrics("a", None, Some(1)).unwrap();
        assert_eq!(outcome.module, "a");
        assert_eq!(outcome.test_pass_rate.as_deref(), Some("90%"));
        assert_eq!(outcome.known_differences, 1);
        assert_eq!(
            m.state_file().modules["a"].test_pass_rate.as_deref(),
            Some("90%")
        );
        assert_eq!(m.state_file().modules["a"].known_differences, 1);
    }

    #[test]
    fn test_record_metrics_rejects_invalid_rate_without_mutation() {
        for invalid in ["garbage", "101%", "1/0", "-1"] {
            let mut m = new_machine();
            let mut module = module_with_status(ModuleStatus::Testing);
            module.test_pass_rate = Some("90%".to_string());
            m.update_module("a", module);

            let result = m.record_metrics("a", Some(invalid), Some(3));
            assert!(
                matches!(result, Err(MigrateError::Config(_))),
                "{invalid} 应被拒绝"
            );
            let unchanged = &m.state_file().modules["a"];
            assert_eq!(unchanged.test_pass_rate.as_deref(), Some("90%"));
            assert_eq!(unchanged.known_differences, 0);
        }
    }

    #[test]
    fn test_record_metrics_rejects_non_running_and_graduate_states() {
        for status in [
            ModuleStatus::Pending,
            ModuleStatus::Done,
            ModuleStatus::Paused,
            ModuleStatus::Blocked,
            ModuleStatus::DegradeSkip,
        ] {
            let mut m = new_machine();
            m.update_module("a", module_with_status(status));
            assert!(
                matches!(
                    m.record_metrics("a", Some("100%"), Some(0)),
                    Err(MigrateError::Config(_))
                ),
                "{status} 不应允许写度量"
            );
        }

        let mut graduated = new_machine();
        graduated.update_module("a", module_with_status(ModuleStatus::Reviewing));
        for state in [
            ProjectState::Profile,
            ProjectState::Plan,
            ProjectState::Scaffold,
            ProjectState::SprintLoop,
            ProjectState::Graduate,
        ] {
            graduated.transition(state).unwrap();
        }
        assert!(matches!(
            graduated.record_metrics("a", Some("100%"), Some(0)),
            Err(MigrateError::Config(_))
        ));
    }

    #[test]
    fn test_record_metrics_normalizes_composite_member() {
        let mut m = new_machine();
        let mut group = module_with_status(ModuleStatus::Testing);
        group.member_files = Some(vec!["group".to_string(), "file:helper.ts".to_string()]);
        m.update_module("group", group);

        let outcome = m
            .record_metrics("file:helper.ts", Some("1.0"), None)
            .expect("非代表成员应归一到组代表");
        assert_eq!(outcome.module, "group");
        assert_eq!(
            m.state_file().modules["group"].test_pass_rate.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn test_canonical_module_key_rejects_broken_partition() {
        // member_files 跨组互斥不变量被破坏（同一文件属多组）→ release 也硬错（非静默取首个）。
        let mut m = new_machine();
        let mut g1 = module_with_status(ModuleStatus::Pending);
        g1.member_files = Some(vec!["g1".to_string(), "file:shared.ts".to_string()]);
        let mut g2 = module_with_status(ModuleStatus::Pending);
        g2.member_files = Some(vec!["g2".to_string(), "file:shared.ts".to_string()]);
        m.update_module("g1", g1);
        m.update_module("g2", g2);
        // 对同属两组的成员发 reset：应报「不变量被破坏」而非静默错归组。
        let err = m.reset_module("file:shared.ts", false);
        assert!(
            matches!(&err, Err(MigrateError::Config(msg)) if msg.contains("不变量被破坏")),
            "破坏的划分应硬错: {err:?}"
        );
    }

    #[test]
    fn test_reset_module_force_recovers_terminal_and_anchor() {
        // 带 --force：done/blocked/degrade_* 均可回退到 translating（blocked 锚点一并清）。
        let mut m = new_machine();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["file:dep.ts".to_string()]);
        blocked.pre_blocked_status = Some(ModuleStatus::Translating);
        m.update_module("a", blocked);
        let out = m
            .reset_module("a", true)
            .expect("force reset blocked 应成功");
        assert_eq!(out.reset_to, ModuleStatus::Translating);
        let md = &m.state_file().modules["a"];
        assert_eq!(md.status, ModuleStatus::Translating);
        assert!(md.blocked_by.is_none());
        assert!(md.pre_blocked_status.is_none());

        for st in [ModuleStatus::Done, ModuleStatus::DegradeSkip] {
            let mut m2 = new_machine();
            m2.update_module("b", module_with_status(st));
            let o = m2.reset_module("b", true).expect("force reset 终态应成功");
            assert_eq!(o.reset_from, st);
            assert_eq!(o.reset_to, ModuleStatus::Translating);
        }
    }

    #[test]
    fn test_reset_module_normalizes_non_representative_member() {
        // 传折叠组的非代表成员 key：归一到组代表，member_files 作用域回传全组成员。
        let mut m = new_machine();
        let mut grp = dirty_module(ModuleStatus::Testing);
        grp.member_files = Some(vec!["grp".to_string(), "file:helper.ts".to_string()]);
        m.update_module("grp", grp);

        let out = m
            .reset_module("file:helper.ts", false)
            .expect("成员 reset 应归一成功");
        assert_eq!(out.module, "grp");
        assert_eq!(
            out.member_files,
            vec!["grp".to_string(), "file:helper.ts".to_string()]
        );
        assert_eq!(
            m.state_file().modules["grp"].status,
            ModuleStatus::Translating
        );
        assert!(!m.state_file().modules.contains_key("file:helper.ts"));
    }

    #[test]
    fn test_reset_module_missing() {
        let mut m = new_machine();
        assert!(matches!(
            m.reset_module("nonexistent", false),
            Err(MigrateError::Config(_))
        ));
    }

    // === M4-ROB-01b：recover_module（watchdog stall 恢复）===

    #[test]
    fn test_recover_retry_rolls_back_and_records() {
        // retry 策略：委派 reset（force）回退 + 追加 stall-recover:retry 审计（保留 reset 审计）。
        let mut m = new_machine();
        m.update_module("a", dirty_module(ModuleStatus::CompileFixing));
        let out = m
            .recover_module("a", RecoverPolicy::Retry, Some("stdout 静默 620s"))
            .expect("retry 恢复应成功");
        assert_eq!(out.policy, RecoverPolicy::Retry);
        assert_eq!(out.from, ModuleStatus::CompileFixing);
        assert_eq!(out.to, ModuleStatus::Translating);
        assert!(!out.was_noop);
        assert_eq!(out.member_files, vec!["a".to_string()]);

        let md = &m.state_file().modules["a"];
        assert_eq!(md.status, ModuleStatus::Translating);
        assert!(md.substatus.is_none());
        // attempts：既有 1 + reset 审计 1 + stall-recover 审计 1 = 3。
        assert_eq!(md.attempts.len(), 3);
        assert_eq!(md.attempts[1].result, "reset:compile_fixing→translating");
        assert_eq!(
            md.attempts[2].result,
            "stall-recover:retry reason=stdout 静默 620s"
        );
    }

    #[test]
    fn test_recover_retry_from_paused_forces_through() {
        // stall 常把模块留在 paused（重试耗尽）：retry 应跨守护 force-reset 回 translating。
        let mut m = new_machine();
        m.update_module("a", dirty_module(ModuleStatus::Paused));
        let out = m
            .recover_module("a", RecoverPolicy::Retry, None)
            .expect("paused retry 恢复应成功");
        assert_eq!(out.to, ModuleStatus::Translating);
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::Translating
        );
        // 无 reason 时审计后缀为空。
        let attempts = &m.state_file().modules["a"].attempts;
        assert_eq!(attempts.last().unwrap().result, "stall-recover:retry");
    }

    #[test]
    fn test_recover_skip_sets_paused_bypassing_matrix() {
        // skip 策略：stall 发生在 translating（Phase A），translating→paused 不在转换矩阵，
        // recover 须绕过矩阵直设 paused（决策点）。
        let mut m = new_machine();
        assert!(
            !ModuleStatus::Translating.can_transition_to(ModuleStatus::Paused),
            "前提：translating→paused 不在矩阵"
        );
        m.update_module("a", dirty_module(ModuleStatus::Translating));
        let out = m
            .recover_module("a", RecoverPolicy::Skip, Some("stall"))
            .expect("skip 恢复应成功");
        assert_eq!(out.policy, RecoverPolicy::Skip);
        assert_eq!(out.from, ModuleStatus::Translating);
        assert_eq!(out.to, ModuleStatus::Paused);
        assert!(!out.was_noop);
        let md = &m.state_file().modules["a"];
        assert_eq!(md.status, ModuleStatus::Paused);
        // substatus（translating 瞬态标记）清空；进度字段保留供降级分析。
        assert!(md.substatus.is_none(), "skip 应清 substatus（语义不符）");
        assert_eq!(
            md.phase_a_version,
            Some("hash-abc".to_string()),
            "进度字段保留"
        );
        assert_eq!(md.coverage, Some(42), "进度字段保留");
        assert_eq!(
            md.attempts.last().unwrap().result,
            "stall-recover:skip reason=stall"
        );
    }

    #[test]
    fn test_recover_idempotent_noop() {
        // retry 已在干净入口 → noop（复用 reset 幂等，不追加审计）。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Translating));
        let out = m
            .recover_module("a", RecoverPolicy::Retry, Some("x"))
            .unwrap();
        assert!(out.was_noop);
        assert!(m.state_file().modules["a"].attempts.is_empty());

        // skip 已 paused → noop（不重复置态/记录）。recover;recover == recover。
        let mut m2 = new_machine();
        m2.update_module("b", dirty_module(ModuleStatus::Testing));
        let first = m2
            .recover_module("b", RecoverPolicy::Skip, Some("s"))
            .unwrap();
        assert!(!first.was_noop);
        let after_first = m2.state_file().modules["b"].clone();
        let second = m2
            .recover_module("b", RecoverPolicy::Skip, Some("s"))
            .unwrap();
        assert!(second.was_noop, "第二次 skip 应幂等 noop");
        assert_eq!(
            &after_first,
            &m2.state_file().modules["b"],
            "recover;recover 状态应一致（无多余审计）"
        );
    }

    #[test]
    fn test_recover_rejects_degrade_no_force_bypass() {
        // degrade_* 是人类降级决策——recover 两策略均拒绝（防 retry 绕 --force 撤销降级、
        // 防 retry;skip 把依赖侧已视终态的模块变回非终态）。如需恢复走 state reset --force。
        for st in [
            ModuleStatus::DegradeSkip,
            ModuleStatus::DegradeFfi,
            ModuleStatus::DegradeManual,
        ] {
            for policy in [RecoverPolicy::Retry, RecoverPolicy::Skip] {
                let mut m = new_machine();
                m.update_module("a", module_with_status(st));
                assert!(
                    matches!(
                        m.recover_module("a", policy, None),
                        Err(MigrateError::Config(_))
                    ),
                    "{st} + {policy:?} recover 应拒绝（不撤销人类降级）"
                );
                assert_eq!(m.state_file().modules["a"].status, st, "守护应保状态不变");
            }
        }
    }

    #[test]
    fn test_recover_guards_non_stall_states() {
        // pending（未起步）/ done（终态）/ blocked（等依赖）非 stall 态 → 两策略均拒绝、状态不变。
        for st in [
            ModuleStatus::Pending,
            ModuleStatus::Done,
            ModuleStatus::Blocked,
        ] {
            for policy in [RecoverPolicy::Retry, RecoverPolicy::Skip] {
                let mut m = new_machine();
                m.update_module("a", module_with_status(st));
                assert!(
                    matches!(
                        m.recover_module("a", policy, None),
                        Err(MigrateError::Config(_))
                    ),
                    "{st} + {policy:?} recover 应报 Config 错误"
                );
                assert_eq!(m.state_file().modules["a"].status, st, "守护应保状态不变");
            }
        }
    }

    #[test]
    fn test_recover_rejected_in_graduate() {
        // 项目级守护：graduate 下 recover 一律拒绝（含两策略）。
        let mut m = new_machine();
        for st in [
            ProjectState::Profile,
            ProjectState::Plan,
            ProjectState::Scaffold,
            ProjectState::SprintLoop,
            ProjectState::Graduate,
        ] {
            m.transition(st).unwrap();
        }
        m.update_module("a", dirty_module(ModuleStatus::Translating));
        assert!(matches!(
            m.recover_module("a", RecoverPolicy::Retry, None),
            Err(MigrateError::Config(_))
        ));
        assert!(matches!(
            m.recover_module("a", RecoverPolicy::Skip, None),
            Err(MigrateError::Config(_))
        ));
    }

    #[test]
    fn test_recover_normalizes_member_and_scope() {
        // 传折叠组非代表成员：归一到组代表；skip 回传全组 member_files 作用域。
        let mut m = new_machine();
        let mut grp = dirty_module(ModuleStatus::Testing);
        grp.member_files = Some(vec!["grp".to_string(), "file:helper.ts".to_string()]);
        m.update_module("grp", grp);
        let out = m
            .recover_module("file:helper.ts", RecoverPolicy::Skip, None)
            .expect("成员 recover 应归一成功");
        assert_eq!(out.module, "grp");
        assert_eq!(
            out.member_files,
            vec!["grp".to_string(), "file:helper.ts".to_string()]
        );
        assert_eq!(m.state_file().modules["grp"].status, ModuleStatus::Paused);
    }

    #[test]
    fn test_full_field_round_trip_preserves_all_fields() {
        // M4-ROB-01a checkpoint 硬化：全字段填满 → save（tmp-fsync-rename 原子写）→ load →
        // 逐字段相等。钉住「save/load 不丢字段」——防止未来新增字段漏进序列化/反序列化路径。
        use crate::types::common::DangerCategory;
        use crate::types::state::{CompositeKind, ModuleTier, SubAgentCall};

        let module = ModuleState {
            status: ModuleStatus::Reviewing,
            substatus: Some("incomplete".to_string()),
            sprint: Some(2),
            attempts: vec![AttemptRecord {
                timestamp: Timestamp::new("2026-07-05T01:00:00Z".to_string()),
                result: "编译失败".to_string(),
                retry_count: 2,
                checkpoint: Some("cp-1".to_string()),
            }],
            test_pass_rate: Some("0.95".to_string()),
            coverage: Some(88),
            known_differences: 4,
            tier: Some(ModuleTier::Full),
            phase_a_version: Some("hash-a".to_string()),
            phase_a_audit_passed: Some(true),
            blocked_by: Some(vec!["file:dep.ts".to_string()]),
            pre_blocked_status: Some(ModuleStatus::Testing),
            member_files: Some(vec!["file:a.ts".to_string(), "file:b.ts".to_string()]),
            composite_kind: Some(CompositeKind::CoupledBatch),
            decomposition_snapshot: Some("snap-hash".to_string()),
            decomposition_frozen: true,
            danger: vec![DangerCategory::Concurrency, DangerCategory::Ffi],
        };

        let original = MigrationStateFile {
            version: STATE_SCHEMA_VERSION.to_string(),
            state: ProjectState::SprintLoop,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: Timestamp::new("2026-07-05T00:00:00Z".to_string()),
                exited_at: Some(Timestamp::new("2026-07-05T00:10:00Z".to_string())),
            }],
            project: Some(ProjectInfo {
                name: "p".to_string(),
                source_language: SourceLang::TypeScript,
                source_commit: Some("abc123".to_string()),
                source_loc: 1234,
                created_at: Timestamp::new("2026-07-05T00:00:00Z".to_string()),
            }),
            sprint: Some(SprintState {
                current: 2,
                history: vec![SprintEntry {
                    id: 1,
                    started_at: Timestamp::new("2026-07-05T00:00:00Z".to_string()),
                    completed_at: Some(Timestamp::new("2026-07-05T00:30:00Z".to_string())),
                    target_modules: vec!["file:a.ts".to_string()],
                    completed_modules: vec!["file:a.ts".to_string()],
                    notes: Some("done".to_string()),
                    porting_md_version: Some("v2".to_string()),
                }],
            }),
            modules: std::collections::HashMap::from([("grp".to_string(), module)]),
            config_ref: Some(".rustmigrate.toml".to_string()),
            subagent_calls: vec![SubAgentCall {
                step_index: 6,
                subagent_name: "translator".to_string(),
                started_at: Timestamp::new("2026-07-05T00:05:00Z".to_string()),
                ended_at: Some(Timestamp::new("2026-07-05T00:06:00Z".to_string())),
                status: "success".to_string(),
                error_message: Some("none".to_string()),
            }],
            metadata: Some(MigrationMetadata {
                graph_build_completed: true,
                graph_build_completed_at: Some(Timestamp::new("2026-07-05T00:02:00Z".to_string())),
                last_error: Some("transient".to_string()),
                lock_token: Some("tok".to_string()),
                version: 7,
                last_modified_by: Some("me".to_string()),
            }),
        };

        let machine = MigrationStateMachine {
            state_file: original.clone(),
            recovered_from_backup: false,
            persistence_config: PersistenceConfig::default(),
        };
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        machine.save(&path).expect("save 应成功");
        let loaded = MigrationStateMachine::load(&path).expect("load 应成功");
        assert_eq!(
            loaded.state_file(),
            &original,
            "save→load 应逐字段还原，无字段丢失（checkpoint 硬化）"
        );
    }

    /// VER-05：load 对含非法 timestamp 的合法 JSON 应加载失败（自定义 Deserialize 拒非法值 → Json 错误）。
    #[test]
    fn test_load_invalid_timestamp_rejected() {
        // 构造合法 JSON 结构，但 state_history[0].entered_at 值非法。
        let json = r#"{
            "schema_version": "1.0.0",
            "state": "init",
            "state_history": [
                {
                    "state": "init",
                    "entered_at": "not-a-timestamp"
                }
            ],
            "modules": {}
        }"#;
        let mut tmp = NamedTempFile::new().expect("创建临时文件失败");
        tmp.write_all(json.as_bytes()).unwrap();
        tmp.flush().unwrap();
        let result = MigrationStateMachine::load(tmp.path());
        assert!(result.is_err(), "含非法 timestamp 的状态文件应加载失败");
        // 非法 timestamp 在 Timestamp 自定义 Deserialize 层被拒 → serde 错误 → Json 变体。
        // load() 对 Json 损坏会尝试 backup 回退，本例无 backup，返回 primary（Json）。
        match result.unwrap_err() {
            MigrateError::Json(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("时间戳") || msg.contains("not-a-timestamp"),
                    "错误消息应指出时间戳格式问题，实际: {msg}"
                );
            }
            other => panic!("期望 Json（含时间戳格式错误），实际: {:?}", other),
        }
    }

    /// VER-05：合法 timestamp 的状态文件加载成功（正向路径回归）。
    #[test]
    fn test_load_valid_timestamps_accepted() {
        let m = new_machine();
        let tmp = NamedTempFile::new().expect("创建临时文件失败");
        let path = tmp.path().to_owned();
        m.save(&path).expect("保存失败");
        // chrono 生成的 RFC 3339 时间戳应通过校验。
        let loaded = MigrationStateMachine::load(&path);
        assert!(loaded.is_ok(), "合法 timestamp 的状态文件应加载成功");
    }

    #[test]
    fn test_advance_sprint_all_terminal() {
        let mut m = new_machine();
        // 模拟 populate 分配了 2 个 sprint。
        m.update_module("a", {
            let mut ms = module_with_status(ModuleStatus::Done);
            ms.sprint = Some(1);
            ms
        });
        m.update_module("b", {
            let mut ms = module_with_status(ModuleStatus::Pending);
            ms.sprint = Some(2);
            ms
        });
        m.set_sprint(SprintState {
            current: 1,
            history: vec![SprintEntry {
                id: 1,
                started_at: Timestamp::new("2026-06-17T00:00:00Z"),
                completed_at: None,
                target_modules: vec!["a".to_owned()],
                completed_modules: Vec::new(),
                notes: None,
                porting_md_version: None,
            }],
        });
        // sprint 1 全终态 → 应推进到 2。
        assert_eq!(m.try_advance_sprint(), SprintAdvanceResult::Advanced(2));
        assert_eq!(m.state_file().sprint.as_ref().unwrap().current, 2);
        // history 应有 2 条记录。
        assert_eq!(m.state_file().sprint.as_ref().unwrap().history.len(), 2);
        // sprint 1 应有 completed_at。
        assert!(m.state_file().sprint.as_ref().unwrap().history[0]
            .completed_at
            .is_some());
    }

    #[test]
    fn test_advance_sprint_not_all_terminal() {
        let mut m = new_machine();
        m.update_module("a", {
            let mut ms = module_with_status(ModuleStatus::Translating);
            ms.sprint = Some(1);
            ms
        });
        m.set_sprint(SprintState {
            current: 1,
            history: Vec::new(),
        });
        // sprint 1 有非终态模块 → 不推进。
        assert!(matches!(
            m.try_advance_sprint(),
            SprintAdvanceResult::NotReady
        ));
        assert_eq!(m.state_file().sprint.as_ref().unwrap().current, 1);
    }

    #[test]
    fn test_advance_sprint_no_next_sprint() {
        let mut m = new_machine();
        // 只有 sprint 1 的模块且全终态，但无 sprint 2 模块。
        m.update_module("a", {
            let mut ms = module_with_status(ModuleStatus::Done);
            ms.sprint = Some(1);
            ms
        });
        m.set_sprint(SprintState {
            current: 1,
            history: Vec::new(),
        });
        // 无下一 sprint → 全部完成。
        assert!(matches!(
            m.try_advance_sprint(),
            SprintAdvanceResult::AllCompleted
        ));
    }

    #[test]
    fn test_advance_sprint_degrade_counts_as_terminal() {
        let mut m = new_machine();
        m.update_module("a", {
            let mut ms = module_with_status(ModuleStatus::DegradeFfi);
            ms.sprint = Some(1);
            ms
        });
        m.update_module("b", {
            let mut ms = module_with_status(ModuleStatus::Pending);
            ms.sprint = Some(2);
            ms
        });
        m.set_sprint(SprintState {
            current: 1,
            history: Vec::new(),
        });
        // degrade 是终态 → 应推进。
        assert_eq!(m.try_advance_sprint(), SprintAdvanceResult::Advanced(2));
    }

    #[test]
    fn test_save_backup_on_write_true_creates_backup() {
        // 默认 backup_on_write=true：二次保存应生成 .backup。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        let m = new_machine();
        m.save(&path).unwrap(); // 首次保存，无需备份
        assert!(!backup.exists(), "首次保存不应生成 backup（无原文件）");

        m.save(&path).unwrap(); // 二次保存，应备份旧文件
        assert!(
            backup.exists(),
            "backup_on_write=true 时二次保存应生成 .backup"
        );
    }

    #[test]
    fn test_save_backup_on_write_false_skips_backup() {
        // backup_on_write=false：保存不应生成 .backup。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        let mut m = new_machine();
        m.set_persistence_config(PersistenceConfig {
            backup_on_write: false,
            retention_days: None,
        });
        m.save(&path).unwrap();
        m.save(&path).unwrap(); // 二次保存也不备份
        assert!(!backup.exists(), "backup_on_write=false 时不应生成 .backup");
    }

    #[test]
    fn test_save_retention_days_cleans_old_backup() {
        // retention_days=0 时，任何已有 backup 都应被清理。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        let mut m = new_machine();
        m.save(&path).unwrap(); // 首次保存
        m.save(&path).unwrap(); // 二次保存生成 .backup
        assert!(backup.exists(), "预置：backup 应存在");

        // 设置 retention_days=0，再 save 一次触发清理。
        m.set_persistence_config(PersistenceConfig {
            backup_on_write: true,
            retention_days: Some(0),
        });
        m.save(&path).unwrap();
        // retention_days=0：save 先用 atomic_write 创建新 backup（backup_on_write=true），
        // 随后 cleanup_old_backups 检查 age > 0 秒。刚创建的 backup age 至少有几微秒，
        // 所以会被清理掉。
        assert!(
            !backup.exists(),
            "retention_days=0 时 save 后 backup 应被清理"
        );
    }

    #[test]
    fn test_save_retention_days_none_keeps_backup() {
        // retention_days=None（默认）：不清理 backup。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        let m = new_machine(); // 默认 persistence_config
        m.save(&path).unwrap();
        m.save(&path).unwrap();
        assert!(backup.exists(), "backup 应存在");

        m.save(&path).unwrap(); // 再次保存，retention_days=None 不清理
        assert!(backup.exists(), "retention_days=None 时 backup 不应被清理");
    }

    #[test]
    fn test_cleanup_old_backups_removes_expired() {
        // 直接测试 cleanup_old_backups 函数。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        std::fs::write(&backup, b"old backup content").unwrap();

        // retention_days=36500（100年）：刚创建的 backup 不应被清理。
        cleanup_old_backups(&path, 36500);
        assert!(backup.exists(), "retention_days=36500 时 backup 不应被清理");

        // retention_days=0：阈值为 0 秒，刚创建的文件 age > 0（至少 1 微秒），应被清理。
        cleanup_old_backups(&path, 0);
        assert!(
            !backup.exists(),
            "retention_days=0 时 backup 应被清理（age > 0 秒阈值）"
        );
    }

    #[test]
    fn test_set_persistence_config() {
        // 验证 set_persistence_config 正确注入。
        let mut m = new_machine();
        // 默认值。
        m.set_persistence_config(PersistenceConfig {
            backup_on_write: false,
            retention_days: Some(7),
        });
        // 通过 save 行为间接验证已注入。
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("state.json");
        let backup = sibling_with_suffix(&path, ".backup");
        m.save(&path).unwrap();
        m.save(&path).unwrap();
        assert!(
            !backup.exists(),
            "set_persistence_config(backup_on_write=false) 应阻止备份"
        );
    }

    #[test]
    fn test_persistence_config_default_backward_compatible() {
        // 默认 PersistenceConfig 行为与改动前一致：backup_on_write=true, retention_days=None。
        let m = new_machine();
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("migration-state.json");
        let backup = sibling_with_suffix(&path, ".backup");

        m.save(&path).unwrap();
        m.save(&path).unwrap();
        assert!(backup.exists(), "默认配置应生成 backup（向后兼容）");
    }

    // ===== 两层 done 协议（M2-SCALE-02e）=====

    #[test]
    fn test_is_agent_done_true() {
        // substatus 为 agent_done 时返回 true。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Reviewing);
        module.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
        m.update_module("a", module);
        assert!(m.is_agent_done("a"));
    }

    #[test]
    fn test_is_agent_done_false_different_substatus() {
        // substatus 非 agent_done 时返回 false。
        let mut m = new_machine();
        let mut module = module_with_status(ModuleStatus::Reviewing);
        module.substatus = Some("phase_a_complete_awaiting_review".to_owned());
        m.update_module("a", module);
        assert!(!m.is_agent_done("a"));
    }

    #[test]
    fn test_is_agent_done_false_no_substatus() {
        // substatus 为 None 时返回 false。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Reviewing));
        assert!(!m.is_agent_done("a"));
    }

    #[test]
    fn test_is_agent_done_nonexistent_module() {
        // 模块不存在时返回 false。
        let m = new_machine();
        assert!(!m.is_agent_done("not_exist"));
    }

    #[test]
    fn test_batch_transition_done_all_success() {
        // 全部模块 reviewing + agent_done → 应全部成功转为 done。
        let mut m = new_machine();
        for name in ["a", "b", "c"] {
            let mut module = module_with_status(ModuleStatus::Reviewing);
            module.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
            m.update_module(name, module);
        }
        let modules: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let succeeded = m.batch_transition_done(&modules).unwrap();
        assert_eq!(succeeded.len(), 3);
        for name in ["a", "b", "c"] {
            assert_eq!(m.state_file().modules[name].status, ModuleStatus::Done);
            // done 时 substatus 被清空（transition_module 的 Done 清空逻辑）。
            assert!(m.state_file().modules[name].substatus.is_none());
        }
    }

    #[test]
    fn test_batch_transition_done_partial_failure() {
        // a: reviewing + agent_done（应成功）
        // b: translating + agent_done（status 不对，转换矩阵拒绝，应失败但不影响 a、c）
        // c: reviewing + agent_done（应成功）
        let mut m = new_machine();

        let mut ma = module_with_status(ModuleStatus::Reviewing);
        ma.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
        m.update_module("a", ma);

        let mut mb = module_with_status(ModuleStatus::Translating);
        mb.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
        m.update_module("b", mb);

        let mut mc = module_with_status(ModuleStatus::Reviewing);
        mc.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
        m.update_module("c", mc);

        let modules: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let succeeded = m.batch_transition_done(&modules).unwrap();

        // a、c 成功，b 失败。
        assert_eq!(succeeded, vec!["a".to_owned(), "c".to_owned()]);
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Done);
        assert_eq!(
            m.state_file().modules["b"].status,
            ModuleStatus::Translating,
            "b 应保持 translating（转换失败）"
        );
        assert_eq!(m.state_file().modules["c"].status, ModuleStatus::Done);
    }

    #[test]
    fn test_batch_transition_done_skips_non_agent_done() {
        // substatus 非 agent_done 的模块应被跳过。
        let mut m = new_machine();

        let mut ma = module_with_status(ModuleStatus::Reviewing);
        ma.substatus = Some(SUBSTATUS_AGENT_DONE.to_owned());
        m.update_module("a", ma);

        let mb = module_with_status(ModuleStatus::Reviewing); // substatus=None
        m.update_module("b", mb);

        let modules: Vec<String> = vec!["a".into(), "b".into()];
        let succeeded = m.batch_transition_done(&modules).unwrap();

        assert_eq!(succeeded, vec!["a".to_owned()]);
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Done);
        assert_eq!(
            m.state_file().modules["b"].status,
            ModuleStatus::Reviewing,
            "b 应保持 reviewing（非 agent_done 跳过）"
        );
    }

    #[test]
    fn test_batch_transition_done_empty_list() {
        // 空列表应返回空结果。
        let mut m = new_machine();
        let succeeded = m.batch_transition_done(&[]).unwrap();
        assert!(succeeded.is_empty());
    }

    #[test]
    fn test_agent_done_substatus_set_via_transition_module() {
        // 通过 transition_module 的 substatus-only 路径设置 agent_done。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Reviewing));
        m.transition_module("a", None, Some(SUBSTATUS_AGENT_DONE), None, false)
            .unwrap();
        assert!(m.is_agent_done("a"));
        // status 不变。
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Reviewing);
    }

    #[test]
    fn test_metadata_version_default_zero() {
        // 新建状态机的 metadata.version 默认为 0。
        let m = new_machine();
        assert_eq!(m.metadata_version(), 0);
    }

    #[test]
    fn test_update_with_cas_success() {
        // CAS 版本匹配：状态转换成功并递增版本号。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        let (prev, new_ver) = m
            .update_with_cas("a", ModuleStatus::Translating, 0, None, None)
            .expect("CAS 版本匹配应成功");
        assert_eq!(prev, ModuleStatus::Pending);
        assert_eq!(new_ver, 1);
        assert_eq!(
            m.state_file().modules["a"].status,
            ModuleStatus::Translating
        );
        assert_eq!(m.metadata_version(), 1);
    }

    #[test]
    fn test_update_with_cas_version_mismatch() {
        // CAS 版本不匹配：返回 LockConflict，状态不变。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        let err = m
            .update_with_cas("a", ModuleStatus::Translating, 42, None, None)
            .unwrap_err();
        match err {
            MigrateError::LockConflict(msg) => {
                assert!(msg.contains("42"), "应包含期望版本号");
                assert!(msg.contains("0"), "应包含当前版本号");
            }
            other => panic!("期望 LockConflict，实际: {:?}", other),
        }
        // 状态未改变。
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Pending);
        assert_eq!(m.metadata_version(), 0);
    }

    #[test]
    fn test_update_with_cas_module_not_found() {
        // 模块不存在：返回 Config 错误。
        let mut m = new_machine();
        let err = m
            .update_with_cas("ghost", ModuleStatus::Translating, 0, None, None)
            .unwrap_err();
        assert!(matches!(err, MigrateError::Config(_)));
    }

    #[test]
    fn test_update_with_cas_invalid_transition() {
        // 转换不合法（done → translating）：CAS 通过但转换失败，版本不递增。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Done));
        let err = m
            .update_with_cas("a", ModuleStatus::Translating, 0, None, None)
            .unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
        assert_eq!(m.metadata_version(), 0, "转换失败时版本不应递增");
    }

    #[test]
    fn test_update_with_cas_sequential_increments() {
        // 连续两次 CAS 更新：版本号从 0→1→2。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        // 第一次：0→1
        let (_, v1) = m
            .update_with_cas("a", ModuleStatus::Translating, 0, None, None)
            .unwrap();
        assert_eq!(v1, 1);
        // 第二次：1→2
        let (_, v2) = m
            .update_with_cas("a", ModuleStatus::CompileFixing, 1, None, None)
            .unwrap();
        assert_eq!(v2, 2);
        assert_eq!(m.metadata_version(), 2);
    }

    #[test]
    fn test_update_with_cas_substatus_and_reason() {
        // CAS 更新带 substatus 和 reason。
        let mut m = new_machine();
        m.update_module("a", module_with_status(ModuleStatus::Pending));
        m.update_with_cas(
            "a",
            ModuleStatus::Translating,
            0,
            Some("phase_a_in_progress"),
            Some("开始翻译"),
        )
        .unwrap();
        let module = &m.state_file().modules["a"];
        assert_eq!(module.substatus.as_deref(), Some("phase_a_in_progress"));
        assert_eq!(module.attempts.len(), 1);
        assert!(module.attempts[0].result.contains("开始翻译"));
    }

    // ---- M4-ROB-01c：resume_plan（额度耗尽续跑断点计划）----

    #[test]
    fn test_resume_plan_running_states_go_to_interrupted() {
        // 四种运行态全部归 interrupted（需 recover retry 幂等重入）。
        let mut m = new_machine();
        m.update_module("t", module_with_status(ModuleStatus::Translating));
        m.update_module("c", module_with_status(ModuleStatus::CompileFixing));
        m.update_module("e", module_with_status(ModuleStatus::Testing));
        m.update_module("r", module_with_status(ModuleStatus::Reviewing));

        let plan = m.resume_plan();
        // 字典序排序：c, e, r, t。
        let keys: Vec<&str> = plan.interrupted.iter().map(|i| i.module.as_str()).collect();
        assert_eq!(keys, vec!["c", "e", "r", "t"]);
        assert_eq!(plan.progress().in_progress, 4);
        assert!(plan.awaiting_decision.is_empty());
        assert!(plan.next.is_empty());
        // 单文件模块 member_files 回退到 [module]。
        assert_eq!(plan.interrupted[0].member_files, vec!["c".to_string()]);
    }

    #[test]
    fn test_resume_plan_paused_is_awaiting_decision_not_interrupted() {
        // paused 是决策点：单列 awaiting_decision，**不进** interrupted（续跑不复活）。
        let mut m = new_machine();
        m.update_module("p", module_with_status(ModuleStatus::Paused));
        m.update_module("t", module_with_status(ModuleStatus::Translating));

        let plan = m.resume_plan();
        assert_eq!(plan.awaiting_decision, vec!["p".to_string()]);
        assert_eq!(plan.progress().awaiting_decision, 1);
        let interrupted_keys: Vec<&str> =
            plan.interrupted.iter().map(|i| i.module.as_str()).collect();
        assert_eq!(interrupted_keys, vec!["t"], "paused 不得混入 interrupted");
    }

    #[test]
    fn test_resume_plan_terminal_excluded_but_counted() {
        // done/degrade_* 仅计入 progress，不出现在任何可操作桶（不重跑）。
        let mut m = new_machine();
        m.update_module("d", module_with_status(ModuleStatus::Done));
        m.update_module("f", module_with_status(ModuleStatus::DegradeFfi));
        m.update_module("man", module_with_status(ModuleStatus::DegradeManual));
        m.update_module("s", module_with_status(ModuleStatus::DegradeSkip));

        let plan = m.resume_plan();
        assert_eq!(plan.progress().done, 1);
        assert_eq!(plan.progress().degraded, 3);
        assert_eq!(plan.progress().total, 4, "全终态项目 total 仍对账");
        assert!(plan.interrupted.is_empty());
        assert!(plan.next.is_empty());
        assert!(plan.awaiting_decision.is_empty());
        assert!(plan.blocked.is_empty());
    }

    #[test]
    fn test_resume_plan_pending_and_blocked_buckets() {
        let mut m = new_machine();
        // 逆序插入 p2 先于 p1，验证 next 桶的字典序确定性（HashMap 迭代非确定 + `.sort()`）。
        m.update_module("p2", module_with_status(ModuleStatus::Pending));
        m.update_module("p1", module_with_status(ModuleStatus::Pending));
        m.update_module("b", module_with_status(ModuleStatus::Blocked));

        let plan = m.resume_plan();
        assert_eq!(
            plan.next,
            vec!["p1".to_string(), "p2".to_string()],
            "next 桶应字典序（不受插入序影响）"
        );
        assert_eq!(plan.blocked, vec!["b".to_string()]);
        assert_eq!(plan.progress().pending, 2);
        assert_eq!(plan.progress().blocked, 1);
    }

    #[test]
    fn test_resume_plan_progress_counts_reconcile() {
        // 混合态：progress.total == 各桶之和（done+degraded+in_progress+pending+blocked+awaiting_decision）。
        let mut m = new_machine();
        m.update_module("done", module_with_status(ModuleStatus::Done));
        m.update_module("deg", module_with_status(ModuleStatus::DegradeSkip));
        m.update_module("run", module_with_status(ModuleStatus::CompileFixing));
        m.update_module("pend", module_with_status(ModuleStatus::Pending));
        m.update_module("blk", module_with_status(ModuleStatus::Blocked));
        m.update_module("pau", module_with_status(ModuleStatus::Paused));

        let plan = m.resume_plan();
        let p = plan.progress();
        assert_eq!(p.total, 6);
        assert_eq!(
            p.done + p.degraded + p.in_progress + p.pending + p.blocked + p.awaiting_decision,
            p.total,
            "各桶计数应与 total 对账"
        );
        // 派生等式：progress 计数须与 ResumePlan 列表长度一致（钉死单一真相源，见 MDR-017 审查加固）。
        assert_eq!(p.in_progress, plan.interrupted.len());
        assert_eq!(p.pending, plan.next.len());
        assert_eq!(p.blocked, plan.blocked.len());
        assert_eq!(p.awaiting_decision, plan.awaiting_decision.len());
        assert_eq!(p.done, plan.done);
        assert_eq!(p.degraded, plan.degraded);
    }

    #[test]
    fn test_resume_plan_empty_modules() {
        // 空 modules：全空桶、progress 归零；init 无 sprint 状态 → sprint 为 None。
        let m = new_machine();
        let plan = m.resume_plan();
        assert!(plan.interrupted.is_empty());
        assert!(plan.next.is_empty());
        assert_eq!(plan.progress().total, 0);
        assert!(
            plan.sprint.is_none(),
            "init 无 sprint 状态时 sprint 应为 None"
        );
    }

    #[test]
    fn test_resume_plan_sprint_reflects_current() {
        // 有 sprint 状态时，plan.sprint 映射 sprint.current。
        use crate::types::state::SprintState;
        let mut m = new_machine();
        m.set_sprint(SprintState {
            current: 3,
            history: Vec::new(),
        });
        assert_eq!(m.resume_plan().sprint, Some(3));
    }

    #[test]
    fn test_resume_plan_composite_member_files_scope() {
        // composite 组：interrupted 项 member_files 用组的 member_files（供编排器清产物）。
        let mut m = new_machine();
        let mut group = module_with_status(ModuleStatus::Testing);
        group.member_files = Some(vec!["file:a.ts".to_string(), "file:b.ts".to_string()]);
        m.update_module("file:a.ts", group);

        let plan = m.resume_plan();
        assert_eq!(plan.interrupted.len(), 1);
        assert_eq!(
            plan.interrupted[0].member_files,
            vec!["file:a.ts".to_string(), "file:b.ts".to_string()]
        );
    }
}
