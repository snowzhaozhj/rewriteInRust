//! 配置/状态校验模块。
//!
//! 提供状态文件完整性检查、前置条件验证、blocked 模块检查与自动解除。

pub mod rules;

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::error::{MigrateError, Result};
use crate::state::{MigrationStateMachine, STATE_SCHEMA_VERSION};
use crate::types::state::{MigrationStateFile, ModuleStatus, ProjectState};

/// 校验状态文件完整性。
///
/// 检查项：
/// - version 非空且 schema 主版本号与当前 CLI 兼容（见 [`check_version_compat`]）
/// - state_history 非空且末条状态与当前状态一致
/// - state_history 相邻状态满足合法转换（Init→Profile→…→Graduate）
/// - 前置条件：各状态要求的数据字段是否存在
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
    pub unresolved: Vec<String>,
    /// 是否就绪可解除（`unresolved` 为空）。
    pub ready: bool,
}

/// 检查所有 blocked 模块的依赖就绪状态。
///
/// 遍历 `modules` 中 `status == Blocked` 的模块，逐个检查其 `blocked_by`
/// 引用的模块是否已进入终态（done/degrade_ffi/degrade_manual/degrade_skip）。
///
/// 返回每个 blocked 模块的检查结果（含已解决/未解决依赖列表）。
pub fn check_blocked_modules(state_file: &MigrationStateFile) -> Vec<BlockedCheckResult> {
    let mut results = Vec::new();

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
            let is_terminal = state_file
                .modules
                .get(dep)
                .map(|m| m.status.is_terminal())
                .unwrap_or(false);
            if is_terminal {
                resolved.push(dep.clone());
            } else {
                unresolved.push(dep.clone());
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

    // 构建 blocked 子图的邻接表：M → [N...]（M blocked_by N，N 也是 blocked）。
    let mut adj: HashMap<&String, Vec<&String>> = HashMap::new();
    for key in &blocked_set {
        let deps: Vec<&String> = state_file.modules[*key]
            .blocked_by
            .as_ref()
            .map(|bs| bs.iter().filter(|b| blocked_set.contains(b)).collect())
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
    use crate::types::state::{MigrationMetadata, ProjectInfo, StateHistoryEntry};
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
        }
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
        // blocked_by 引用不存在的模块 → 视为非终态（安全侧）。
        let mut state = minimal_init_state();
        let mut blocked = module_with_status(ModuleStatus::Blocked);
        blocked.blocked_by = Some(vec!["nonexistent".to_owned()]);
        state.modules.insert("target".to_owned(), blocked);

        let results = check_blocked_modules(&state);
        assert_eq!(results.len(), 1);
        assert!(!results[0].ready);
        assert_eq!(results[0].unresolved, vec!["nonexistent".to_owned()]);
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
