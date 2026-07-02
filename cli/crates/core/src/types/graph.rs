//! 源码图类型定义：节点、边、图结构。
//!
//! 参照 `docs/design/04-toolchain.md § 5.7.1` 和
//! `docs/design/09-appendix-schemas.md § 附录 D`。

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::common::{Complexity, MigrationPriority, NodeId, Span, Timestamp};
use super::state::ModuleStatus;

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
    /// 对外导出（File → Function/Class/Interface/Enum/Variable/TypeAlias）。
    /// Variable/TypeAlias 目标由 Go adapter 产出（M4-GO-04：Go 导出=首字母大写，
    /// 包级 const/var 与 type 别名同样可导出）。
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

/// RustTarget 节点的 Rust 类型种类（迁移映射目标端的实体类别）。
///
/// 对应 `docs/design/04-toolchain.md § 5.7.1` 的 `rust_kind: Option<RustKind>`。
/// 序列化保持 PascalCase（与既有落库数据 `"Function"` 等一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
pub enum RustKind {
    Struct,
    Enum,
    Trait,
    Function,
    Module,
    Crate,
}

/// 边的子类型（在同一 `EdgeType` 下进一步区分语义）。
///
/// 对应 `docs/design/04-toolchain.md § 5.7.1` 的 `sub_kind`：
/// - `Extends` 边用 `Implements` 区分「实现接口」与「继承」（继承为 `None`）。
/// - `Calls` 边用 `Constructor` 标注构造调用（`new Foo()`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EdgeSubKind {
    /// `Extends` 边：Class 实现 Interface（区别于继承）。
    Implements,
    /// `Calls` 边：构造调用 `new Foo()`。
    Constructor,
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
    /// 符号声明签名（build 时 lang adapter 用 AST 提取：function/class 剥函数体，
    /// interface/enum 整节点）。契约 agent 输入来源，query 直读不回读源文件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
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
    /// 模块迁移状态（编排器在模块达终态时回写，见 04 § 5.7.1）；
    /// 复用 `ModuleStatus`——graph 节点状态与 `migration-state.json` 同口径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_status: Option<ModuleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_priority: Option<MigrationPriority>,
    /// RustTarget 节点的 Rust 类型种类（Struct/Enum/Trait/Function/Module/Crate）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_kind: Option<RustKind>,
    /// RustTarget 节点的 Rust 模块路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_path: Option<String>,
    /// RustTarget 节点所属的 crate 名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
}

impl SourceNode {
    /// 构造器：四必填字段，其余默认。
    pub fn new(id: NodeId, node_type: NodeType, name: String, file_path: String) -> Self {
        Self {
            id,
            node_type,
            name,
            file_path,
            line_range: None,
            signature: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        }
    }
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
    pub sub_kind: Option<EdgeSubKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_notes: Option<String>,
    /// Imports 边专用：target 实际用到的依赖符号名（已排序去重）。
    ///
    /// `Some(names)` 仅当本依赖全部为具名导入（可裁剪）；含 namespace/default/
    /// side-effect/re-export 任一形式时为 `None`，语义="用到全部导出"（保守）。
    /// 供 footprint 估算与 `deps-of` 按符号裁剪使用（M3-DEC-01）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_symbols: Option<Vec<String>>,
}

impl Dependency {
    /// 构造器：三必填字段，其余使用 TreeSitter/1.0/None 默认值。
    pub fn new(source: NodeId, target: NodeId, edge_type: EdgeType) -> Self {
        Self {
            source,
            target,
            edge_type,
            provenance: Provenance::TreeSitter,
            weight: 1.0,
            sub_kind: None,
            mapping_notes: None,
            used_symbols: None,
        }
    }

    pub fn with_sub_kind(mut self, sub_kind: EdgeSubKind) -> Self {
        self.sub_kind = Some(sub_kind);
        self
    }
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
