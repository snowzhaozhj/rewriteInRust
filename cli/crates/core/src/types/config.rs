/// 配置类型定义（.rustmigrate.toml）。
///
/// 参照 `docs/design/06-plugin-structure.md § 11.1`。
use serde::{Deserialize, Serialize};

use super::common::SourceLang;

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
    pub max_retry_rounds: u32,
    pub auto_confirm_intent: bool,
    pub degrade_strategy: String,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            max_retry_rounds: 3,
            auto_confirm_intent: false,
            degrade_strategy: "ffi".to_string(),
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
    pub max_tokens_per_translation: u64,
    pub budget_check_mode: String,
    pub enable_auto_split: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens_per_translation: 100_000,
            budget_check_mode: "strict".to_string(),
            enable_auto_split: true,
        }
    }
}
