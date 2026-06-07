//! 状态机实现。
//!
//! 管理 `migration-state.json` 的生命周期：创建、加载、保存、状态转换。

use std::path::Path;

use crate::error::{MigrateError, Result};
use crate::types::common::{SourceLang, Timestamp};
use crate::types::state::{
    MigrationMetadata, MigrationStateFile, ProjectInfo, ProjectState, StateHistoryEntry,
};

/// 状态文件 schema 版本号。
const STATE_SCHEMA_VERSION: &str = "1.0.0";

/// 迁移状态机，持有并管理 `MigrationStateFile`。
#[derive(Debug, Clone)]
pub struct MigrationStateMachine {
    /// 内部状态文件数据。
    state_file: MigrationStateFile,
}

impl MigrationStateMachine {
    /// 从文件加载状态。
    ///
    /// 如果文件不存在返回 `MigrateError::FileNotFound`，
    /// JSON 格式错误返回 `MigrateError::Json`。
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(MigrateError::FileNotFound(path.to_path_buf()));
        }
        let content = std::fs::read_to_string(path)?;
        let state_file: MigrationStateFile = serde_json::from_str(&content)?;
        Ok(Self { state_file })
    }

    /// 保存状态到文件。
    ///
    /// 自动创建父目录（如果不存在）。
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(&self.state_file)?;
        std::fs::write(path, content)?;
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
            }),
        };

        Self { state_file }
    }

    /// 返回当前项目状态。
    pub fn current_state(&self) -> ProjectState {
        self.state_file.state
    }

    /// 获取内部状态文件的不可变引用。
    pub fn state_file(&self) -> &MigrationStateFile {
        &self.state_file
    }

    /// 获取内部状态文件的可变引用。
    pub fn state_file_mut(&mut self) -> &mut MigrationStateFile {
        &mut self.state_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::NamedTempFile;

    /// 辅助：创建一个初始状态机。
    fn new_machine() -> MigrationStateMachine {
        MigrationStateMachine::init_new("test-project", SourceLang::TypeScript)
    }

    #[test]
    fn test_init_new_设置正确初始状态() {
        let m = new_machine();
        assert_eq!(m.current_state(), ProjectState::Init);
        assert_eq!(m.state_file().version, "1.0.0");
        assert_eq!(m.state_file().state_history.len(), 1);
        let project = m.state_file().project.as_ref().expect("应有 project");
        assert_eq!(project.name, "test-project");
        assert_eq!(project.source_language, SourceLang::TypeScript);
    }

    #[test]
    fn test_合法转换_init_到_profile() {
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
    fn test_全链路转换() {
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
    fn test_非法转换_init_到_plan() {
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
    fn test_非法转换_跳过阶段() {
        let mut m = new_machine();
        assert!(m.transition(ProjectState::Scaffold).is_err());
        assert!(m.transition(ProjectState::SprintLoop).is_err());
        assert!(m.transition(ProjectState::Graduate).is_err());
    }

    #[test]
    fn test_非法转换_回退() {
        let mut m = new_machine();
        m.transition(ProjectState::Profile).unwrap();
        assert!(m.transition(ProjectState::Init).is_err());
    }

    #[test]
    fn test_保存和加载() {
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
    fn test_保存后转换再加载() {
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
    fn test_加载不存在的文件() {
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
    fn test_加载非法json() {
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
    fn test_save创建父目录() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let nested = dir.path().join("sub").join("dir").join("state.json");
        let m = new_machine();
        assert!(m.save(&nested).is_ok());
        assert!(nested.exists());
    }
}
