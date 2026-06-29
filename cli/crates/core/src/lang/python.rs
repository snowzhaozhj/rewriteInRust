//! Python 语言适配器。
//!
//! 基于 tree-sitter-python 的单文件分析。M3 Sprint B 逐步实现：
//! - PY-01: 结构体 + language/can_handle/detect_tier
//! - PY-02~06: analyze_file / resolve_import

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::{normalize_path, NodeId, SourceLang, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};
use crate::types::state::ModuleTier;

use super::{
    CallInfo, DangerCategory, FileAnalysis, FileClassification, FileKind, ImportInfo, ImportKind,
    ImportedSymbol, LanguageAdapter, SymbolKind,
};

/// Python 语言适配器（基于 tree-sitter-python）。
pub struct PythonAdapter {
    parser: Parser,
}

impl PythonAdapter {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::language())
            .map_err(|e| MigrateError::Config(format!("tree-sitter Python 语法加载失败: {e}")))?;
        Ok(Self { parser })
    }
}

impl LanguageAdapter for PythonAdapter {
    fn language(&self) -> SourceLang {
        SourceLang::Python
    }

    fn can_handle(&self, path: &Path) -> bool {
        let ext = path.extension().unwrap_or_default();
        ext == "py"
    }

    fn resolve_extensions(&self) -> &[&str] {
        &["py"]
    }

    fn detect_source_root(&self, project_root: &Path) -> Option<String> {
        // 优先走默认的 src/ 检查
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && super::dir_has_source_files(&src_dir, self.resolve_extensions(), 5) {
            return Some("src".to_string());
        }
        // Python flat-package：唯一顶层含 __init__.py 的包目录
        let Ok(entries) = std::fs::read_dir(project_root) else {
            return None;
        };
        let packages: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .filter(|e| {
                let name = e.file_name();
                let n = name.to_string_lossy();
                !n.starts_with('.')
                    && !crate::types::common::EXCLUDED_DIRS.contains(&n.as_ref())
                    && e.path().join("__init__.py").exists()
            })
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        if packages.len() == 1 {
            return Some(packages.into_iter().next().unwrap());
        }
        None
    }

    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| MigrateError::Parse {
                path: rel_path.into(),
            })?;

        let mut ctx = PyAnalysisContext {
            rel_path,
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
            exported_names: HashSet::new(),
            instance_type_bindings: HashMap::new(),
        };

        collect_all_exports(tree.root_node(), source, &mut ctx.exported_names);

        ctx.nodes.push(SourceNode::new(
            NodeId::file(rel_path),
            NodeType::File,
            rel_path.to_string(),
            rel_path.to_string(),
        ));

        walk_py_ast(tree.root_node(), &mut ctx, false);

        let file_id = NodeId::file(rel_path);
        for node in &ctx.nodes {
            if node.is_exported && node.node_type != NodeType::File {
                ctx.edges.push(Dependency::new(
                    file_id.clone(),
                    node.id.clone(),
                    EdgeType::Exports,
                ));
            }
        }

        Ok(FileAnalysis {
            nodes: ctx.nodes,
            edges: ctx.edges,
            imports: ctx.imports,
            calls: ctx.calls,
            exported_names: ctx.exported_names,
            instance_type_bindings: ctx.instance_type_bindings,
        })
    }

    fn resolve_import(
        &self,
        specifier: &str,
        current_file: &str,
        exists: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        if specifier.is_empty() {
            return None;
        }

        if !specifier.starts_with('.') {
            // 绝对导入：尝试项目内文件
            let as_path = specifier.replace('.', "/");
            let candidate_py = format!("{as_path}.py");
            if exists(&candidate_py) {
                return Some(candidate_py);
            }
            let candidate_init = format!("{as_path}/__init__.py");
            if exists(&candidate_init) {
                return Some(candidate_init);
            }
            return None;
        }

        // 相对导入
        let dot_count = specifier.chars().take_while(|&c| c == '.').count();
        let remainder = &specifier[dot_count..];

        let current_dir = Path::new(current_file).parent().unwrap_or(Path::new(""));
        let depth = current_dir.components().count();
        if dot_count - 1 > depth {
            return None;
        }

        let mut base = current_dir.to_path_buf();
        for _ in 0..(dot_count - 1) {
            base = base.parent().unwrap_or(Path::new("")).to_path_buf();
        }

        let target = if remainder.is_empty() {
            base.clone()
        } else {
            base.join(remainder.replace('.', "/"))
        };

        let normalized = normalize_path(&target)?;

        if normalized.is_empty() {
            // 根目录包的 __init__.py
            if exists("__init__.py") {
                return Some("__init__.py".to_string());
            }
            return None;
        }

        let candidate_py = format!("{normalized}.py");
        if exists(&candidate_py) {
            return Some(candidate_py);
        }
        let candidate_init = format!("{normalized}/__init__.py");
        if exists(&candidate_init) {
            return Some(candidate_init);
        }
        None
    }

    fn detect_tier(&mut self, source: &str) -> ModuleTier {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return ModuleTier::Full,
        };
        let root = tree.root_node();
        if root.has_error() {
            return ModuleTier::Full;
        }
        let signals = scan_tier_signals(root, source);
        if signals.has_danger {
            ModuleTier::Full
        } else if signals.has_non_trivial_content {
            ModuleTier::Standard
        } else {
            ModuleTier::Trivial
        }
    }

    fn classify_file(&mut self, source: &str) -> FileClassification {
        // 解析失败/含语法错误 → 保守 Normal（绝不会被判机械合批）。
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return FileClassification::conservative(),
        };
        let root = tree.root_node();
        if root.has_error() {
            return FileClassification::conservative();
        }
        classify_py(root, source)
    }
}

// === 机械/危险分类（M3-DEC-01；与 TS classify_ts 对齐）===

/// 顶层文件类型判定的累积标志。
#[derive(Default)]
struct PyKindFlags {
    /// 含函数/类(带方法)/控制流/可变全局/副作用表达式 → Normal。
    saw_logic: bool,
    /// 含纯常量字面量绑定（int/str/float/bool/None/tuple-of-immutables）。
    saw_const: bool,
    /// 含纯类型声明（类型别名/TypeVar/NewType/纯数据类即只有注解的 class）。
    saw_type: bool,
}

/// Python 文件机械/危险分类：file_kind（顶层结构）+ danger（全树扫描，6 类）。
///
/// 与 TS `classify_ts` 同构，但按 Python 惯用法判定：
/// - **Barrel**：仅 `import`/`from import`/`__all__`/docstring/空 → 纯转发壳（`__init__.py` 主力）。
/// - **PureType**：仅类型别名 / `TypeVar`·`NewType` / 仅注解无方法的 class（Protocol/TypedDict/Enum 常量）。
/// - **PureConstant**：仅不可变字面量绑定（`MAX = 100`）。
/// - **Normal**：其余（函数 / 带方法的 class / 控制流 / 可变全局 / 副作用）。
///
/// 危险信号独立于 file_kind 全树扫描；控制流本身**不**进 danger（"10 行带 if 不进重型"锚点）。
fn classify_py(root: Node, source: &str) -> FileClassification {
    let mut danger: BTreeSet<DangerCategory> = BTreeSet::new();
    collect_py_danger(root, source, &mut danger);

    let mut flags = PyKindFlags::default();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        py_update_kind_flags(child, source, &mut flags, &mut danger);
    }

    let file_kind = if flags.saw_logic {
        FileKind::Normal
    } else if flags.saw_const {
        FileKind::PureConstant
    } else if flags.saw_type {
        FileKind::PureType
    } else {
        // 仅 import / __all__ / docstring / 空：无自身定义的转发壳 → Barrel（机械）。
        FileKind::Barrel
    };

    FileClassification {
        file_kind,
        danger: danger.into_iter().collect(),
    }
}

