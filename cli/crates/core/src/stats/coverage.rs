//! 迁移进度统计。
//!
//! 从 `MigrationStateFile` 计算模块迁移覆盖率和统计数据。

use crate::types::state::{MigrationStateFile, ModuleStatus};

/// 迁移进度统计。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MigrationStats {
    /// 模块总数。
    pub total_modules: usize,
    /// 已完成模块数（状态为 Done）。
    pub completed_modules: usize,
    /// 进行中模块数（Translating / CompileFixing / Testing / Reviewing）。
    pub in_progress_modules: usize,
    /// 待处理模块数（Pending / Paused / Blocked）。
    pub pending_modules: usize,
    /// 降级模块数（DegradeFfi / DegradeManual / DegradeSkip）。
    pub degraded_modules: usize,
    /// 完成百分比（0.0 ~ 100.0）。
    pub completion_percentage: f64,
}

/// 从迁移状态文件计算统计数据。
///
/// 按 `ModuleStatus` 分类统计各模块数量，并计算完成百分比。
/// 完成百分比 = (completed + degraded) / total * 100。
pub fn compute_stats(state: &MigrationStateFile) -> MigrationStats {
    let total_modules = state.modules.len();

    let mut completed_modules: usize = 0;
    let mut in_progress_modules: usize = 0;
    let mut pending_modules: usize = 0;
    let mut degraded_modules: usize = 0;

    for module in state.modules.values() {
        match module.status {
            ModuleStatus::Done => completed_modules += 1,
            ModuleStatus::Translating
            | ModuleStatus::CompileFixing
            | ModuleStatus::Testing
            | ModuleStatus::Reviewing => in_progress_modules += 1,
            ModuleStatus::Pending | ModuleStatus::Paused | ModuleStatus::Blocked => {
                pending_modules += 1;
            }
            ModuleStatus::DegradeFfi | ModuleStatus::DegradeManual | ModuleStatus::DegradeSkip => {
                degraded_modules += 1
            }
        }
    }

    let completion_percentage = if total_modules == 0 {
        0.0
    } else {
        (completed_modules + degraded_modules) as f64 / total_modules as f64 * 100.0
    };

    MigrationStats {
        total_modules,
        completed_modules,
        in_progress_modules,
        pending_modules,
        degraded_modules,
        completion_percentage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::state::{ModuleState, ProjectState};
    use std::collections::HashMap;

    /// 创建默认 ModuleState（指定 status）。
    fn module(status: ModuleStatus) -> ModuleState {
        ModuleState {
            status,
            substatus: None,
            sprint: None,
            attempts: vec![],
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

    /// 创建空的 MigrationStateFile。
    fn empty_state() -> MigrationStateFile {
        MigrationStateFile {
            version: "1.0.0".to_string(),
            state: ProjectState::SprintLoop,
            state_history: vec![],
            project: None,
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: vec![],
            metadata: None,
        }
    }

    #[test]
    fn test_compute_stats_empty() {
        let state = empty_state();
        let stats = compute_stats(&state);

        assert_eq!(stats.total_modules, 0);
        assert_eq!(stats.completed_modules, 0);
        assert_eq!(stats.in_progress_modules, 0);
        assert_eq!(stats.pending_modules, 0);
        assert_eq!(stats.degraded_modules, 0);
        assert!((stats.completion_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_all_pending() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::Pending));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::Pending));
        state
            .modules
            .insert("c".to_string(), module(ModuleStatus::Blocked));

        let stats = compute_stats(&state);
        assert_eq!(stats.total_modules, 3);
        assert_eq!(stats.pending_modules, 3);
        assert_eq!(stats.completed_modules, 0);
        assert!((stats.completion_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_all_done() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::Done));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::Done));

        let stats = compute_stats(&state);
        assert_eq!(stats.total_modules, 2);
        assert_eq!(stats.completed_modules, 2);
        assert!((stats.completion_percentage - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_mixed() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::Done));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::Translating));
        state
            .modules
            .insert("c".to_string(), module(ModuleStatus::Pending));
        state
            .modules
            .insert("d".to_string(), module(ModuleStatus::DegradeFfi));

        let stats = compute_stats(&state);
        assert_eq!(stats.total_modules, 4);
        assert_eq!(stats.completed_modules, 1);
        assert_eq!(stats.in_progress_modules, 1);
        assert_eq!(stats.pending_modules, 1);
        assert_eq!(stats.degraded_modules, 1);
        // (1 done + 1 degraded) / 4 = 50%
        assert!((stats.completion_percentage - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_in_progress_variants() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::Translating));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::CompileFixing));
        state
            .modules
            .insert("c".to_string(), module(ModuleStatus::Testing));
        state
            .modules
            .insert("d".to_string(), module(ModuleStatus::Reviewing));

        let stats = compute_stats(&state);
        assert_eq!(stats.in_progress_modules, 4);
        assert_eq!(stats.completed_modules, 0);
        assert!((stats.completion_percentage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_degraded_variants() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::DegradeFfi));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::DegradeManual));
        state
            .modules
            .insert("c".to_string(), module(ModuleStatus::DegradeSkip));

        let stats = compute_stats(&state);
        assert_eq!(stats.degraded_modules, 3);
        // 降级也算完成
        assert!((stats.completion_percentage - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_stats_paused_counted_as_pending() {
        let mut state = empty_state();
        state
            .modules
            .insert("a".to_string(), module(ModuleStatus::Paused));
        state
            .modules
            .insert("b".to_string(), module(ModuleStatus::Done));

        let stats = compute_stats(&state);
        assert_eq!(stats.pending_modules, 1);
        assert_eq!(stats.completed_modules, 1);
        assert!((stats.completion_percentage - 50.0).abs() < f64::EPSILON);
    }
}
