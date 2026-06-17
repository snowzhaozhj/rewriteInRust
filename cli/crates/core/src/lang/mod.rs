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
    // TODO(M3): constructor_bindings / ImportKind::StaticType / SymbolKind::Default
    // 是 TS 特有概念，M3 多语言扩展时应下沉到 TS adapter 或用通用 metadata 传递。
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

/// 模块复杂度分档的语言无关信号（由各语言 adapter 的 AST 扫描产出）。
///
/// `detect` 模块据此映射为 `ModuleTier`（Trivial/Standard/Full），
/// 映射策略与语言无关——adapter 只负责报告"有什么"，策略层决定"算什么档"。
#[derive(Debug, Clone, Default)]
pub struct TierSignals {
    /// 是否含 async/await。
    pub has_async: bool,
    /// 是否含 try-catch / throw 错误路径。
    pub has_error_handling: bool,
    /// 是否含并发模式（Promise.all 等）。
    pub has_concurrency: bool,
    /// 是否含 I/O 操作（fs/net/http 等模块导入）。
    pub has_io: bool,
    /// 是否含数值计算（Math.* / parseInt / NaN 等）。
    pub has_numeric: bool,
    /// 是否含全局可变状态（顶层 let/var）。
    pub has_global_mutable_state: bool,
    /// 是否含动态类型操作（typeof / instanceof / as any / as unknown）。
    pub has_dynamic_types: bool,
    /// 是否含条件类型或复杂泛型约束。
    pub has_conditional_types: bool,
    /// 是否含 unknown / never / any 类型注解。
    pub has_unresolvable_types: bool,
    /// 是否含非 trivial 内容（函数体/类/运行时逻辑等）。
    pub has_non_trivial_content: bool,
    /// 是否含顶层副作用表达式。
    pub has_side_effects: bool,
}

impl TierSignals {
    /// 是否含任一危险信号（映射 Full 的充分条件）。
    pub fn has_any_danger(&self) -> bool {
        self.has_async
            || self.has_error_handling
            || self.has_concurrency
            || self.has_io
            || self.has_numeric
            || self.has_global_mutable_state
            || self.has_dynamic_types
            || self.has_conditional_types
            || self.has_unresolvable_types
            || self.has_side_effects
    }
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

    /// 扫描源码产出复杂度分档信号（语言特定 AST 扫描，结果为语言无关信号）。
    ///
    /// `detect` 模块据此映射为 `ModuleTier`。解析失败时返回全 true 信号（保守不降档）。
    fn detect_tier_signals(&mut self, source: &str) -> TierSignals;
}