/// 按单个顶层节点更新 file_kind 标志（顶层可变全局/`global` 顺带记 SharedMutableGlobal 危险）。
fn py_update_kind_flags(
    node: Node,
    source: &str,
    flags: &mut PyKindFlags,
    danger: &mut BTreeSet<DangerCategory>,
) {
    match node.kind() {
        // 转发/元数据/注释/空：不影响 file_kind（保持 Barrel 资格）。
        "import_statement" | "import_from_statement" | "comment" => {}
        "function_definition" => flags.saw_logic = true,
        "class_definition" => {
            if py_class_is_pure_type(node, source) {
                // 仅注解/常量/pass/docstring 的 class（Enum/TypedDict/Protocol/纯 dataclass）→ 类型。
                flags.saw_type = true;
            } else {
                // 含方法 / 控制流 / 副作用调用赋值 / 裸表达式 → 逻辑。
                flags.saw_logic = true;
            }
        }
        "decorated_definition" => {
            // 跟随内层定义判定；装饰器自身的副作用由 danger 扫描兜底。
            if let Some(inner) = py_decorated_inner(node) {
                py_update_kind_flags(inner, source, flags, danger);
            } else {
                flags.saw_logic = true;
            }
        }
        // 控制流 → Normal（非机械），但 `if TYPE_CHECKING:` 是纯类型导入守卫，保持机械资格。
        "if_statement" => {
            if !py_is_type_checking_guard(node, source) {
                flags.saw_logic = true;
            }
        }
        "global_statement" | "nonlocal_statement" => {
            flags.saw_logic = true;
            danger.insert(DangerCategory::SharedMutableGlobal);
        }
        "for_statement" | "while_statement" | "with_statement" | "try_statement"
        | "match_statement" => {
            flags.saw_logic = true;
        }
        "expression_statement" => {
            // 模块 docstring（裸 string）= 中性；其余裸表达式（如 `print(...)`）= 副作用 → Normal。
            match node.child(0).map(|n| n.kind()) {
                Some("string") | None => {}
                Some("assignment") => {
                    py_classify_assignment(node.child(0).unwrap(), source, flags, danger)
                }
                _ => flags.saw_logic = true,
            }
        }
        _ => flags.saw_logic = true,
    }
}

/// 顶层赋值的 file_kind 归类（区分类型别名 / 不可变常量 / 可变全局）。
fn py_classify_assignment(
    assign: Node,
    source: &str,
    flags: &mut PyKindFlags,
    danger: &mut BTreeSet<DangerCategory>,
) {
    // `__all__`/dunder 元数据赋值不影响 file_kind（保持 Barrel 资格）。
    if let Some(left) = assign.child_by_field_name("left") {
        let name = py_node_text(left, source);
        if name == "__all__" || (name.starts_with("__") && name.ends_with("__")) {
            return;
        }
    }
    match assign.child_by_field_name("right") {
        None => {
            // `x: int`（仅注解，无值）= 类型声明。
            flags.saw_type = true;
        }
        Some(right) => {
            if py_is_type_expr(right, source) {
                flags.saw_type = true;
            } else if py_is_immutable_literal(right) {
                flags.saw_const = true;
            } else {
                // 可变字面量（list/dict/set）或 call/其它 → 共享可变全局 / 副作用 → Normal。
                flags.saw_logic = true;
                if py_is_mutable_container_literal(right) {
                    danger.insert(DangerCategory::SharedMutableGlobal);
                }
            }
        }
    }
}

/// class 体是否为"纯类型/数据容器"（仅注解 / 不可变常量 / pass / docstring / comment，无方法、
/// 无控制流、无副作用调用赋值）。任一逻辑特征 → 非纯类型（→ Normal）。
fn py_class_is_pure_type(class_node: Node, source: &str) -> bool {
    let Some(body) = class_node.child_by_field_name("body") else {
        return true; // 无体（理论上不出现）→ 保守视作纯声明。
    };
    let mut cursor = body.walk();
    for stmt in body.named_children(&mut cursor) {
        match stmt.kind() {
            "comment" | "pass_statement" => {}
            "expression_statement" => match stmt.child(0).map(|n| n.kind()) {
                // docstring（裸 string）/ `x: int`（仅注解 assignment 无 right）→ 纯类型。
                Some("string") | None => {}
                Some("assignment") => {
                    let assign = stmt.child(0).unwrap();
                    // `x: int` 仅注解（无 right）/ `x = <不可变常量|类型表达式>` 可接受；
                    // 其余（call/可变容器）→ 逻辑。
                    match assign.child_by_field_name("right") {
                        None => {}
                        Some(right)
                            if py_is_immutable_literal(right) || py_is_type_expr(right, source) => {
                        }
                        Some(_) => return false,
                    }
                }
                _ => return false,
            },
            // 方法 / 控制流 / 其它语句 → 含逻辑。
            _ => return false,
        }
    }
    true
}

/// decorated_definition 的被装饰定义（function/class）。
fn py_decorated_inner(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let inner = node
        .children(&mut cursor)
        .find(|c| matches!(c.kind(), "function_definition" | "class_definition"));
    inner
}

/// `if TYPE_CHECKING:` / `if typing.TYPE_CHECKING:` 守卫（条件文本以 `TYPE_CHECKING` 结尾）。
fn py_is_type_checking_guard(if_node: Node, source: &str) -> bool {
    if_node
        .child_by_field_name("condition")
        .map(|c| py_node_text(c, source))
        .is_some_and(|t| t == "TYPE_CHECKING" || t.ends_with(".TYPE_CHECKING"))
}

/// RHS 是否为类型表达式（泛型下标 `list[int]` / `TypeVar`·`NewType`·`TypeAlias` 构造）。
fn py_is_type_expr(node: Node, source: &str) -> bool {
    match node.kind() {
        "subscript" => true, // list[int] / Dict[str, int] / Optional[X]
        "call" => node
            .child_by_field_name("function")
            .map(|f| py_node_text(f, source))
            .is_some_and(|name| {
                matches!(name, "TypeVar" | "NewType" | "ParamSpec" | "TypeVarTuple")
            }),
        _ => false,
    }
}

/// RHS 是否为不可变字面量（int/float/str/bool/None/可选取负数 + 全不可变元素的 tuple）。
fn py_is_immutable_literal(node: Node) -> bool {
    match node.kind() {
        "integer" | "float" | "concatenated_string" | "true" | "false" | "none" => true,
        // f-string 的节点 kind 仍是 `string`，但含 `interpolation` 子节点——内插表达式可含
        // 副作用调用（`f"{compute()}"`），非不可变常量，须排除（否则有副作用的文件误判机械）。
        "string" => {
            let mut cursor = node.walk();
            let has_interp = node
                .children(&mut cursor)
                .any(|c| c.kind() == "interpolation");
            !has_interp
        }
        "unary_operator" => node
            .child_by_field_name("argument")
            .is_some_and(py_is_immutable_literal),
        "tuple" => {
            let mut cursor = node.walk();
            let all_immut = node
                .named_children(&mut cursor)
                .all(py_is_immutable_literal);
            all_immut
        }
        _ => false,
    }
}

/// RHS 是否为可变容器字面量（list/dict/set 及其推导式）——模块级即共享可变全局。
fn py_is_mutable_container_literal(node: Node) -> bool {
    matches!(
        node.kind(),
        "list"
            | "dictionary"
            | "set"
            | "list_comprehension"
            | "dictionary_comprehension"
            | "set_comprehension"
    )
}

