/// 基础类型定义：项目级通用标识符、范围、语言枚举。
use serde::{Deserialize, Serialize};

/// 图节点的唯一标识符。
///
/// 格式：`{type}:{file_path}:{name}`，例如 `file:src/utils.ts` 或
/// `function:src/utils.ts:clamp`。
pub type NodeId = String;

/// 图边的唯一标识符（源节点 + 目标节点 + 边类型的组合）。
pub type EdgeId = String;

/// 源码行范围（1-based，闭区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
}

/// 源语言枚举。M3+ 扩展 Python/C/Go。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceLang {
    TypeScript,
    Python,
    C,
    Go,
}

impl std::fmt::Display for SourceLang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeScript => write!(f, "typescript"),
            Self::Python => write!(f, "python"),
            Self::C => write!(f, "c"),
            Self::Go => write!(f, "go"),
        }
    }
}

/// 复杂度等级（由 profile 模块评估）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
}

/// 模块风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// 迁移优先级（1 = 最高优先，无依赖的叶节点先迁移）。
pub type MigrationPriority = u32;

/// 时间戳（ISO 8601 字符串）。
pub type Timestamp = String;

/// Schema 版本号。
pub type SchemaVersion = String;
