//! 语言适配器抽象层。
//!
//! `LanguageAdapter` trait 定义了源码分析的完整合约：
//! 文件识别、单文件分析（节点+边+导入信息）。
//! 各语言（TypeScript/Python/C/Go）实现此 trait 即可接入。

pub mod python;
pub mod registry;
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
    /// 实例变量名 → 类型名映射（TS `new Foo()` / Python `Foo()` 均填充）。
    pub instance_type_bindings: HashMap<String, String>,
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
    /// 是否为 re-export（`export {X} from 'm'` / `export * from 'm'`），区别于
    /// 普通 `import`。re-export 把 `module_path` 的符号转手导出，本文件并不"使用"它们；
    /// graph 层据此做透传转发：消费方 `import {X} from './barrel'` 解析到 X 真正定义处,
    /// 而非 barrel——否则 barrel re-export 会制造虚假循环依赖（如 mobx `internal.ts`）。
    /// `symbols` 非空 = 具名 re-export（含 `export * as ns`）；为空 = `export *` 通配。
    pub reexport: bool,
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

    /// import specifier 中可被省略/替换的扩展名（含点），如 TS ESM 的
    /// `[".js", ".jsx", ".mjs", ".cjs"]`——NodeNext 规范要求相对 import 带这些扩展名，
    /// 但实际指向同名源文件（`.ts`/`.tsx`）。
    ///
    /// `resolve_import` 据此解析：先精确匹配原始路径（命中真实同扩展名文件），再 strip
    /// 这些扩展名按 [`resolve_extensions`](Self::resolve_extensions) 候选重试。语言无关的
    /// graph 层只消费此列表，不内嵌任何具体扩展名字面量。默认空——多数语言的 import 不带
    /// 源文件扩展名。
    fn import_specifier_extensions(&self) -> &[&str] {
        &[]
    }

    /// 分析单个文件，返回节点、边和依赖信息。
    ///
    /// `source` 为文件内容，`rel_path` 为相对于项目根的路径。
    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis>;

    /// 解析 import specifier 到相对文件路径。
    ///
    /// `specifier`：原始 import 路径（如 `./utils`、`express`）。
    /// `current_file`：当前文件的项目相对路径。
    /// `exists`：检查候选路径是否在项目文件集中。
    /// 返回 `None` 表示外部依赖或无法解析。
    fn resolve_import(
        &self,
        specifier: &str,
        current_file: &str,
        exists: &dyn Fn(&str) -> bool,
    ) -> Option<String>;

    /// 评估源码的翻译复杂度分档。
    ///
    /// 什么算"危险信号"是语言特定的判断（TS: async/conditional_type；
    /// Python: metaclass/dynamic_attr），由各 adapter 内部决定。
    /// 解析失败时返回 Full（保守不降档）。
    fn detect_tier(&mut self, source: &str) -> crate::types::state::ModuleTier;
}
