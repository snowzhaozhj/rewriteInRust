//! 状态机实现。
//!
//! 管理 `migration-state.json` 的生命周期：创建、加载、保存、状态转换。

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::types::common::{SourceLang, Timestamp};
use crate::types::state::{
    AttemptRecord, MigrationMetadata, MigrationStateFile, ModuleState, ModuleStatus, ProjectInfo,
    ProjectState, StateHistoryEntry,
};

/// 状态文件 schema 版本号。
const STATE_SCHEMA_VERSION: &str = "1.0.0";

/// 迁移状态机，持有并管理 `MigrationStateFile`。
#[derive(Debug, Clone)]
pub struct MigrationStateMachine {
    /// 内部状态文件数据。
    state_file: MigrationStateFile,
    /// 运行时标志（不序列化）：本次 [`load`](Self::load) 是否因主文件损坏而回退到 `.backup`。
    /// 为真表示拿到的是上一次成功保存前的旧状态，最近进度可能丢失，调用方应向用户告警。
    recovered_from_backup: bool,
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
        Ok(Self {
            state_file,
            recovered_from_backup: false,
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
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(&self.state_file)?;
        atomic_write(path, content.as_bytes(), !self.recovered_from_backup)
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

        Self {
            state_file,
            recovered_from_backup: false,
        }
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
    /// 模块不存在返回 `MigrateError::Config`。
    pub fn transition_module(
        &mut self,
        name: &str,
        to: Option<ModuleStatus>,
        substatus: Option<&str>,
        reason: Option<&str>,
        force: bool,
    ) -> Result<()> {
        let module = self
            .state_file
            .modules
            .get_mut(name)
            .ok_or_else(|| MigrateError::Config(format!("模块不存在: {name}")))?;
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
}
