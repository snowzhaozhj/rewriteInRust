//! 状态机实现。
//!
//! 管理 `migration-state.json` 的生命周期：创建、加载、保存、状态转换。

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::types::common::{SourceLang, Timestamp};
use crate::types::state::{
    MigrationMetadata, MigrationStateFile, ModuleState, ModuleStatus, ProjectInfo, ProjectState,
    StateHistoryEntry,
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
    /// 如果文件不存在返回 `MigrateError::FileNotFound`。
    ///
    /// **仅当主文件 JSON 解析失败（数据损坏）时**才回退到 `.backup`——这是应对
    /// 崩溃/并发写入残留半截文件的兜底。I/O / 权限等非损坏错误**直接上抛**，
    /// 不被回退掩盖（否则临时 I/O 故障会让调用方静默读到过期状态）。
    /// 主备双双损坏时，返回**主文件**的错误（primary），保留真正的故障现场。
    ///
    /// 注意：回退到 backup 意味着拿到的是**上一次成功保存前**的旧状态，最近一次
    /// 保存的进度可能丢失。load 本身不自愈主文件（损坏文件残留，依赖下次 save 覆盖）。
    /// TODO(M1-INTEG)：CLI 接线时，应把"已从 backup 恢复、最新进度可能丢失"作为
    /// warning 经统一响应（`Response::ok_with_warnings`）上报给用户。
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
                Self::load_file(&backup).map_err(|_| primary)
            }
            Err(other) => Err(other),
        }
    }

    /// 从指定路径读取并反序列化状态文件。
    fn load_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state_file: MigrationStateFile = serde_json::from_str(&content)?;
        Ok(Self { state_file })
    }

    /// 保存状态到文件（crash-safe）。
    ///
    /// 自动创建父目录；采用 tmp → fsync → 原子 rename，并同步父目录，
    /// 保证进程崩溃或并发写入中断时不会留下半截 JSON。覆盖前先备份 `.backup`，
    /// 供 [`load`](Self::load) 在主文件损坏时回退。
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(&self.state_file)?;
        atomic_write(path, content.as_bytes())
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

    /// 标记 graph 构建完成。
    pub fn set_graph_build_completed(&mut self) {
        let now = Timestamp::new(chrono::Utc::now().to_rfc3339());
        let metadata = self.state_file.metadata.get_or_insert(MigrationMetadata {
            graph_build_completed: false,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
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

    /// 执行模块级状态转换（带合法性校验）。
    ///
    /// 校验 [`ModuleStatus::can_transition_to`]（依据 `docs/design/09-appendix-schemas.md`
    /// 模块状态转换图），非法转换返回 `MigrateError::InvalidTransition`；
    /// 模块不存在返回 `MigrateError::Config`。仅更新 `status`，保留其余字段。
    pub fn transition_module(&mut self, name: &str, to: ModuleStatus) -> Result<()> {
        let module = self
            .state_file
            .modules
            .get_mut(name)
            .ok_or_else(|| MigrateError::Config(format!("模块不存在: {name}")))?;
        if !module.status.can_transition_to(to) {
            return Err(MigrateError::InvalidTransition {
                from: module.status.to_string(),
                to: to.to_string(),
            });
        }
        module.status = to;
        Ok(())
    }

    /// 设置 sprint 信息。
    pub fn set_sprint(&mut self, sprint: crate::types::state::SprintState) {
        self.state_file.sprint = Some(sprint);
    }

    /// 设置最后错误信息。
    pub fn set_last_error(&mut self, error: Option<String>) {
        let metadata = self.state_file.metadata.get_or_insert(MigrationMetadata {
            graph_build_completed: false,
            graph_build_completed_at: None,
            last_error: None,
            lock_token: None,
        });
        metadata.last_error = error;
    }
}

/// 原子写入：覆盖前备份 `.backup`，写入 `.tmp` 并 fsync，再 rename 到目标，最后同步父目录。
///
/// 保证崩溃/并发中断时目标文件要么是旧内容要么是完整新内容，绝不出现半截 JSON。
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::RiskLevel;
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
            risk: RiskLevel::Low,
            phase_a_version: None,
            phase_a_audit_passed: None,
            blocked_by: None,
            pre_blocked_status: None,
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
        assert!(m.transition_module("a", ModuleStatus::Translating).is_ok());
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
        let err = m.transition_module("a", ModuleStatus::Pending).unwrap_err();
        assert!(matches!(err, MigrateError::InvalidTransition { .. }));
        assert_eq!(m.state_file().modules["a"].status, ModuleStatus::Done);
    }

    #[test]
    fn test_transition_module_missing() {
        let mut m = new_machine();
        let err = m
            .transition_module("ghost", ModuleStatus::Translating)
            .unwrap_err();
        assert!(matches!(err, MigrateError::Config(_)));
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
                m.transition_module("a", ModuleStatus::Translating).is_ok(),
                "{st} 应允许恢复到 translating"
            );
        }
    }
}