/// 全树扫描 6 类危险信号（控制流不在内——见锚点）。
fn collect_py_danger(root: Node, source: &str, danger: &mut BTreeSet<DangerCategory>) {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        match cur.kind() {
            // 并发：async/await。
            "async" | "await" => {
                danger.insert(DangerCategory::Concurrency);
            }
            // 动态属性协议方法。
            "function_definition" => {
                if let Some(name_node) = cur.child_by_field_name("name") {
                    if matches!(
                        py_node_text(name_node, source),
                        "__getattr__" | "__setattr__" | "__delattr__" | "__getattribute__"
                    ) {
                        danger.insert(DangerCategory::DynamicReflection);
                    }
                }
            }
            // metaclass= 关键字参数。
            "keyword_argument" => {
                if cur
                    .child_by_field_name("name")
                    .is_some_and(|n| py_node_text(n, source) == "metaclass")
                {
                    danger.insert(DangerCategory::DynamicReflection);
                }
            }
            "call" => collect_py_call_danger(cur, source, danger),
            // import 模块名分类（io / 并发 / 动态 / FFI / 数值）。
            "import_statement" | "import_from_statement" => {
                collect_py_import_danger(cur, source, danger);
            }
            // math.* 属性访问（即便未直接 call，如 math.pi）→ 数值精度。
            "attribute" => {
                if let Some(obj) = cur.child_by_field_name("object") {
                    if py_node_text(obj, source) == "math" {
                        danger.insert(DangerCategory::NumericPrecision);
                    }
                }
            }
            _ => {}
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// call 节点的危险信号（按被调名归类）。
fn collect_py_call_danger(call: Node, source: &str, danger: &mut BTreeSet<DangerCategory>) {
    let Some(func) = call.child_by_field_name("function") else {
        return;
    };
    let name = py_node_text(func, source);
    // 动态/反射执行。
    if matches!(
        name,
        "eval"
            | "exec"
            | "compile"
            | "__import__"
            | "getattr"
            | "setattr"
            | "delattr"
            | "globals"
            | "locals"
            | "vars"
    ) {
        danger.insert(DangerCategory::DynamicReflection);
    }
    // IO 副作用。
    if name == "open" || name == "input" || name.starts_with("subprocess.") {
        danger.insert(DangerCategory::IoSideEffect);
    }
    // 数值精度（math./浮点构造）。
    if name.starts_with("math.") || matches!(name, "float" | "complex" | "Decimal" | "Fraction") {
        danger.insert(DangerCategory::NumericPrecision);
    }
    // 并发原语。
    if name.starts_with("threading.")
        || name.starts_with("asyncio.")
        || name.starts_with("multiprocessing.")
        || name.starts_with("concurrent.")
        || matches!(name, "Thread" | "Lock" | "RLock" | "Pool")
    {
        danger.insert(DangerCategory::Concurrency);
    }
    // FFI。
    if name.starts_with("ctypes.") || name.starts_with("cffi.") || name == "CDLL" {
        danger.insert(DangerCategory::Ffi);
    }
}

/// import 模块名 → 危险类别（覆盖 `import x` / `import a.b as c` / `import a, b` / `from x import y`）。
fn collect_py_import_danger(node: Node, source: &str, danger: &mut BTreeSet<DangerCategory>) {
    match node.kind() {
        "import_from_statement" => {
            if let Some(m) = node.child_by_field_name("module_name") {
                classify_module_root(py_node_text(m, source), danger);
            }
        }
        _ => {
            // `import a.b.c` / `import x as y` / `import a, b`：逐个 import 名（含别名）分类。
            // 别名导入 `import x as y` 是 `aliased_import` 节点，模块名在其 `name` 字段（dotted_name）；
            // 同行多导入 `import a, b` 有多个并列 dotted_name——都要遍历（否则危险信号漏判）。
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                let module_node = match child.kind() {
                    "dotted_name" => Some(child),
                    "aliased_import" => child.child_by_field_name("name"),
                    _ => None,
                };
                if let Some(mn) = module_node {
                    classify_module_root(py_node_text(mn, source), danger);
                }
            }
        }
    }
}

/// 模块点分路径的根名 → 危险类别。
fn classify_module_root(module: &str, danger: &mut BTreeSet<DangerCategory>) {
    let root_mod = module
        .trim_start_matches('.')
        .split('.')
        .next()
        .unwrap_or("");
    match root_mod {
        "os" | "sys" | "subprocess" | "socket" | "shutil" | "io" | "pathlib" | "requests"
        | "urllib" | "http" | "aiohttp" | "tempfile" | "fileinput" => {
            danger.insert(DangerCategory::IoSideEffect);
        }
        "asyncio" | "threading" | "multiprocessing" | "concurrent" | "queue" | "_thread" => {
            danger.insert(DangerCategory::Concurrency);
        }
        "ctypes" | "cffi" => {
            danger.insert(DangerCategory::Ffi);
        }
        "importlib" | "inspect" => {
            danger.insert(DangerCategory::DynamicReflection);
        }
        // math 走 import 级识别——别名(`import math as m`)/from(`from math import sqrt`) 下
        // 调用文本不含 `math.` 前缀，仅靠 collect_py_call_danger 会漏判（主审 finder#1）。
        "math" | "decimal" | "fractions" | "cmath" | "statistics" => {
            danger.insert(DangerCategory::NumericPrecision);
        }
        _ => {}
    }
}

#[derive(Default)]
struct PyTierSignals {
    has_danger: bool,
    has_non_trivial_content: bool,
}

fn scan_tier_signals(root: Node, source: &str) -> PyTierSignals {
    let mut s = PyTierSignals::default();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "import_statement" | "import_from_statement" => {}
            "function_definition" | "class_definition" => {
                s.has_non_trivial_content = true;
                check_danger_in_subtree(child, source, &mut s);
            }
            "decorated_definition" => {
                s.has_non_trivial_content = true;
                check_danger_in_subtree(child, source, &mut s);
            }
            "expression_statement" => {
                let inner = child.child(0);
                if inner.is_some_and(|n| n.kind() != "assignment") {
                    s.has_non_trivial_content = true;
                }
                check_danger_in_subtree(child, source, &mut s);
            }
            "global_statement" | "nonlocal_statement" => {
                s.has_non_trivial_content = true;
                s.has_danger = true;
            }
            "try_statement" => {
                s.has_non_trivial_content = true;
                s.has_danger = true;
            }
            "for_statement" | "while_statement" | "if_statement" | "with_statement" => {
                s.has_non_trivial_content = true;
                check_danger_in_subtree(child, source, &mut s);
            }
            _ => {
                s.has_non_trivial_content = true;
            }
        }
    }
    s
}

fn check_danger_in_subtree(node: Node, source: &str, signals: &mut PyTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        match current.kind() {
            "async" => {
                signals.has_danger = true;
                return;
            }
            "try_statement" => {
                signals.has_danger = true;
                return;
            }
            "global_statement" | "nonlocal_statement" => {
                signals.has_danger = true;
                return;
            }
            // metaclass=... 在 class 定义的 argument_list 中
            "keyword_argument" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = &source[name_node.byte_range()];
                    if name == "metaclass" {
                        signals.has_danger = true;
                        return;
                    }
                }
            }
            // __getattr__/__setattr__/__delattr__ 方法定义（动态属性）
            "function_definition" => {
                if let Some(name_node) = current.child_by_field_name("name") {
                    let name = &source[name_node.byte_range()];
                    if name == "__getattr__" || name == "__setattr__" || name == "__delattr__" {
                        signals.has_danger = true;
                        return;
                    }
                }
            }
            // exec/eval（动态代码执行）
            "call" => {
                if let Some(func) = current.child_by_field_name("function") {
                    let name = &source[func.byte_range()];
                    if name == "exec" || name == "eval" {
                        signals.has_danger = true;
                        return;
                    }
                }
            }
            _ => {}
        }

        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

