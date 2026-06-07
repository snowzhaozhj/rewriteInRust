//! 配置/状态校验模块。
//!
//! 提供状态文件完整性检查和前置条件验证。

pub mod rules;

use crate::error::{MigrateError, Result};
use crate::types::state::{MigrationStateFile, ProjectState};

/// 校验状态文件完整性。
///
/// 检查项：
/// - version 非空
/// - state_history 非空且末条状态与当前状态一致
/// - 前置条件：各状态要求的数据字段是否存在
pub fn validate_state(state_file: &MigrationStateFile) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();

    // 基本完整性：version 非空
    if state_file.version.is_empty() {
        return Err(MigrateError::SchemaValidation(
            "version 字段为空".to_owned(),
        ));
    }

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

/// 前置条件检查：确保进入特定状态前所需数据已就位。
///
/// - 进入 Plan 之前必须有 project 信息
/// - 进入 Scaffold 之前必须有 graph（metadata.graph_build_completed）
/// - 进入 SprintLoop 之前必须有 sprint 信息
fn check_preconditions(state_file: &MigrationStateFile) -> Result<()> {
    match state_file.state {
        ProjectState::Init => {
            // 初始阶段无前置条件
        }
        ProjectState::Profile => {
            // Profile 阶段需要有 project 基本信息
            if state_file.project.is_none() {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 profile 阶段需要 project 信息".to_owned(),
                });
            }
        }
        ProjectState::Plan => {
            // Plan 阶段需要 project 信息
            if state_file.project.is_none() {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 plan 阶段需要 project 信息".to_owned(),
                });
            }
        }
        ProjectState::Scaffold => {
            // Scaffold 阶段需要 graph 构建完成
            if state_file.project.is_none() {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 scaffold 阶段需要 project 信息".to_owned(),
                });
            }
            let graph_done = state_file
                .metadata
                .as_ref()
                .map(|m| m.graph_build_completed)
                .unwrap_or(false);
            if !graph_done {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 scaffold 阶段需要 graph 构建完成".to_owned(),
                });
            }
        }
        ProjectState::SprintLoop => {
            // SprintLoop 需要 project、graph、sprint
            if state_file.project.is_none() {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 sprint_loop 阶段需要 project 信息".to_owned(),
                });
            }
            let graph_done = state_file
                .metadata
                .as_ref()
                .map(|m| m.graph_build_completed)
                .unwrap_or(false);
            if !graph_done {
                return Err(MigrateError::PreconditionFailed {
                    condition: "进入 sprint_loop 阶段需要 graph 构建完成".to_owned(),
                });
            }
        }
        ProjectState::Graduate => {
            // Graduate 阶段所有模块应为终态（作为警告而非硬性条件）
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::{SourceLang, Timestamp};
    use crate::types::state::{MigrationMetadata, ProjectInfo, StateHistoryEntry};
    use std::collections::HashMap;

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
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Plan,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Plan,
                entered_at: now,
                exited_at: None,
            }],
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
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Scaffold,
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
    fn test_validate_sprint_loop_warnings() {
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::SprintLoop,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::SprintLoop,
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
                graph_build_completed: true,
                graph_build_completed_at: None,
                last_error: None,
                lock_token: None,
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
        let now = Timestamp::new("2024-01-01T00:00:00Z");
        let state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Profile,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Profile,
                entered_at: now,
                exited_at: None,
            }],
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
}
