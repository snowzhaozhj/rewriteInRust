//! 语言适配器抽象层。
//!
//! `LanguageAdapter` trait 定义了源码分析的完整合约：
//! 文件识别、单文件分析（节点+边+导入信息）。
//! 各语言（TypeScript/Python/C/Go）实现此 trait 即可接入。

pub mod typescript;

use std::collections::HashMap;
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
    /// 所有导出名称（含 re-export、type-only export、default）。
    pub exported_names: std::collections::HashSet<String>,
    /// 本地构造绑定（`const x = new Foo()` → `"x" → "Foo"`）。
    /// 用于跨文件方法调用解析：`x.method()` → 查 `Foo.method` 方法节点。
    pub constructor_bindings: HashMap<String, String>,
}

/// 导入的种类（互斥，枚举消除原 `is_type_only`/`is_side_effect`/`is_dynamic`
/// 三个布尔字段的非法组合，如「同时是 side-effect 又是 dynamic」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// 普通值导入（`import { a } from 'x'`）。
    StaticValue,
    /// type-only 导入（`import type { T } from 'x'`）。
    StaticType,
    /// side-effect 导入（`import 'x'`，无符号）。
    SideEffect,
    /// 动态导入（`import('x')`）。
    Dynamic,
}

/// 导入信息。
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// 模块路径（如 `./utils`、`express`）。
    pub module_path: String,
    /// 导入的符号列表。
    pub symbols: Vec<ImportedSymbol>,
    /// 导入种类。
    pub kind: ImportKind,
}

/// 导入符号的种类（互斥，枚举消除原 `is_default`/`is_namespace`
/// 同时为真的非法组合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// 具名导入（`import { a }`）。
    Named,
    /// 默认导入（`import X`）。
    Default,
    /// namespace 导入（`import * as X`）。
    Namespace,
}

/// 单个导入符号。
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    /// 原始名称。
    pub name: String,
    /// 别名（如有）。
    pub alias: Option<String>,
    /// 符号种类。
    pub kind: SymbolKind,
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

    /// 判断文件是否属于该语言（可包含启发式逻辑，如排除 .d.ts）。
    fn can_handle(&self, path: &Path) -> bool;

    /// 模块解析时的候选扩展名（不带点，如 "ts", "tsx"）。
    ///
    /// 用于 `resolve_import` 生成候选路径，与 `can_handle` 职责分离：
    /// `can_handle` 决定文件归属，此方法决定解析时尝试哪些后缀。
    fn resolve_extensions(&self) -> &[&str];

    /// 分析单个文件，返回节点、边和依赖信息。
    ///
    /// `source` 为文件内容，`rel_path` 为相对于项目根的路径。
    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis>;
}