// === Python 分析上下文 ===

struct PyAnalysisContext<'a> {
    rel_path: &'a str,
    source: &'a str,
    nodes: Vec<SourceNode>,
    edges: Vec<Dependency>,
    imports: Vec<ImportInfo>,
    calls: Vec<CallInfo>,
    exported_names: HashSet<String>,
    instance_type_bindings: HashMap<String, String>,
}

// === __all__ 收集 ===

fn collect_all_exports(root: Node, source: &str, exports: &mut HashSet<String>) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            if let Some(assign) = child.child(0) {
                if assign.kind() == "assignment" {
                    if let Some(left) = assign.child_by_field_name("left") {
                        if py_node_text(left, source) == "__all__" {
                            if let Some(right) = assign.child_by_field_name("right") {
                                extract_all_strings(right, source, exports);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn extract_all_strings(node: Node, source: &str, exports: &mut HashSet<String>) {
    // __all__ = ["a", "b"] 或 ("a", "b")
    if node.kind() == "list" || node.kind() == "tuple" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "string" {
                let s = extract_string_content(child, source);
                if !s.is_empty() {
                    exports.insert(s);
                }
            }
        }
    }
}

fn extract_string_content(node: Node, source: &str) -> String {
    // string 节点包含 string_start + string_content + string_end 子节点
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string_content" {
            return py_node_text(child, source).to_string();
        }
    }
    // 回退：去除引号
    let raw = py_node_text(node, source);
    raw.trim_matches(|c: char| c == '\'' || c == '"')
        .to_string()
}

// === AST 遍历 ===

fn walk_py_ast(node: Node, ctx: &mut PyAnalysisContext, in_type_checking: bool) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                extract_py_import(child, ctx, in_type_checking);
            }
            "import_from_statement" => {
                extract_py_from_import(child, ctx, in_type_checking);
            }
            "function_definition" => {
                extract_py_function(child, ctx, &[]);
            }
            "class_definition" => {
                extract_py_class(child, ctx);
            }
            "decorated_definition" => {
                handle_decorated(child, ctx);
            }
            "if_statement" => {
                handle_if_type_checking(child, ctx, in_type_checking);
            }
            "expression_statement" => {
                // 顶层调用提取
                extract_calls_from_node(child, ctx);
                // 顶层赋值中的构造绑定
                extract_assignment_bindings(child, ctx);
            }
            _ => {
                extract_calls_from_node(child, ctx);
            }
        }
    }
}

// === Import 提取 ===

fn extract_py_import(node: Node, ctx: &mut PyAnalysisContext, in_type_checking: bool) {
    // `import os`, `import os.path`, `import os as operating_system`
    let kind = if in_type_checking {
        ImportKind::StaticType
    } else {
        ImportKind::StaticValue
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                let module = py_node_text(child, ctx.source).to_string();
                ctx.imports.push(ImportInfo {
                    module_path: module.clone(),
                    symbols: vec![ImportedSymbol {
                        name: module,
                        alias: None,
                        kind: SymbolKind::Named,
                    }],
                    kind,
                    reexport: false,
                });
            }
            "aliased_import" => {
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");
                if let Some(name_n) = name_node {
                    let module = py_node_text(name_n, ctx.source).to_string();
                    let alias = alias_node.map(|a| py_node_text(a, ctx.source).to_string());
                    ctx.imports.push(ImportInfo {
                        module_path: module.clone(),
                        symbols: vec![ImportedSymbol {
                            name: module,
                            alias,
                            kind: SymbolKind::Named,
                        }],
                        kind,
                        reexport: false,
                    });
                }
            }
            _ => {}
        }
    }
}

fn extract_py_from_import(node: Node, ctx: &mut PyAnalysisContext, in_type_checking: bool) {
    // `from os import path`, `from . import utils`, `from ..models import Base`
    let kind = if in_type_checking {
        ImportKind::StaticType
    } else {
        ImportKind::StaticValue
    };

    let module_path = build_from_module_path(node, ctx.source);

    // 检测通配导入 `from x import *`
    let mut has_wildcard = false;
    let mut symbols = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "wildcard_import" => {
                has_wildcard = true;
            }
            "dotted_name" => {
                // 可能是 `from . import utils` 中的 import 目标（非 module_name 字段）
                if child.start_byte() > module_end_byte(node, ctx.source) {
                    let name = py_node_text(child, ctx.source).to_string();
                    symbols.push(ImportedSymbol {
                        name,
                        alias: None,
                        kind: SymbolKind::Named,
                    });
                }
            }
            "aliased_import" => {
                if let Some(name_n) = child.child_by_field_name("name") {
                    let name = py_node_text(name_n, ctx.source).to_string();
                    let alias = child
                        .child_by_field_name("alias")
                        .map(|a| py_node_text(a, ctx.source).to_string());
                    symbols.push(ImportedSymbol {
                        name,
                        alias,
                        kind: SymbolKind::Named,
                    });
                }
            }
            _ => {}
        }
    }

    let final_kind = if has_wildcard {
        ImportKind::SideEffect
    } else {
        kind
    };

    // `from . import utils` — 每个 symbol 可能是子模块，生成独立 ImportInfo
    // 使 graph 层能解析到 ./utils.py（而非仅 ./__init__.py）
    let is_pure_relative = module_path.chars().all(|c| c == '.');
    if is_pure_relative && !has_wildcard && !symbols.is_empty() {
        for sym in &symbols {
            ctx.imports.push(ImportInfo {
                module_path: format!("{}{}", module_path, sym.name),
                symbols: vec![sym.clone()],
                kind: final_kind,
                reexport: false,
            });
        }
    } else {
        ctx.imports.push(ImportInfo {
            module_path,
            symbols,
            kind: final_kind,
            reexport: false,
        });
    }
}

fn build_from_module_path(node: Node, source: &str) -> String {
    // 收集 leading dots + module_name（如果有）
    let mut dots = String::new();
    let mut module_name = String::new();

    let module_name_node = node.child_by_field_name("module_name");
    if let Some(mn) = module_name_node {
        module_name = py_node_text(mn, source).to_string();
    }

    // 计算 dots：从 `import_prefix` 子节点或直接 "." 子节点
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_prefix" {
            dots = py_node_text(child, source).to_string();
            break;
        }
        if child.kind() == "." {
            dots.push('.');
        }
        // 当遇到 "import" 关键字就停止收集 dots
        if child.kind() == "import" {
            break;
        }
        // 遇到 module_name（dotted_name）也停止
        if child.kind() == "dotted_name" || child.kind() == "relative_import" {
            break;
        }
    }

    if dots.is_empty() && module_name.is_empty() {
        return String::new();
    }

    format!("{dots}{module_name}")
}

fn module_end_byte(node: Node, source: &str) -> usize {
    // 找到 "import" 关键字的位置，之后的 dotted_name 才是被导入符号
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import" {
            return child.end_byte();
        }
    }
    // 回退：find "import" in text
    let text = &source[node.byte_range()];
    if let Some(pos) = text.find("import") {
        node.start_byte() + pos + 6
    } else {
        node.start_byte()
    }
}

