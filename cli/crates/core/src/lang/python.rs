//! Python 语言适配器。
//!
//! 基于 tree-sitter-python 的单文件分析。M3 Sprint B 逐步实现：
//! - PY-01: 结构体 + language/can_handle/detect_tier
//! - PY-02~06: analyze_file / resolve_import

use std::collections::{HashMap, HashSet};
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::{normalize_path, NodeId, SourceLang, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};
use crate::types::state::ModuleTier;

use super::{
    CallInfo, FileAnalysis, ImportInfo, ImportKind, ImportedSymbol, LanguageAdapter, SymbolKind,
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

    ctx.imports.push(ImportInfo {
        module_path,
        symbols,
        kind: final_kind,
        reexport: false,
    });
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

    // 遍历 else 分支（alternative）中的非 TYPE_CHECKING 部分
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "else_clause" || child.kind() == "elif_clause" {
            walk_py_ast(child, ctx, already_in);
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
        // 将 decorators 拼入 signature 头部（为了保留信息但不修改 signature 字段语义，
        // 我们不覆盖 signature——保持与 TS adapter 一致：signature 只含声明头）
        // 但 decorator 信息通过节点 line_range 覆盖整个 decorated 范围
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
    let sig = source.get(start..end).unwrap_or("").trim_end();
    sig.strip_suffix(':').unwrap_or(sig).trim_end().to_string()
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
    _decorators: &[String],
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
    ctx.nodes.push(sn);

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

        assert_eq!(result.imports.len(), 3);

        assert_eq!(result.imports[0].module_path, "os");
        assert_eq!(result.imports[0].symbols[0].name, "path");

        assert_eq!(result.imports[1].module_path, ".");
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
}
