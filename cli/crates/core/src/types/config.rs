//! 配置类型定义（.rustmigrate.toml）。
//!
//! 参照 `docs/design/06-plugin-structure.md § 11.1`。

use serde::{Deserialize, Serialize};

use super::common::SourceLang;

/// 降级策略枚举。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DegradeStrategy {
    /// 通过 FFI 桥接保留原实现。
    #[default]
    Ffi,
    /// 标记为手动迁移。
    Manual,
    /// 跳过该模块。
    Skip,
}

impl std::fmt::Display for DegradeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ffi => write!(f, "ffi"),
            Self::Manual => write!(f, "manual"),
            Self::Skip => write!(f, "skip"),
        }
    }
}

/// 上下文预算检查模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetCheckMode {
    /// 严格模式：超预算则拒绝。
    #[default]
    Strict,
    /// 警告模式：超预算仅告警。
    Warn,
    /// 忽略模式：不做预算检查。
    Ignore,
}

impl std::fmt::Display for BudgetCheckMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strict => write!(f, "strict"),
            Self::Warn => write!(f, "warn"),
            Self::Ignore => write!(f, "ignore"),
        }
    }
}

/// 顶层配置结构（.rustmigrate.toml 反序列化目标）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MigrateConfig {
    pub project: ProjectConfig,
    pub strategy: StrategyConfig,
    pub testing: TestingConfig,
    pub orchestration: OrchestrationConfig,
    pub context: ContextConfig,
}

/// 项目基础配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub name: String,
    pub source_language: SourceLang,
    pub source_root: String,
    pub rust_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            source_language: SourceLang::TypeScript,
            source_root: "src".to_string(),
            rust_root: "rust-src".to_string(),
            source_commit: None,
            exclude: vec![
                "node_modules".to_string(),
                "dist".to_string(),
                ".git".to_string(),
            ],
        }
    }
}

/// 迁移策略配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StrategyConfig {
    /// 最大重试轮数。
    pub max_retry_rounds: u32,
    /// 是否自动确认翻译意图。
    pub auto_confirm_intent: bool,
    /// 降级策略。
    pub degrade_strategy: DegradeStrategy,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            max_retry_rounds: 3,
            auto_confirm_intent: false,
            degrade_strategy: DegradeStrategy::default(),
        }
    }
}

/// 测试配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestingConfig {
    pub coverage_threshold: u32,
    pub proptest_cases: u32,
    pub fuzz_duration_secs: u32,
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            coverage_threshold: 80,
            proptest_cases: 1000,
            fuzz_duration_secs: 60,
        }
    }
}

/// 编排配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrchestrationConfig {
    pub subagent_timeout_secs: u64,
    pub max_retries_per_step: u32,
    pub lock_timeout_secs: u64,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            subagent_timeout_secs: 300,
            max_retries_per_step: 2,
            lock_timeout_secs: 300,
        }
    }
}

/// 上下文预算配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    /// 单次翻译最大 token 数。
    pub max_tokens_per_translation: u64,
    /// 预算检查模式。
    pub budget_check_mode: BudgetCheckMode,
    /// 是否自动拆分超预算模块。
    pub enable_auto_split: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens_per_translation: 100_000,
            budget_check_mode: BudgetCheckMode::default(),
            enable_auto_split: true,
        }
    }
}