// === if TYPE_CHECKING 检测 ===

fn handle_if_type_checking(node: Node, ctx: &mut PyAnalysisContext, already_in: bool) {
    // 检查条件是否为 TYPE_CHECKING
    let condition = node.child_by_field_name("condition");
    let is_tc = condition
        .map(|c| {
            let text = py_node_text(c, ctx.source);
            text == "TYPE_CHECKING" || text.ends_with(".TYPE_CHECKING")
        })
        .unwrap_or(false);

    let in_tc = already_in || is_tc;

    // 遍历 consequence（if 体）
    if let Some(body) = node.child_by_field_name("consequence") {
        if is_tc {
            walk_py_ast(body, ctx, true);
        } else {
            walk_py_ast(body, ctx, in_tc);
        }
    }

    // 遍历 else/elif 分支
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "else_clause" {
            if let Some(body) = child.child_by_field_name("body") {
                walk_py_ast(body, ctx, already_in);
            }
        } else if child.kind() == "elif_clause" {
            if let Some(consequence) = child.child_by_field_name("consequence") {
                walk_py_ast(consequence, ctx, already_in);
            }
        }
    }
}

// === decorated_definition 处理 ===

fn handle_decorated(node: Node, ctx: &mut PyAnalysisContext) {
    let mut decorators = Vec::new();
    let mut inner = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "decorator" => {
                decorators.push(py_node_text(child, ctx.source).to_string());
            }
            "function_definition" => {
                inner = Some(child);
            }
            "class_definition" => {
                extract_py_class(child, ctx);
                return;
            }
            _ => {}
        }
    }

    if let Some(func_node) = inner {
        extract_py_function(func_node, ctx, &decorators);
    }
}

// === 函数提取 ===

fn extract_py_function(node: Node, ctx: &mut PyAnalysisContext, decorators: &[String]) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = py_node_text(name_node, ctx.source).to_string();
    let is_exported = ctx.exported_names.contains(&name);

    // 检查 async：如果 decorated_definition 包含 async，或者 node 的前一个兄弟是 async
    let is_async = {
        let parent = node.parent();
        let parent_text = parent.map(|p| py_node_text(p, ctx.source));
        parent_text
            .map(|t| t.starts_with("async ") || t.starts_with("async\n"))
            .unwrap_or(false)
            || py_node_text(node, ctx.source).starts_with("async ")
    };

    let signature = build_function_signature(node, ctx.source);

    let mut sn = SourceNode::new(
        NodeId::symbol(NodeType::Function, ctx.rel_path, &name),
        NodeType::Function,
        name.clone(),
        ctx.rel_path.to_string(),
    );
    sn.line_range = Some(py_node_span(node));
    sn.signature = Some(signature);
    sn.is_exported = is_exported;
    sn.is_async = is_async;

    if !decorators.is_empty() {
        sn.decorators = decorators.to_vec();
        if let Some(parent) = node.parent() {
            if parent.kind() == "decorated_definition" {
                sn.line_range = Some(py_node_span(parent));
            }
        }
    }

    ctx.nodes.push(sn);

    // 递归提取函数体内的调用
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_node(body, ctx);
    }
}

fn build_function_signature(node: Node, source: &str) -> String {
    // 从 def 到 body 开始之间的文本
    let start = node.start_byte();
    let end = node
        .child_by_field_name("body")
        .map(|b| b.start_byte())
        .unwrap_or_else(|| node.end_byte());
    let sig = source.get(start..end).unwrap_or("").trim_end();
    // 去除尾部的冒号
    sig.strip_suffix(':').unwrap_or(sig).trim_end().to_string()
}

// === 类提取 ===

fn extract_py_class(node: Node, ctx: &mut PyAnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = py_node_text(name_node, ctx.source).to_string();
    let is_exported = ctx.exported_names.contains(&name);
    let class_id = NodeId::symbol(NodeType::Class, ctx.rel_path, &name);

    let signature = build_class_signature(node, ctx.source);

    let mut sn = SourceNode::new(
        class_id.clone(),
        NodeType::Class,
        name.clone(),
        ctx.rel_path.to_string(),
    );
    sn.line_range = Some(py_node_span(node));
    sn.signature = Some(signature);
    sn.is_exported = is_exported;
    ctx.nodes.push(sn);

    // 基类 → Extends 边
    extract_py_bases(node, &class_id, ctx);

    // 方法 → Function 节点 + Contains 边
    if let Some(body) = node.child_by_field_name("body") {
        extract_py_methods(body, &class_id, &name, ctx);
    }
}

fn build_class_signature(node: Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node
        .child_by_field_name("body")
        .map(|b| b.start_byte())
        .unwrap_or_else(|| node.end_byte());
    let header = source.get(start..end).unwrap_or("").trim_end();
    let header = header.strip_suffix(':').unwrap_or(header).trim_end();

    // 方法列表骨架
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            let fn_node = match child.kind() {
                "function_definition" => Some(child),
                "decorated_definition" => {
                    let mut inner = None;
                    let mut ic = child.walk();
                    for c in child.children(&mut ic) {
                        if c.kind() == "function_definition" {
                            inner = Some(c);
                        }
                    }
                    inner
                }
                _ => None,
            };
            if let Some(f) = fn_node {
                if let Some(n) = f.child_by_field_name("name") {
                    methods.push(py_node_text(n, source).to_string());
                }
            }
        }
    }

    if methods.is_empty() {
        header.to_string()
    } else {
        format!("{header} [{methods}]", methods = methods.join(", "))
    }
}

fn extract_py_bases(node: Node, class_id: &NodeId, ctx: &mut PyAnalysisContext) {
    // class Foo(Base1, Base2): 的 superclasses 在 argument_list 子节点中
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "argument_list" {
            let mut arg_cursor = child.walk();
            for arg in child.children(&mut arg_cursor) {
                match arg.kind() {
                    "identifier" => {
                        let base_name = py_node_text(arg, ctx.source);
                        let target_id = NodeId::symbol(NodeType::Class, ctx.rel_path, base_name);
                        ctx.edges.push(Dependency::new(
                            class_id.clone(),
                            target_id,
                            EdgeType::Extends,
                        ));
                    }
                    "attribute" => {
                        let base_name = py_node_text(arg, ctx.source);
                        let target_id = NodeId::symbol(NodeType::Class, ctx.rel_path, base_name);
                        ctx.edges.push(Dependency::new(
                            class_id.clone(),
                            target_id,
                            EdgeType::Extends,
                        ));
                    }
                    "keyword_argument" => {
                        // metaclass=... 等，跳过
                    }
                    "call" => {
                        if let Some(func) = arg.child_by_field_name("function") {
                            let base_name = py_node_text(func, ctx.source);
                            let target_id =
                                NodeId::symbol(NodeType::Class, ctx.rel_path, base_name);
                            ctx.edges.push(Dependency::new(
                                class_id.clone(),
                                target_id,
                                EdgeType::Extends,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            break;
        }
    }
}

fn extract_py_methods(
    body: Node,
    class_id: &NodeId,
    class_name: &str,
    ctx: &mut PyAnalysisContext,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                extract_py_method(child, class_id, class_name, ctx, &[]);
            }
            "decorated_definition" => {
                let mut decorators = Vec::new();
                let mut func_node = None;
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    match inner.kind() {
                        "decorator" => {
                            decorators.push(py_node_text(inner, ctx.source).to_string());
                        }
                        "function_definition" => {
                            func_node = Some(inner);
                        }
                        _ => {}
                    }
                }
                if let Some(fn_node) = func_node {
                    extract_py_method(fn_node, class_id, class_name, ctx, &decorators);
                }
            }
            _ => {}
        }
    }
}

fn extract_py_method(
    node: Node,
    class_id: &NodeId,
    class_name: &str,
    ctx: &mut PyAnalysisContext,
    decorators: &[String],
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let method_name = py_node_text(name_node, ctx.source);
    let qualified_name = format!("{class_name}.{method_name}");

    let is_async = {
        let parent = node.parent();
        let parent_text = parent.map(|p| py_node_text(p, ctx.source));
        parent_text
            .map(|t| t.starts_with("async ") || t.starts_with("async\n"))
            .unwrap_or(false)
            || py_node_text(node, ctx.source).starts_with("async ")
    };

    let method_id = NodeId::symbol(NodeType::Function, ctx.rel_path, &qualified_name);

    let mut sn = SourceNode::new(
        method_id.clone(),
        NodeType::Function,
        qualified_name,
        ctx.rel_path.to_string(),
    );
    sn.line_range = Some(py_node_span(node));
    sn.is_async = is_async;
    sn.signature = Some(build_function_signature(node, ctx.source));
    if !decorators.is_empty() {
        sn.decorators = decorators.to_vec();
    }
    ctx.nodes.push(sn);

    // self → class_name 绑定，使 self.method() 调用可解析
    ctx.instance_type_bindings
        .insert("self".to_string(), class_name.to_string());

    ctx.edges.push(Dependency::new(
        class_id.clone(),
        method_id,
        EdgeType::Contains,
    ));

    // 递归提取方法体内的调用
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_from_node(body, ctx);
    }
}

