//! 语言适配器抽象层。
//!
//! `LanguageAdapter` trait 定义了源码分析的完整合约：
//! 文件识别、单文件分析（节点+边+导入信息）。
//! 各语言（TypeScript/Python/C/Go）实现此 trait 即可接入。

pub mod typescript;

use std::path::Path;

use crate::error::Result;
use crate::types::common::SourceLang;
use crate::types::graph::{Dependency, SourceNode};

/// 单个文件的完整分析结果。
#[derive(Debug, Clone)]
pub struct FileAnalysis {
    /// 该文件产出的所有节点（File 节点 + 符号节点）。
    pub nodes: Vec<SourceNode>,
    /// 文件内部的边（Contains / Extends / Exports）。
    pub edges: Vec<Dependency>,
    /// 导入信息（用于构建跨文件 Imports 边）。
    pub imports: Vec<ImportInfo>,
    /// 文件内的函数调用（用于构建跨文件 Calls 边）。
    pub calls: Vec<CallInfo>,
}

/// 导入信息。
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// 模块路径（如 `./utils`、`express`）。
    pub module_path: String,
    /// 导入的符号列表。
    pub symbols: Vec<ImportedSymbol>,
    /// 是否为 type-only import。
    pub is_type_only: bool,
    /// 是否为 side-effect import（无符号导入）。
    pub is_side_effect: bool,
    /// 是否为动态 import。
    pub is_dynamic: bool,
}

/// 单个导入符号。
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    /// 原始名称。
    pub name: String,
    /// 别名（如有）。
    pub alias: Option<String>,
    /// 是否为默认导入。
    pub is_default: bool,
    /// 是否为 namespace 导入（`* as X`）。
    pub is_namespace: bool,
}

/// 函数调用信息。
#[derive(Debug, Clone)]
pub struct CallInfo {
    /// 被调用者名称（如 `clamp`、`obj.method`、`new EventEmitter`）。
    pub callee: String,
    /// 是否为构造函数调用（new）。
    pub is_constructor: bool,
}

/// 语言适配器 trait。
///
/// M1 实现 TypeScript，M3 扩展 Python/C/Go。
/// `build_graph` 通过此 trait 实现语言无关的图构建。
pub trait LanguageAdapter: Send {
    /// 该适配器支持的源语言。
    fn language(&self) -> SourceLang;

    /// 判断文件是否属于该语言。
    fn can_handle(&self, path: &Path) -> bool;

    /// 分析单个文件，返回节点、边和依赖信息。
    ///
    /// `source` 为文件内容，`rel_path` 为相对于项目根的路径。
    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis>;
}
