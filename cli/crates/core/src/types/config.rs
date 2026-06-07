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
    Strict,
    /// 警告模式：超预算仅告警。
    #[default]
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

/// 异步策略枚举。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsyncStrategy {
    /// 边界异步（仅在模块边界使用 async）。
    #[default]
    BoundaryAsync,
    /// 全量异步。
    FullAsync,
    /// 同步优先。
    SyncFirst,
}

impl std::fmt::Display for AsyncStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BoundaryAsync => write!(f, "boundary_async"),
            Self::FullAsync => write!(f, "full_async"),
            Self::SyncFirst => write!(f, "sync_first"),
        }
    }
}

/// FFI 覆盖率模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FfiCoverageMode {
    /// 仅统计 Rust 侧覆盖率。
    #[default]
    RustOnly,
    /// 包含 FFI 桥接的覆盖率。
    IncludeFfi,
    /// 全量覆盖（含原语言侧）。
    Full,
}

impl std::fmt::Display for FfiCoverageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RustOnly => write!(f, "rust_only"),
            Self::IncludeFfi => write!(f, "include_ffi"),
            Self::Full => write!(f, "full"),
        }
    }
}

/// 顶层配置结构（.rustmigrate.toml 反序列化目标）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MigrateConfig {
    /// 项目基础配置。
    pub project: ProjectConfig,
    /// 迁移策略配置。
    pub strategy: StrategyConfig,
    /// 测试配置。
    pub testing: TestingConfig,
    /// 编排配置。
    pub orchestration: OrchestrationConfig,
    /// 上下文预算配置。
    pub context: ContextConfig,
    /// 工具链配置（M2 预留）。
    #[serde(default)]
    pub tools: ToolsConfig,
    /// 质量门配置（M2 预留）。
    #[serde(default)]
    pub quality: QualityConfig,
    /// 解析器配置（M2 预留）。
    #[serde(default)]
    pub parser: ParserConfig,
    /// 分析配置（M2 预留）。
    #[serde(default)]
    pub analysis: AnalysisConfig,
    /// 可复现性配置（M2 预留）。
    #[serde(default)]
    pub reproducibility: ReproducibilityConfig,
    /// 工作空间配置（M2 预留）。
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    /// 持久化配置（M2 预留）。
    #[serde(default)]
    pub persistence: PersistenceConfig,
    /// 校验配置（M2 预留）。
    #[serde(default)]
    pub validation: ValidationConfig,
    /// 规则配置（M2 预留）。
    #[serde(default)]
    pub rules: RulesConfig,
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
    /// 最大并发 Agent 数。
    pub max_concurrent_agents: u32,
    /// 异步策略。
    pub async_strategy: AsyncStrategy,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            max_retry_rounds: 3,
            auto_confirm_intent: false,
            degrade_strategy: DegradeStrategy::default(),
            max_concurrent_agents: 4,
            async_strategy: AsyncStrategy::default(),
        }
    }
}

/// 测试配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestingConfig {
    /// 覆盖率阈值（百分比）。
    pub coverage_threshold: u32,
    /// proptest 测试用例数。
    pub proptest_cases: u32,
    /// fuzz 测试持续秒数。
    pub fuzz_duration_secs: u32,
    /// 基准测试容差（比例）。
    pub benchmark_tolerance: f64,
    /// nextest 并发线程策略。
    pub nextest_threads: String,
    /// FFI 覆盖率统计模式。
    pub ffi_coverage_mode: FfiCoverageMode,
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            coverage_threshold: 80,
            proptest_cases: 256,
            fuzz_duration_secs: 60,
            benchmark_tolerance: 0.1,
            nextest_threads: "auto".to_string(),
            ffi_coverage_mode: FfiCoverageMode::default(),
        }
    }
}

/// 编排配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrchestrationConfig {
    /// SubAgent 超时秒数。
    pub subagent_timeout_secs: u64,
    /// 每步最大重试次数。
    pub max_retries_per_step: u32,
    /// 锁超时秒数。
    pub lock_timeout_secs: u64,
    /// 重试退避间隔序列（秒）。
    pub retry_backoff_secs: Vec<u64>,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            subagent_timeout_secs: 600,
            max_retries_per_step: 2,
            lock_timeout_secs: 300,
            retry_backoff_secs: vec![5, 15],
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
    /// 模块摘要策略。
    pub module_summary_strategy: String,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens_per_translation: 100_000,
            budget_check_mode: BudgetCheckMode::default(),
            enable_auto_split: true,
            module_summary_strategy: "interface_only".to_string(),
        }
    }
}

// ─── M2 预留配置段（空结构体占位） ───────────────────────────────────

/// 工具链配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {}

/// 质量门配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityConfig {}

/// 解析器配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParserConfig {}

/// 分析配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalysisConfig {}

/// 可复现性配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReproducibilityConfig {}

/// 工作空间配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {}

/// 持久化配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistenceConfig {}

/// 校验配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationConfig {}

/// 规则配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {}