// === 调用提取 ===

fn extract_calls_from_node(node: Node, ctx: &mut PyAnalysisContext) {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "call" {
            extract_single_call(current, ctx);
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn extract_single_call(node: Node, ctx: &mut PyAnalysisContext) {
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };

    let callee = match func.kind() {
        "identifier" => py_node_text(func, ctx.source).to_string(),
        "attribute" => py_node_text(func, ctx.source).to_string(),
        _ => return,
    };

    if callee.is_empty() {
        return;
    }

    // 首字母大写 → 构造调用
    let base_name = callee.rsplit('.').next().unwrap_or(&callee);
    let is_constructor = base_name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);

    ctx.calls.push(CallInfo {
        callee,
        is_constructor,
    });
}

// === 赋值绑定提取 ===

fn extract_assignment_bindings(node: Node, ctx: &mut PyAnalysisContext) {
    // expression_statement 的第一个子节点如果是 assignment
    let Some(assign) = node.child(0) else { return };
    if assign.kind() != "assignment" {
        return;
    }

    let Some(left) = assign.child_by_field_name("left") else {
        return;
    };
    let Some(right) = assign.child_by_field_name("right") else {
        return;
    };

    if left.kind() != "identifier" {
        return;
    }

    // 检查右侧是否为构造调用 Foo()
    if right.kind() == "call" {
        if let Some(func) = right.child_by_field_name("function") {
            let callee = py_node_text(func, ctx.source);
            let is_constructor = callee
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false);
            if is_constructor {
                let var_name = py_node_text(left, ctx.source).to_string();
                ctx.instance_type_bindings
                    .insert(var_name, callee.to_string());
            }
        }
    }
}

// === 工具函数 ===

fn py_node_text<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

fn py_node_span(node: Node) -> Span {
    Span {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_adapter() {
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(adapter.language(), SourceLang::Python);
    }

    #[test]
    fn can_handle_py_files() {
        let adapter = PythonAdapter::new().unwrap();
        assert!(adapter.can_handle(Path::new("main.py")));
        assert!(adapter.can_handle(Path::new("src/utils.py")));
        assert!(!adapter.can_handle(Path::new("main.ts")));
        assert!(!adapter.can_handle(Path::new("main.pyi")));
    }

    #[test]
    fn detect_tier_trivial() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
import os
from pathlib import Path

X = 42
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Trivial);
    }

    #[test]
    fn detect_tier_standard() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
def add(a: int, b: int) -> int:
    return a + b
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Standard);
    }

    #[test]
    fn detect_tier_full_async() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
import asyncio

async def fetch():
    await asyncio.sleep(1)
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_try_except() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
def risky():
    try:
        x = int("abc")
    except ValueError:
        pass
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_global() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
counter = 0
def inc():
    global counter
    counter += 1
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_eval() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
def dynamic(code):
    return eval(code)
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_toplevel_eval() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = "result = eval('1+1')\n";
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_toplevel_try() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
try:
    import optional_lib
except ImportError:
    optional_lib = None
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_metaclass() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
class Singleton(metaclass=SingletonMeta):
    pass
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_full_getattr() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
class Proxy:
    def __getattr__(self, name):
        return getattr(self._target, name)
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Full);
    }

    #[test]
    fn detect_tier_standard_decorated() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
@app.route("/")
def index():
    return "hello"
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Standard);
    }

    #[test]
    fn detect_tier_standard_toplevel_for() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
items = []
for i in range(10):
    items.append(i)
"#;
        assert_eq!(adapter.detect_tier(source), ModuleTier::Standard);
    }

    // === analyze_file / resolve_import 测试 ===

    #[test]
    fn analyze_import_statement() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = "import os\nimport os.path as p\n";
        let result = adapter.analyze_file(source, "test.py").unwrap();

        assert_eq!(result.imports.len(), 2);

        let imp_os = &result.imports[0];
        assert_eq!(imp_os.module_path, "os");
        assert_eq!(imp_os.symbols.len(), 1);
        assert_eq!(imp_os.symbols[0].name, "os");
        assert_eq!(imp_os.symbols[0].alias, None);
        assert_eq!(imp_os.kind, ImportKind::StaticValue);

        let imp_path = &result.imports[1];
        assert_eq!(imp_path.module_path, "os.path");
        assert_eq!(imp_path.symbols[0].name, "os.path");
        assert_eq!(imp_path.symbols[0].alias, Some("p".to_string()));
    }

    #[test]
    fn analyze_from_import() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = "from os import path\nfrom . import utils\nfrom ..models import Base\n";
        let result = adapter.analyze_file(source, "pkg/sub/test.py").unwrap();

        // from os import path → 1 entry
        // from . import utils → 1 entry with module_path=".utils" (submodule resolution)
        // from ..models import Base → 1 entry
        assert_eq!(result.imports.len(), 3);

        assert_eq!(result.imports[0].module_path, "os");
        assert_eq!(result.imports[0].symbols[0].name, "path");

        // `from . import utils` → module_path 带上 symbol 名以解析子模块
        assert_eq!(result.imports[1].module_path, ".utils");
        assert_eq!(result.imports[1].symbols[0].name, "utils");

        assert_eq!(result.imports[2].module_path, "..models");
        assert_eq!(result.imports[2].symbols[0].name, "Base");
    }

    #[test]
    fn analyze_type_checking_import() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .models import User
    import os

from .utils import helper
"#;
        let result = adapter.analyze_file(source, "pkg/test.py").unwrap();

        let tc_imports: Vec<_> = result
            .imports
            .iter()
            .filter(|i| i.kind == ImportKind::StaticType)
            .collect();
        assert_eq!(
            tc_imports.len(),
            2,
            "TYPE_CHECKING 块内应有 2 个 type import"
        );
        assert!(tc_imports.iter().any(|i| i.module_path == ".models"));
        assert!(tc_imports.iter().any(|i| i.module_path == "os"));

        let val_imports: Vec<_> = result
            .imports
            .iter()
            .filter(|i| i.kind == ImportKind::StaticValue && i.module_path != "typing")
            .collect();
        assert!(val_imports.iter().any(|i| i.module_path == ".utils"));
    }

    #[test]
    fn analyze_function() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
