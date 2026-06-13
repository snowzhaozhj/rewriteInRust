//! 源码图类型定义：节点、边、图结构。
//!
//! 参照 `docs/design/04-toolchain.md § 5.7.1` 和
//! `docs/design/09-appendix-schemas.md § 附录 D`。

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::common::{Complexity, MigrationPriority, NodeId, Span, Timestamp};

/// 图节点类型（12 种：MVP 9 + M2 预留 3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum NodeType {
    /// 源文件。
    File,
    /// 逻辑模块（TS namespace / Python package）。
    Module,
    /// 顶层包。
    Package,
    /// 函数/方法（通过 `contains` 边区分：被 Class 包含 = method）。
    Function,
    /// 类/结构体。
    Class,
    /// 接口/trait。
    Interface,
    /// 枚举。
    #[strum(serialize = "enum")]
    Enum,
    /// Rust 目标节点（迁移映射的目标端）。
    RustTarget,
    /// 测试夹具节点（TestedBy 边的目标端）。
    TestFixture,
    /// 类型别名（M2 扩展，预留）。
    TypeAlias,
    /// 模块级常量/变量（M2 扩展，预留）。
    Variable,
    /// 功能聚类（M2 扩展，Leiden 算法产出）。
    Community,
}

/// 图边类型（MVP 8 种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EdgeType {
    /// 父子包含（Class → Function）。
    Contains,
    /// 导入依赖（File → File）。
    Imports,
    /// 函数调用（File → Function/Class）。
    Calls,
    /// 继承/实现（Class → Interface/Class）。
    Extends,
    /// 类型引用（Function → Class/Interface/Enum）。
    UsesType,
    /// 对外导出（File → Function/Class/Interface/Enum）。
    Exports,
    /// 迁移映射（源节点 → RustTarget）。
    MapsTo,
    /// 测试覆盖（Function → TestFixture）。
    TestedBy,
}

/// 边的来源（谁产出了这条边）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Provenance {
    /// tree-sitter AST 确定性解析。
    TreeSitter,
    /// 确定性辅助工具（ast-grep / dependency-cruiser 等）。
    ToolAssisted,
    /// LLM 推断（需人工确认）。
    Llm,
    /// 用户手动标注。
    Manual,
}

/// 节点可见性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Private,
}

/// 源码图节点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceNode {
    pub id: NodeId,
    pub node_type: NodeType,
    pub name: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_range: Option<Span>,
    #[serde(default)]
    pub is_exported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity: Option<Complexity>,
    #[serde(default)]
    pub is_async: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(default)]
    pub is_abstract: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_priority: Option<MigrationPriority>,
    /// RustTarget 节点的 Rust 类型种类（Struct/Enum/Trait/Function/Module/Crate）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_kind: Option<String>,
    /// RustTarget 节点的 Rust 模块路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_path: Option<String>,
    /// RustTarget 节点所属的 crate 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
}

/// 源码图边（依赖关系）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub source: NodeId,
    pub target: NodeId,
    pub edge_type: EdgeType,
    #[serde(default = "default_provenance")]
    pub provenance: Provenance,
    #[serde(default = "default_weight")]
    pub weight: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_notes: Option<String>,
}

fn default_provenance() -> Provenance {
    Provenance::TreeSitter
}

fn default_weight() -> f64 {
    1.0
}

/// 文件指纹（用于增量图更新检测）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub content_hash: String,
    pub structure_hash: String,
    pub analyzed_at: Timestamp,
}

/// 源码图导出格式（JSON）。
///
/// 主存储使用 SQLite，此结构用于 `graph export --format json` 导出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceGraphExport {
    pub version: String,
    pub generated_at: Timestamp,
    pub storage: String,
    pub db_path: String,
    pub nodes: Vec<SourceNode>,
    pub edges: Vec<Dependency>,
    #[serde(default)]
    pub topological_order: Vec<String>,
    #[serde(default)]
    pub file_fingerprints: std::collections::HashMap<String, FileFingerprint>,
}
