//! 配置类型定义（.rustmigrate.toml）。
//!
//! 参照 `docs/design/06-plugin-structure.md § 11.1`。

use serde::{Deserialize, Serialize};
use strum::Display;

use super::common::SourceLang;

/// 降级策略枚举。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum DegradeStrategy {
    /// 通过 FFI 桥接保留原实现。
    #[default]
    Ffi,
    /// 标记为手动迁移。
    Manual,
    /// 跳过该模块。
    Skip,
}

/// 上下文预算检查模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum BudgetCheckMode {
    /// 严格模式：超预算则拒绝。
    Strict,
    /// 警告模式：超预算仅告警。
    #[default]
    Warn,
    /// 忽略模式：不做预算检查。
    Ignore,
}

/// 异步策略枚举。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AsyncStrategy {
    /// 边界异步（仅在模块边界使用 async）。
    #[default]
    BoundaryAsync,
    /// 全量异步。
    FullAsync,
    /// 同步优先。
    SyncFirst,
}

/// FFI 覆盖率模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum FfiCoverageMode {
    /// 仅统计 Rust 侧覆盖率。
    #[default]
    RustOnly,
    /// 包含 FFI 桥接的覆盖率。
    IncludeFfi,
    /// 全量覆盖（含原语言侧）。
    Full,
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
    pub source_language: Option<SourceLang>,
    pub source_root: String,
    pub rust_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    /// 用户自定义排除目录（追加在语言默认排除之上）。
    ///
    /// TODO(M3-PLG): 当前遍历（graph/profile/stats）统一用语言无关的
    /// [`crate::types::common::EXCLUDED_DIRS`] 全集，**尚未读取本字段**。
    /// Sprint C（用户配置接入）将本字段贯穿遍历链路（build_graph 等接收 exclude 参数）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// 语言适配器目录（含 `analysis-tools.json` 和 `porting-template.md`）。
    /// 省略时由 SKILL.md analyze 流程按优先级自动定位。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_path: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            source_language: None,
            source_root: "src".to_string(),
            rust_root: "rust-src".to_string(),
            source_commit: None,
            exclude: Vec::new(),
            adapter_path: None,
        }
    }
}

/// 任何项目都应排除的非源码目录（VCS、Rust 构建产物）。
pub const COMMON_EXCLUDES: &[&str] = &[".git", "target"];

/// 某语言专属的依赖 / 构建产物目录（不含 [`COMMON_EXCLUDES`]）。
///
/// 排除目录的**唯一权威**：[`crate::types::common::EXCLUDED_DIRS`]（语言未知时的全集）
/// 由本函数各语言并集派生，一致性由 `excluded_dirs_is_union_of_all_langs` 测试保证。
pub fn lang_vendor_dirs(lang: SourceLang) -> &'static [&'static str] {
    match lang {
        SourceLang::TypeScript => &["node_modules", "dist"],
        SourceLang::Python => &["__pycache__", ".venv", "venv", ".mypy_cache"],
        SourceLang::C => &["build", ".obj"],
        SourceLang::Go => &["vendor"],
    }
}

/// 某语言的完整默认排除 = [`COMMON_EXCLUDES`] + 语言专属 vendor 目录。
pub fn default_excludes_for_lang(lang: SourceLang) -> Vec<String> {
    COMMON_EXCLUDES
        .iter()
        .chain(lang_vendor_dirs(lang))
        .map(|s| s.to_string())
        .collect()
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

/// 质量门配置（对齐 03 §7.5 阈值 + 06 §11.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityConfig {
    pub done_threshold: f64,
    pub degrade_ffi_threshold: f64,
    pub evaluation_method_version: String,
    pub baseline_sprint: u32,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            done_threshold: 80.0,
            degrade_ffi_threshold: 60.0,
            evaluation_method_version: "0.1".to_string(),
            baseline_sprint: 1,
        }
    }
}

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

/// 持久化配置（state 持久化与崩溃恢复）。
///
/// 对齐 `docs/design/06-plugin-structure.md § [persistence]`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistenceConfig {
    /// 每次写 migration-state.json 前是否创建 .backup（默认 true，与既有行为一致）。
    pub backup_on_write: bool,
    /// 备份保留天数（None = 不过期，不清理旧备份）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            backup_on_write: true,
            retention_days: None,
        }
    }
}

/// 校验配置（M2 预留）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationConfig {}

/// 规则治理配置（`06 § 11.1` `[rules]` 段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// 规则版本一致性强制开关（M4-GOV-01）：`true` 时 `validate rules` 检出各适配器
    /// `porting-template.md` 的 `rule_version` 与权威清单不一致即返回**错误**（非静默阻断）；
    /// `false` 时降级为 warning。默认 `true`（对齐 `06 § 11.1` 缺省）。
    pub enforce_rule_version_consistency: bool,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self {
            enforce_rule_version_consistency: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::EXCLUDED_DIRS;
    use std::collections::BTreeSet;

    /// 防漂移：语言无关全集 `EXCLUDED_DIRS` 必须等于各语言默认排除的并集，
    /// 保证排除目录单一来源（新增语言/目录时两处同步）。
    #[test]
    fn excluded_dirs_is_union_of_all_langs() {
        let union: BTreeSet<String> = [
            SourceLang::TypeScript,
            SourceLang::Python,
            SourceLang::C,
            SourceLang::Go,
        ]
        .iter()
        .flat_map(|l| default_excludes_for_lang(*l))
        .collect();
        let constant: BTreeSet<String> = EXCLUDED_DIRS.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            union, constant,
            "EXCLUDED_DIRS 必须等于各语言 default_excludes_for_lang 的并集"
        );
    }

    /// 语言默认排除含通用目录 + 该语言 vendor 目录。
    #[test]
    fn default_excludes_compose_common_and_vendor() {
        let py = default_excludes_for_lang(SourceLang::Python);
        assert!(py.contains(&".git".to_string()), "应含通用 .git");
        assert!(py.contains(&".venv".to_string()), "应含 Python .venv");
        assert!(
            !py.contains(&"node_modules".to_string()),
            "Python 不应含 TS 的 node_modules"
        );
    }
}