__all__ = ["greet"]

@decorator
def greet(name: str) -> str:
    return f"Hello, {name}"

def helper():
    pass
"#;
        let result = adapter.analyze_file(source, "test.py").unwrap();

        let greet = result
            .nodes
            .iter()
            .find(|n| n.name == "greet")
            .expect("should have greet node");
        assert_eq!(greet.node_type, NodeType::Function);
        assert!(greet.is_exported);
        assert!(
            greet
                .signature
                .as_deref()
                .unwrap()
                .contains("def greet(name: str) -> str"),
            "signature: {:?}",
            greet.signature
        );

        let helper = result
            .nodes
            .iter()
            .find(|n| n.name == "helper")
            .expect("should have helper node");
        assert!(!helper.is_exported);
    }

    #[test]
    fn analyze_class() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
class Animal:
    def speak(self):
        pass

class Dog(Animal):
    def speak(self):
        return "woof"

    def fetch(self):
        pass
"#;
        let result = adapter.analyze_file(source, "test.py").unwrap();

        let dog = result
            .nodes
            .iter()
            .find(|n| n.name == "Dog" && n.node_type == NodeType::Class);
        assert!(dog.is_some(), "should have Dog class");

        let extends_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Extends)
            .collect();
        assert!(!extends_edges.is_empty(), "Dog should extend Animal");

        let contains_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Contains)
            .collect();
        assert!(
            contains_edges.len() >= 3,
            "should have Contains edges for methods: {contains_edges:?}"
        );

        let method_names: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.name.starts_with("Dog."))
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            method_names.contains(&"Dog.speak"),
            "methods: {method_names:?}"
        );
        assert!(
            method_names.contains(&"Dog.fetch"),
            "methods: {method_names:?}"
        );
    }

    #[test]
    fn analyze_calls() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
def main():
    print("hello")
    result = helper()
    obj.method()
    svc = MyService()
"#;
        let result = adapter.analyze_file(source, "test.py").unwrap();

        let callees: Vec<&str> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"print"), "callees: {callees:?}");
        assert!(callees.contains(&"helper"), "callees: {callees:?}");
        assert!(callees.contains(&"obj.method"), "callees: {callees:?}");

        let ctors: Vec<&str> = result
            .calls
            .iter()
            .filter(|c| c.is_constructor)
            .map(|c| c.callee.as_str())
            .collect();
        assert!(ctors.contains(&"MyService"), "constructors: {ctors:?}");
    }

    #[test]
    fn analyze_all_export() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = r#"
__all__ = ["public_func", "PublicClass"]

def public_func():
    pass

def _private():
    pass

class PublicClass:
    pass
