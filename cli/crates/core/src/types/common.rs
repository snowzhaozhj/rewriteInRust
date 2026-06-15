//! 基础类型定义：项目级通用标识符、范围、语言枚举。

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::graph::NodeType;

/// 图节点的唯一标识符。
///
/// 格式：`{type}:{file_path}` 或 `{type}:{file_path}:{name}`。
/// 例如 `file:src/utils.ts`、`function:src/utils.ts:clamp`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    /// 创建新的节点标识符（原始字符串）。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 构造文件节点 ID：`file:{rel_path}`。
    pub fn file(rel_path: &str) -> Self {
        Self(format!("{}:{rel_path}", NodeType::File))
    }

    /// 构造符号节点 ID：`{type}:{rel_path}:{name}`。
    pub fn symbol(node_type: NodeType, rel_path: &str, name: &str) -> Self {
        Self(format!("{node_type}:{rel_path}:{name}"))
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 解析节点类型前缀。
    pub fn kind(&self) -> Option<NodeType> {
        let prefix = self.0.split(':').next()?;
        prefix.parse().ok()
    }

    /// 解析文件路径部分。
    pub fn file_path(&self) -> Option<&str> {
        let mut parts = self.0.splitn(3, ':');
        parts.next()?; // type prefix
        parts.next() // file_path (or file_path:name combined for 2-part IDs)
    }

    /// 解析符号名称（仅 3 段 ID 有值）。
    pub fn symbol_name(&self) -> Option<&str> {
        let mut parts = self.0.splitn(3, ':');
        parts.next()?; // type prefix
        parts.next()?; // file_path
        parts.next() // name
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 图边的唯一标识符（源节点 + 目标节点 + 边类型的组合）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(String);

impl EdgeId {
    /// 创建新的边标识符。
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for EdgeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for EdgeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 源码行范围（1-based，闭区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// 起始行号。
    pub start_line: u32,
    /// 结束行号。
    pub end_line: u32,
}

/// 源语言枚举。M3+ 扩展 Python/C/Go。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SourceLang {
    /// TypeScript 源语言。
    #[serde(rename = "typescript")]
    #[strum(serialize = "typescript")]
    TypeScript,
    /// Python 源语言。
    Python,
    /// C 源语言。
    C,
    /// Go 源语言。
    Go,
}

/// 复杂度等级（由 profile 模块评估）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Complexity {
    /// 简单模块。
    Simple,
    /// 中等复杂度模块。
    Moderate,
    /// 高复杂度模块。
    Complex,
}

/// 模块风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// 低风险。
    Low,
    /// 中风险。
    Medium,
    /// 高风险。
    High,
}

/// 迁移优先级（1 = 最高优先，无依赖的叶节点先迁移）。
pub type MigrationPriority = u32;

/// 时间戳（ISO 8601 字符串）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// 创建新的时间戳。
    pub fn new(ts: impl Into<String>) -> Self {
        Self(ts.into())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 校验是否为合法 ISO 8601 / RFC 3339 时间戳。
    pub fn is_valid(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.0).is_ok()
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Timestamp {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Timestamp {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Schema 版本号。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(String);

impl SchemaVersion {
    /// 创建新的 Schema 版本号。
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }

    /// 返回内部字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SchemaVersion {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SchemaVersion {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 文件遍历时排除的目录名。
pub const EXCLUDED_DIRS: &[&str] = &["node_modules", ".git", "dist", "build", "target"];
