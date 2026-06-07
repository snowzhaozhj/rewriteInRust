//! 迁移状态机类型定义。
//!
//! 参照 `docs/design/02-architecture.md § 3.4` 和
//! `docs/design/09-appendix-schemas.md § 附录 A`。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::common::{RiskLevel, SourceLang, Timestamp};

/// 项目级状态机节点（编排器状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectState {
    /// 初始化阶段。
    Init,
    /// 项目画像分析。
    Profile,
    /// 迁移计划生成。
    Plan,
    /// Rust 工程脚手架。
    Scaffold,
    /// Sprint 循环迁移。
    SprintLoop,
    /// 毕业（迁移完成）。
    Graduate,
}

impl ProjectState {
    /// 检查是否允许从当前状态转换到目标状态。
    ///
    /// 合法转换路径：Init → Profile → Plan → Scaffold → SprintLoop → Graduate。
    pub fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Init, Self::Profile)
                | (Self::Profile, Self::Plan)
                | (Self::Plan, Self::Scaffold)
                | (Self::Scaffold, Self::SprintLoop)
                | (Self::SprintLoop, Self::Graduate)
        )
    }
}

impl std::fmt::Display for ProjectState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Init => write!(f, "init"),
            Self::Profile => write!(f, "profile"),
            Self::Plan => write!(f, "plan"),
            Self::Scaffold => write!(f, "scaffold"),
            Self::SprintLoop => write!(f, "sprint_loop"),
            Self::Graduate => write!(f, "graduate"),
        }
    }
}

/// 模块级状态（模块迁移生命周期）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleStatus {
    Pending,
    Translating,
    CompileFixing,
    Testing,
    Reviewing,
    Done,
    DegradeFfi,
    DegradeManual,
    DegradeSkip,
    Paused,
    Blocked,
}

impl ModuleStatus {
    /// 是否为终态（done 或 degrade_*）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::DegradeFfi | Self::DegradeManual | Self::DegradeSkip
        )
    }

    /// 是否为降级状态。
    pub fn is_degraded(self) -> bool {
        matches!(
            self,
            Self::DegradeFfi | Self::DegradeManual | Self::DegradeSkip
        )
    }
}

impl std::fmt::Display for ModuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Translating => write!(f, "translating"),
            Self::CompileFixing => write!(f, "compile_fixing"),
            Self::Testing => write!(f, "testing"),
            Self::Reviewing => write!(f, "reviewing"),
            Self::Done => write!(f, "done"),
            Self::DegradeFfi => write!(f, "degrade_ffi"),
            Self::DegradeManual => write!(f, "degrade_manual"),
            Self::DegradeSkip => write!(f, "degrade_skip"),
            Self::Paused => write!(f, "paused"),
            Self::Blocked => write!(f, "blocked"),
        }
    }
}

/// 翻译阶段（Phase A 忠实翻译 / Phase B 惯用化优化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TranslationPhase {
    A,
    B,
}

/// 状态历史条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub state: ProjectState,
    pub entered_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<Timestamp>,
}

/// 项目基本信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub source_language: SourceLang,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    #[serde(default)]
    pub source_loc: u64,
    pub created_at: Timestamp,
}

/// Sprint 历史条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprintEntry {
    pub id: u32,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(default)]
    pub target_modules: Vec<String>,
    #[serde(default)]
    pub completed_modules: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Sprint 状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprintState {
    pub current: u32,
    #[serde(default)]
    pub history: Vec<SprintEntry>,
}

/// 模块迁移尝试记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub timestamp: Timestamp,
    pub result: String,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

/// 单个模块的迁移状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleState {
    pub status: ModuleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substatus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<u32>,
    #[serde(default)]
    pub attempts: Vec<AttemptRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_pass_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<u32>,
    #[serde(default)]
    pub known_differences: u32,
    #[serde(default = "default_risk")]
    pub risk: RiskLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_a_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_a_audit_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_blocked_status: Option<ModuleStatus>,
}

fn default_risk() -> RiskLevel {
    RiskLevel::Low
}

/// SubAgent 调用记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubAgentCall {
    pub step_index: u32,
    pub subagent_name: String,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Timestamp>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// 迁移元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationMetadata {
    #[serde(default)]
    pub graph_build_completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_build_completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_token: Option<String>,
}

/// 迁移状态文件 (migration-state.json) 的完整结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationStateFile {
    pub version: String,
    pub state: ProjectState,
    #[serde(default)]
    pub state_history: Vec<StateHistoryEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<SprintState>,
    #[serde(default)]
    pub modules: HashMap<String, ModuleState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_ref: Option<String>,
    #[serde(default)]
    pub subagent_calls: Vec<SubAgentCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MigrationMetadata>,
}