"#;
        let result = adapter.analyze_file(source, "test.py").unwrap();

        assert!(result.exported_names.contains("public_func"));
        assert!(result.exported_names.contains("PublicClass"));
        assert!(!result.exported_names.contains("_private"));

        let pub_func = result
            .nodes
            .iter()
            .find(|n| n.name == "public_func")
            .unwrap();
        assert!(pub_func.is_exported);

        let priv_func = result.nodes.iter().find(|n| n.name == "_private").unwrap();
        assert!(!priv_func.is_exported);

        let exports_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Exports)
            .collect();
        assert!(
            exports_edges.len() >= 2,
            "should have Exports edges: {exports_edges:?}"
        );
    }

    #[test]
    fn analyze_instance_type_bindings() {
        let mut adapter = PythonAdapter::new().unwrap();
        let source = "svc = MyService()\ndb = Database()\nx = 42\n";
        let result = adapter.analyze_file(source, "test.py").unwrap();

        assert_eq!(
            result.instance_type_bindings.get("svc"),
            Some(&"MyService".to_string())
        );
        assert_eq!(
            result.instance_type_bindings.get("db"),
            Some(&"Database".to_string())
        );
        assert!(!result.instance_type_bindings.contains_key("x"));
    }

    #[test]
    fn resolve_import_relative() {
        let adapter = PythonAdapter::new().unwrap();
        let files: HashSet<String> = ["pkg/utils.py", "pkg/models/base.py", "pkg/__init__.py"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let exists = |p: &str| files.contains(p);

        // from . import utils → pkg/utils.py
        let result = adapter.resolve_import(".utils", "pkg/main.py", &exists);
        assert_eq!(result, Some("pkg/utils.py".to_string()));

        // from ..models import base → pkg/models/base.py
        let result = adapter.resolve_import("..models.base", "pkg/sub/main.py", &exists);
        assert_eq!(result, Some("pkg/models/base.py".to_string()));

        // from . import (package) → pkg/__init__.py
        let result = adapter.resolve_import(".", "pkg/main.py", &exists);
        assert_eq!(result, Some("pkg/__init__.py".to_string()));
    }

    #[test]
    fn resolve_import_absolute() {
        let adapter = PythonAdapter::new().unwrap();
        let files: HashSet<String> = ["mypackage/module.py", "mypackage/__init__.py"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let exists = |p: &str| files.contains(p);

        let result = adapter.resolve_import("mypackage.module", "other.py", &exists);
        assert_eq!(result, Some("mypackage/module.py".to_string()));

        let result = adapter.resolve_import("mypackage", "other.py", &exists);
        assert_eq!(result, Some("mypackage/__init__.py".to_string()));
    }

    #[test]
    fn resolve_import_external() {
        let adapter = PythonAdapter::new().unwrap();
        let exists = |_: &str| false;

        assert_eq!(adapter.resolve_import("os", "test.py", &exists), None);
        assert_eq!(
            adapter.resolve_import("numpy.array", "test.py", &exists),
            None
        );
    }

    // === classify_file（M3-DEC-01 机械/危险分类）===

    fn classify(src: &str) -> FileClassification {
        PythonAdapter::new().unwrap().classify_file(src)
    }

    #[test]
    fn classify_barrel_init_is_mechanical() {
        // 典型 __init__.py：纯 re-export + __all__ → Barrel（机械）。
        let c = classify(
            "from .base import Base\nfrom .impl import Impl\n__all__ = ['Base', 'Impl']\n",
        );
        assert_eq!(c.file_kind, FileKind::Barrel);
        assert!(c.danger.is_empty());
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_pure_type_alias_is_mechanical() {
        let c = classify("from typing import TypeVar\nVector = list[float]\nT = TypeVar('T')\n");
        assert_eq!(c.file_kind, FileKind::PureType);
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_annotation_only_class_is_pure_type() {
        // 仅注解无方法的 class（TypedDict/Protocol/纯 dataclass）→ PureType。
        let c = classify("class Point:\n    x: int\n    y: int\n");
        assert_eq!(c.file_kind, FileKind::PureType);
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_pure_constant_is_mechanical() {
        let c = classify("MAX = 100\nNAME = 'x'\nPI = 3.14\nFLAGS = (1, 2, 3)\n");
        assert_eq!(c.file_kind, FileKind::PureConstant);
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_function_def_is_normal() {
        let c = classify("def inc(x):\n    return x + 1\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_class_with_method_is_normal() {
        let c = classify("class Svc:\n    def run(self):\n        return 1\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_plain_if_is_not_danger() {
        // 锚点（decomposition-redesign §8）：10 行带 if 不因 if 进重型——
        // if 使文件 Normal（非机械），但**不**进 danger。
        let c = classify("def pick(x):\n    if x > 0:\n        return x\n    return 0\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(
            c.danger.is_empty(),
            "纯 if 控制流不应命中危险信号: {:?}",
            c.danger
        );
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_math_hits_numeric_precision() {
        // 锚点：带 math/浮点的小函数命中数值精度危险信号。
        let c = classify("import math\ndef dist(a, b):\n    return math.sqrt(a * a + b * b)\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(c.danger.contains(&DangerCategory::NumericPrecision));
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_python_clamp_min_max_is_not_numeric_danger() {
        // 对照锚点：Python clamp 用内建 min/max（精确，无精度风险）→ 不命中数值危险。
        let c = classify("def clamp(v, lo, hi):\n    return min(max(v, lo), hi)\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(
            !c.danger.contains(&DangerCategory::NumericPrecision),
            "min/max clamp 不应命中数值精度: {:?}",
            c.danger
        );
    }

    #[test]
    fn classify_async_hits_concurrency() {
        let c = classify("async def load():\n    await go()\n");
        assert!(c.danger.contains(&DangerCategory::Concurrency));
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_aliased_import_hits_danger() {
        // 别名导入 `import x as y` 不得漏判危险（专项审查 important）。
        let c = classify("import multiprocessing as mp\ndef f():\n    return mp.Pool()\n");
        assert!(
            c.danger.contains(&DangerCategory::Concurrency),
            "import as 别名应命中并发: {:?}",
            c.danger
        );
    }

    #[test]
    fn classify_multi_import_hits_all_dangers() {
        // 同行多导入 `import a, b` 每个都要分类（专项审查 important）。
        let c = classify("import json, threading\ndef f():\n    pass\n");
        assert!(
            c.danger.contains(&DangerCategory::Concurrency),
            "多导入第二项 threading 应命中并发: {:?}",
            c.danger
        );
    }

    #[test]
    fn classify_aliased_math_import_hits_numeric() {
        // `import math as m` / `from math import sqrt`：调用文本无 `math.` 前缀，靠 import 级识别（finder#1）。
        let c1 = classify("import math as m\ndef f(x):\n    return m.sqrt(x)\n");
        assert!(
            c1.danger.contains(&DangerCategory::NumericPrecision),
            "import math as m 应命中数值: {:?}",
            c1.danger
        );
        let c2 = classify("from math import sqrt\ndef f(x):\n    return sqrt(x)\n");
        assert!(c2.danger.contains(&DangerCategory::NumericPrecision));
    }

    #[test]
    fn classify_fstring_with_call_is_not_pure_constant() {
        // f-string 含副作用内插（`f"{compute()}"`）→ 非不可变常量、非机械（finder#1）。
        let c = classify("URL = f\"{compute()}/api\"\n");
        assert_ne!(c.file_kind, FileKind::PureConstant);
        assert!(
            !c.is_mechanical(),
            "含副作用内插的 f-string 不应判机械: {c:?}"
        );
    }

    #[test]
    fn classify_plain_string_constant_still_pure() {
        // 普通字符串常量（无内插）仍为不可变 → PureConstant。
        let c = classify("NAME = \"hello\"\nGREETING = 'hi'\n");
        assert_eq!(c.file_kind, FileKind::PureConstant);
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_class_with_call_assignment_is_normal() {
        // 无方法但类体含副作用调用赋值 → 非纯类型（专项审查 nit）。
        let c = classify("class C:\n    value = compute()\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(!c.is_mechanical(), "类体副作用赋值不应判机械");
    }

    #[test]
    fn classify_module_mutable_dict_is_shared_global_normal() {
        // 模块级可变容器 = 共享可变全局 → Normal（非机械）。
        let c = classify("_cache = {}\n");
        assert_eq!(c.file_kind, FileKind::Normal);
        assert!(c.danger.contains(&DangerCategory::SharedMutableGlobal));
        assert!(!c.is_mechanical());
    }

    #[test]
    fn classify_eval_hits_dynamic_reflection() {
        let c = classify("def run(s):\n    return eval(s)\n");
        assert!(c.danger.contains(&DangerCategory::DynamicReflection));
    }

    #[test]
    fn classify_ctypes_import_hits_ffi() {
        let c = classify("import ctypes\nlib = ctypes.CDLL('libm.so')\n");
        assert!(c.danger.contains(&DangerCategory::Ffi));
    }

    #[test]
    fn classify_io_import_hits_io_side_effect() {
        let c = classify("import os\ndef p():\n    return os.getcwd()\n");
        assert!(c.danger.contains(&DangerCategory::IoSideEffect));
    }

    #[test]
    fn classify_type_checking_guard_keeps_barrel() {
        // `if TYPE_CHECKING:` 是纯类型导入守卫，不使文件变 Normal（保持机械资格）。
        let c = classify(
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from .x import Y\n__all__ = []\n",
        );
        assert!(
            matches!(c.file_kind, FileKind::Barrel | FileKind::PureType),
            "TYPE_CHECKING 守卫不应令文件变 Normal: {:?}",
            c.file_kind
        );
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_metaclass_hits_dynamic_reflection() {
        let c = classify("class Meta(type):\n    pass\nclass A(metaclass=Meta):\n    x: int\n");
        assert!(c.danger.contains(&DangerCategory::DynamicReflection));
    }

    #[test]
    fn classify_dunder_all_does_not_make_normal() {
        // __all__（list 字面量）是导出元数据，不应判为可变全局/Normal。
        let c = classify("from .a import A\n__all__ = ['A']\n");
        assert_eq!(c.file_kind, FileKind::Barrel);
        assert!(c.is_mechanical());
    }

    #[test]
    fn classify_unparseable_is_conservative() {
        let c = classify("def broken( :::\n");
        assert_eq!(c, FileClassification::conservative());
    }

    // === detect_source_root 测试 ===

    fn write_file(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn source_root_flat_package() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "mypackage/__init__.py", "");
        write_file(tmp.path(), "mypackage/core.py", "x = 1");
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_source_root(tmp.path()),
            Some("mypackage".to_string())
        );
    }

    #[test]
    fn source_root_src_layout() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "src/mypackage/__init__.py", "");
        write_file(tmp.path(), "src/mypackage/core.py", "x = 1");
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_source_root(tmp.path()),
            Some("src".to_string())
        );
    }

    #[test]
    fn source_root_multiple_packages_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "pkg_a/__init__.py", "");
        write_file(tmp.path(), "pkg_a/a.py", "x = 1");
        write_file(tmp.path(), "pkg_b/__init__.py", "");
        write_file(tmp.path(), "pkg_b/b.py", "y = 2");
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(adapter.detect_source_root(tmp.path()), None);
    }

    #[test]
    fn source_root_no_init_py_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "scripts/run.py", "print('hello')");
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(adapter.detect_source_root(tmp.path()), None);
    }

    #[test]
    fn source_root_with_tests_package_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "mypackage/__init__.py", "");
        write_file(tmp.path(), "mypackage/core.py", "x = 1");
        write_file(tmp.path(), "tests/__init__.py", "");
        write_file(tmp.path(), "tests/test_core.py", "def test_x(): pass");
        let adapter = PythonAdapter::new().unwrap();
        // 两个含 __init__.py 的顶层目录 → 多包，保守回退 None
        assert_eq!(adapter.detect_source_root(tmp.path()), None);
    }

    #[test]
    fn source_root_excludes_venv() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), ".venv/__init__.py", "");
        write_file(tmp.path(), ".venv/lib.py", "x = 1");
        write_file(tmp.path(), "main.py", "print('hello')");
        let adapter = PythonAdapter::new().unwrap();
        assert_eq!(adapter.detect_source_root(tmp.path()), None);
    }
}
