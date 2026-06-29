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

/// 文件的机械属性分类（M3-DEC-01 决策②）。
///
/// 只有 `Barrel`/`PureType`/`PureConstant` 三类**且无危险信号且 footprint 小**才"可证明
/// 机械"（走轻量路径合批）；`Normal`（含函数/类/控制流）永不机械。`Normal` 是保守默认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// 纯 re-export 转发（barrel，如 `export * from`/`export {X} from`），无自身定义。
    Barrel,
    /// 纯类型声明（interface/type/enum），无值定义。
    PureType,
    /// 纯常量（`const` 字面量绑定），无函数/类/控制流。
    PureConstant,
    /// 其余（含函数/类/可变变量/控制流/表达式语句）——非机械。
    Normal,
}

/// 危险信号类别（M3-DEC-01 决策②(b)）。
///
/// 命中任一 → 该文件即"非机械"（走重流程）+ 应注入对应 porting 规则 + 加定向测试。
/// 与"流程深浅"是两件事：重流程本身不抓陷阱，规则注入才是针对性的一刀。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerCategory {
    /// 数值精度（Math./浮点/Number/parseInt/NaN/Infinity）。
    NumericPrecision,
    /// 并发（async/await/Promise.all/定时器）。
    Concurrency,
    /// 动态/反射（eval/typeof/instanceof/as any/getattr/metaclass）。
    DynamicReflection,
    /// IO 副作用（fs/net/process 等 import 或顶层表达式语句）。
    IoSideEffect,
    /// FFI / 跨语言边界（如 napi、ctypes、cgo——主要供非 TS 语言）。
    Ffi,
    /// 跨文件共享可变全局（顶层 `let`/`var` 可变绑定）。
    SharedMutableGlobal,
}

impl DangerCategory {
    /// 该陷阱的人读说明，供翻译上下文注入 / dry-run 报告展示。
    ///
    /// 不在此硬编码具体 RULE-NN（除已核实的 RULE-22 异步原语）——规则注入由 translator
    /// 依完整规则目录决定，避免在核心层固化可能漂移的规则映射。
    pub fn concern(&self) -> &'static str {
        match self {
            DangerCategory::NumericPrecision => {
                "数值精度：JS number 为 f64，整数运算/取整与 Rust i64/f64 语义不同，需对边界值定向测试"
            }
            DangerCategory::Concurrency => {
                "并发：async/Promise 语义需映射到 tokio（RULE-22 异步原语），注意执行顺序与取消语义"
            }
            DangerCategory::DynamicReflection => {
                "动态/反射：typeof/instanceof/any 等运行时类型操作无法静态翻译，需显式建模"
            }
            DangerCategory::IoSideEffect => {
                "IO 副作用：文件/网络/进程调用，需隔离副作用并对错误路径定向测试"
            }
            DangerCategory::Ffi => "FFI：跨语言边界，按 degrade_skip 评估或选 Rust 替代 crate",
            DangerCategory::SharedMutableGlobal => {
                "跨文件共享可变全局（顶层 let/var）：Rust 需 OnceLock/Mutex 等显式同步"
            }
        }
    }
}

/// 单文件的机械/危险分类结果（M3-DEC-01）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileClassification {
    /// 文件机械属性。
    pub file_kind: FileKind,
    /// 命中的危险信号类别（排序去重）。
    pub danger: Vec<DangerCategory>,
}

impl FileClassification {
    /// 保守默认：`Normal` + 无危险（未实现分类的语言用此，绝不会被判机械）。
    pub fn conservative() -> Self {
        Self {
            file_kind: FileKind::Normal,
            danger: Vec::new(),
        }
    }

    /// "可证明机械"判定 = 文件类型属于 Barrel/PureType/PureConstant **且**无任何危险信号。
    ///
    /// 注：MDR-011 拆解已**不再**用机械门做合批/流程分流（改目录+耦合凝聚，见
    /// `graph/decompose.rs`）。本判定**保留供 PR-2 轻量翻译路径**对纯机械簇做更省的处理选型，
    /// 当前仅单测引用——勿当死代码删除。
    pub fn is_mechanical(&self) -> bool {
        self.danger.is_empty() && !matches!(self.file_kind, FileKind::Normal)
    }
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

    /// 机械/危险分类（M3-DEC-01 拆解引擎用）。
    ///
    /// 区别于 `detect_tier`（流程档位）：此方法判"是否可证明机械"（可合批走轻量路径）
    /// 与"命中哪些危险信号类别"（规则注入 + 定向测试）。两者正交。
    /// 默认返回保守的 `Normal`+无危险——未实现分类的语言绝不会被合批，安全。
    fn classify_file(&mut self, _source: &str) -> FileClassification {
        FileClassification::conservative()
    }

    /// 探测项目的源码根目录（相对于 `project_root`）。
    ///
    /// 默认实现：递归检查 `src/` 是否含 `resolve_extensions()` 匹配的源文件。
    /// 返回 `None` 表示未探测到，由调用方回退 `"."`。
    /// 语言有特殊项目布局约定时 override（如 Python flat-package）。
    fn detect_source_root(&self, project_root: &Path) -> Option<String> {
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && dir_has_source_files(&src_dir, self.resolve_extensions(), 5) {
            return Some("src".to_string());
        }
        None
    }
}

/// 目录下是否含有指定扩展名的源文件（递归，过滤 EXCLUDED_DIRS，深度限制）。
fn dir_has_source_files(dir: &Path, extensions: &[&str], depth: u32) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let ft = entry.file_type();
        if ft.as_ref().is_ok_and(|t| t.is_file()) {
            if entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e))
            {
                return true;
            }
        } else if depth > 0 && ft.as_ref().is_ok_and(|t| t.is_dir()) {
            let name = entry.file_name();
            let n = name.to_string_lossy();
            if !n.starts_with('.')
                && !crate::types::common::EXCLUDED_DIRS.contains(&n.as_ref())
                && dir_has_source_files(&entry.path(), extensions, depth - 1)
            {
                return true;
            }
        }
    }
    false
}
