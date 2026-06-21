//! TypeScript 语言适配器。
//!
//! 基于 tree-sitter-typescript 的单文件完整分析：
//! 解析 AST → 提取节点（File/Function/Class/Interface/Enum）
//! + 边（Contains/Extends/Exports）+ 导入/调用信息。
//!
//! 本文件按 `node.kind()`（节点类型名）和 `child_by_field_name(...)`（命名字段）
//! 遍历 AST，这些字符串硬编码、无编译期检查。关键解析点配有「TS 片段 → AST 骨架」
//! 注释帮助阅读；骨架中 `field:` 前缀表示命名字段、其余为匿名/位置子节点。
//! 这些 kind/field 的存在性由 `tests/ast_contract.rs` 在 grammar 升级时兜底校验
//! （示例基于 tree-sitter-typescript 0.21.x，AST 形状随 grammar 版本可能演进，
//! 以 node-types.json 与该测试为准）。

use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::{NodeId, SourceLang, Span};
use crate::types::graph::{Dependency, EdgeSubKind, EdgeType, NodeType, SourceNode};

use super::{
    CallInfo, FileAnalysis, ImportInfo, ImportKind, ImportedSymbol, LanguageAdapter, SymbolKind,
};

/// TypeScript 语言适配器（基于 tree-sitter-typescript）。
pub struct TypeScriptAdapter {
    parser: Parser,
}

impl TypeScriptAdapter {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .map_err(|e| {
                MigrateError::Config(format!("tree-sitter TypeScript 语法加载失败: {e}"))
            })?;
        Ok(Self { parser })
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn language(&self) -> SourceLang {
        SourceLang::TypeScript
    }

    fn can_handle(&self, path: &Path) -> bool {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // 排除 .d.ts（类型声明）与 .test/.spec（测试文件）——均非迁移源：测试随源码以
        // Rust `#[cfg(test)]`/`tests/` 重写（设计 09 §507），不作为独立迁移单元；设计 06
        // 的 exclude 默认亦含 `src/**/*.test.ts`。
        // TODO: 用户自定义 exclude glob（ProjectConfig.exclude）尚未在 collect_source_files
        // 应用，excluded_imports 输出（设计 04 §5.7.4）亦未实现 —— 独立任务。
        (name.ends_with(".ts") || name.ends_with(".tsx"))
            && !name.ends_with(".d.ts")
            && !name.ends_with(".test.ts")
            && !name.ends_with(".test.tsx")
            && !name.ends_with(".spec.ts")
            && !name.ends_with(".spec.tsx")
    }

    fn resolve_extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    fn analyze_file(&mut self, source: &str, rel_path: &str) -> Result<FileAnalysis> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| MigrateError::Parse {
                path: rel_path.into(),
            })?;

        let mut ctx = AnalysisContext {
            rel_path,
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
            exported_names: std::collections::HashSet::new(),
            constructor_bindings: std::collections::HashMap::new(),
        };

        // 第一遍：收集 export 名称
        collect_exports(tree.root_node(), source, &mut ctx.exported_names);

        // 添加 File 节点
        ctx.nodes.push(SourceNode::new(
            NodeId::file(rel_path),
            NodeType::File,
            rel_path.to_string(),
            rel_path.to_string(),
        ));

        // 第二遍：提取符号节点、边、导入、调用
        walk_ast(tree.root_node(), &mut ctx, true);

        // 生成 Exports 边
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
            constructor_bindings: ctx.constructor_bindings,
        })
    }

    fn detect_tier(&mut self, source: &str) -> crate::types::state::ModuleTier {
        use crate::types::state::ModuleTier;
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
struct TsTierSignals {
    has_danger: bool,
    has_non_trivial_content: bool,
}

fn scan_tier_signals(root: Node, source: &str) -> TsTierSignals {
    let mut s = TsTierSignals::default();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                if has_non_trivial_declaration(child, source) {
                    s.has_non_trivial_content = true;
                }
                scan_subtree_signals(child, source, &mut s);
            }
            "import_statement" => {
                if is_io_import(child, source) {
                    s.has_danger = true;
                }
            }
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {}
            "function_declaration" | "class_declaration" | "abstract_class_declaration" => {
                s.has_non_trivial_content = true;
                scan_subtree_signals(child, source, &mut s);
            }
            "lexical_declaration" | "variable_declaration" => {
                if ts_is_mutable_or_complex_var(child, source) {
                    s.has_non_trivial_content = true;
                    let text = ts_node_text(child, source);
                    if text.starts_with("let ") || text.starts_with("var ") {
                        s.has_danger = true;
                    }
                }
                scan_subtree_signals(child, source, &mut s);
            }
            "expression_statement" => {
                s.has_non_trivial_content = true;
                s.has_danger = true;
            }
            "try_statement" | "if_statement" | "for_statement" | "for_in_statement"
            | "while_statement" | "do_statement" | "switch_statement" => {
                s.has_non_trivial_content = true;
                s.has_danger = true;
            }
            _ => {}
        }
    }
    s
}

fn scan_subtree_signals(node: Node, source: &str, s: &mut TsTierSignals) {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "await_expression" => s.has_danger = true,
            "try_statement" | "throw_statement" => s.has_danger = true,
            "call_expression" => {
                if let Some(func) = current.child_by_field_name("function") {
                    let text = ts_node_text(func, source);
                    if matches!(
                        text,
                        "Promise.all"
                            | "Promise.race"
                            | "Promise.allSettled"
                            | "Promise.any"
                            | "setTimeout"
                            | "setInterval"
                            | "setImmediate"
                            | "process.nextTick"
                            | "queueMicrotask"
                    ) {
                        s.has_danger = true;
                    }
                    if text.starts_with("Math.")
                        || matches!(
                            text,
                            "parseFloat"
                                | "parseInt"
                                | "Number"
                                | "Number.isNaN"
                                | "Number.isFinite"
                        )
                    {
                        s.has_danger = true;
                    }
                }
            }
            "import_statement" => {
                if is_io_import(current, source) {
                    s.has_danger = true;
                }
            }
            "conditional_type" => s.has_danger = true,
            "predefined_type" => {
                let text = ts_node_text(current, source);
                if text == "never" || text == "unknown" || text == "any" {
                    s.has_danger = true;
                }
            }
            "typeof_expression" | "instanceof_expression" => s.has_danger = true,
            "as_expression" => {
                let text = ts_node_text(current, source);
                if text.ends_with("as any") || text.ends_with("as unknown") {
                    s.has_danger = true;
                }
            }
            "function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression" => {
                if current.child_by_field_name("async").is_some()
                    || ts_node_text(current, source).starts_with("async ")
                    || ts_node_text(current, source).starts_with("async\n")
                {
                    s.has_danger = true;
                }
            }
            "identifier" => {
                let text = ts_node_text(current, source);
                if text == "NaN" || text == "Infinity" {
                    s.has_danger = true;
                }
            }
            _ => {}
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn has_non_trivial_declaration(export_node: Node, source: &str) -> bool {
    let mut cursor = export_node.walk();
    for child in export_node.children(&mut cursor) {
        match child.kind() {
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {}
            "function_declaration" | "class_declaration" | "abstract_class_declaration" => {
                return true;
            }
            "lexical_declaration" | "variable_declaration" => {
                if ts_is_mutable_or_complex_var(child, source) {
                    return true;
                }
            }
            "export_clause" | "namespace_export" => {}
            "identifier" | "string" | "number" | "true" | "false" | "null" => {}
            _ => {}
        }
    }
    false
}

fn ts_is_mutable_or_complex_var(decl_node: Node, source: &str) -> bool {
    let text = ts_node_text(decl_node, source);
    if text.starts_with("let ") || text.starts_with("var ") {
        return true;
    }
    ts_contains_kind(
        decl_node,
        &["arrow_function", "function", "function_expression"],
    ) || ts_contains_kind(decl_node, &["call_expression", "new_expression"])
}

fn ts_contains_kind(node: Node, kinds: &[&str]) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if kinds.contains(&current.kind()) {
            return true;
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn is_io_import(node: Node, source: &str) -> bool {
    let text = ts_node_text(node, source);
    const IO_MODULES: &[&str] = &[
        "\"fs\"",
        "'fs'",
        "\"fs/promises\"",
        "'fs/promises'",
        "\"path\"",
        "'path'",
        "\"http\"",
        "'http'",
        "\"https\"",
        "'https'",
        "\"net\"",
        "'net'",
        "\"child_process\"",
        "'child_process'",
        "\"os\"",
        "'os'",
        "\"stream\"",
        "'stream'",
        "\"dgram\"",
        "'dgram'",
        "\"tty\"",
        "'tty'",
        "\"cluster\"",
        "'cluster'",
        "\"worker_threads\"",
        "'worker_threads'",
    ];
    IO_MODULES.iter().any(|m| text.contains(m))
}

fn ts_node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

struct AnalysisContext<'a> {
    rel_path: &'a str,
    source: &'a str,
    nodes: Vec<SourceNode>,
    edges: Vec<Dependency>,
    imports: Vec<ImportInfo>,
    calls: Vec<CallInfo>,
    exported_names: std::collections::HashSet<String>,
    /// 本地构造绑定（`const x = new Foo()` → `"x" → "Foo"`）。
    constructor_bindings: std::collections::HashMap<String, String>,
}

// === Export 收集 ===

/// 第一遍：收集所有 export 名称（供后续判定符号 is_exported）。
///
/// export 的多种形态对应不同 AST 结构：
///   export function f(){}      → export_statement declaration:(function_declaration …)
///   export default x           → export_statement + 匿名子节点 "default", value:(identifier)
///   export { a as b }          → export_statement (export_clause (export_specifier name: alias:))
///   export * from "m"          → export_statement + 匿名子节点 "*", source:(string)
///   export * as ns from "m"    → export_statement (namespace_export (identifier)) source:(string)
fn collect_exports(node: Node, source: &str, exports: &mut std::collections::HashSet<String>) {
    if node.kind() == "export_statement" {
        if let Some(decl) = node.child_by_field_name("declaration") {
            for name in declaration_names(decl, source) {
                exports.insert(name);
            }
        }
        let is_default = has_anon_child(node, "default");
        if is_default {
            exports.insert("default".to_string());
        }
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "export_clause" => {
                        let cc = child.child_count();
                        for j in 0..cc {
                            if let Some(spec) = child.child(j) {
                                if spec.kind() == "export_specifier" {
                                    let name = spec
                                        .child_by_field_name("alias")
                                        .or_else(|| spec.child_by_field_name("name"))
                                        .map(|n| text(n, source));
                                    if let Some(n) = name {
                                        exports.insert(n);
                                    }
                                }
                            }
                        }
                    }
                    "*" => {
                        let src = get_source_string(node, source);
                        let label = match src {
                            Some(s) => format!("*<-{s}"),
                            None => "*".to_string(),
                        };
                        exports.insert(label);
                    }
                    "namespace_export" => {
                        let nc = child.child_count();
                        for j in 0..nc {
                            if let Some(id) = child.child(j) {
                                if id.kind() == "identifier" {
                                    exports.insert(text(id, source));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            collect_exports(child, source, exports);
        }
    }
}

fn declaration_names(decl: Node, source: &str) -> Vec<String> {
    match decl.kind() {
        "function_declaration"
        | "generator_function_declaration"
        | "class_declaration"
        | "abstract_class_declaration"
        | "interface_declaration"
        | "type_alias_declaration"
        | "enum_declaration" => decl
            .child_by_field_name("name")
            .map(|n| vec![text(n, source)])
            .unwrap_or_default(),
        "lexical_declaration" | "variable_declaration" => {
            let mut names = Vec::new();
            let count = decl.child_count();
            for i in 0..count {
                if let Some(child) = decl.child(i) {
                    if child.kind() == "variable_declarator" {
                        if let Some(n) = child.child_by_field_name("name") {
                            names.push(text(n, source));
                        }
                    }
                }
            }
            names
        }
        _ => Vec::new(),
    }
}

// === AST 遍历 ===

/// `top_level` = true：文件顶层遍历，提取所有结构（函数/变量绑定/import/export）。
/// `top_level` = false：class body 内部遍历，跳过 lexical_declaration（避免方法体内的
/// 局部函数/构造绑定泄漏到文件级）、function_declaration（局部函数）、
/// import/export（方法体内不合法）。
fn walk_ast(node: Node, ctx: &mut AnalysisContext, top_level: bool) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" if top_level => {
            extract_function(node, ctx);
        }
        "class_declaration" | "abstract_class_declaration" => {
            extract_class(node, ctx);
            return;
        }
        "interface_declaration" => {
            extract_interface(node, ctx);
        }
        "enum_declaration" => {
            extract_enum(node, ctx);
        }
        "lexical_declaration" | "variable_declaration" if top_level => {
            process_var_declaration(node, ctx);
        }
        "export_statement" if top_level => {
            if let Some(decl) = node.child_by_field_name("declaration") {
                walk_ast(decl, ctx, true);
                return;
            }
            if let Some(module_path) = get_source_string(node, ctx.source) {
                let is_type_only = has_anon_child(node, "type");
                let mut symbols = Vec::new();
                let count = node.child_count();
                for i in 0..count {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "export_clause" {
                            let cc = child.child_count();
                            for j in 0..cc {
                                if let Some(spec) = child.child(j) {
                                    if spec.kind() == "export_specifier" {
                                        let name = spec
                                            .child_by_field_name("name")
                                            .map(|n| text(n, ctx.source))
                                            .unwrap_or_default();
                                        let alias = spec
                                            .child_by_field_name("alias")
                                            .map(|n| text(n, ctx.source));
                                        symbols.push(ImportedSymbol {
                                            name,
                                            alias,
                                            kind: SymbolKind::Named,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                ctx.imports.push(ImportInfo {
                    module_path,
                    symbols,
                    kind: if is_type_only {
                        ImportKind::StaticType
                    } else {
                        ImportKind::StaticValue
                    },
                });
            }
        }
        "import_statement" if top_level => {
            extract_import(node, ctx);
        }
        "call_expression" => {
            extract_call(node, ctx);
        }
        "new_expression" => {
            extract_new(node, ctx);
        }
        _ => {}
    }

    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            walk_ast(child, ctx, top_level);
        }
    }
}

fn extract_function(node: Node, ctx: &mut AnalysisContext) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = text(name_node, ctx.source);
        let is_async = has_anon_child(node, "async");
        let is_exported = ctx.exported_names.contains(&name);
        let mut sn = SourceNode::new(
            NodeId::symbol(NodeType::Function, ctx.rel_path, &name),
            NodeType::Function,
            name,
            ctx.rel_path.to_string(),
        );
        sn.line_range = Some(node_span(node));
        sn.is_exported = is_exported;
        sn.is_async = is_async;
        ctx.nodes.push(sn);
    }
}

/// 判断一个值节点是否为函数表达式（箭头/函数/生成器）。
fn is_function_value(kind: &str) -> bool {
    matches!(
        kind,
        "arrow_function" | "function" | "function_expression" | "generator_function"
    )
}

/// 从 `lexical_declaration`/`variable_declaration` 中提取：
/// 1. 函数表达式绑定（`const f = () => {}`）→ Function 节点
/// 2. 构造绑定（`const x = new Foo()`）→ constructor_bindings 映射
fn process_var_declaration(node: Node, ctx: &mut AnalysisContext) {
    let count = node.child_count();
    for i in 0..count {
        let Some(declarator) = node.child(i) else {
            continue;
        };
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        let Some(value) = declarator.child_by_field_name("value") else {
            continue;
        };

        // 1. 函数表达式绑定 → Function 节点
        if is_function_value(value.kind()) {
            let name = text(name_node, ctx.source);
            if name.is_empty() {
                continue;
            }
            let is_async = has_anon_child(value, "async");
            let is_exported = ctx.exported_names.contains(&name);
            let mut sn = SourceNode::new(
                NodeId::symbol(NodeType::Function, ctx.rel_path, &name),
                NodeType::Function,
                name,
                ctx.rel_path.to_string(),
            );
            sn.line_range = Some(node_span(declarator));
            sn.is_exported = is_exported;
            sn.is_async = is_async;
            ctx.nodes.push(sn);
            continue;
        }

        // 2. 构造绑定：`const x = new Foo()` / `const x = await new Foo()`
        let new_expr = if value.kind() == "await_expression" {
            (0..value.child_count())
                .find_map(|j| value.child(j).filter(|c| c.kind() == "new_expression"))
        } else if value.kind() == "new_expression" {
            Some(value)
        } else {
            None
        };
        if let Some(new_expr) = new_expr {
            if let Some(constructor) = new_expr.child_by_field_name("constructor") {
                let class_name = text(constructor, ctx.source);
                if !class_name.is_empty() {
                    let var_name = text(name_node, ctx.source);
                    ctx.constructor_bindings.insert(var_name, class_name);
                }
            }
        }
    }
}

fn extract_class(node: Node, ctx: &mut AnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = text(name_node, ctx.source);
    let is_exported = ctx.exported_names.contains(&name);
    let is_abstract = node.kind() == "abstract_class_declaration";
    let class_id = NodeId::symbol(NodeType::Class, ctx.rel_path, &name);

    let mut sn = SourceNode::new(
        class_id.clone(),
        NodeType::Class,
        name.clone(),
        ctx.rel_path.to_string(),
    );
    sn.line_range = Some(node_span(node));
    sn.is_exported = is_exported;
    sn.is_abstract = is_abstract;
    ctx.nodes.push(sn);

    // extends / implements
    extract_heritage(node, class_id.as_str(), ctx);

    if let Some(body) = node.child_by_field_name("body") {
        extract_methods(body, class_id.as_str(), &name, ctx);
        let count = body.child_count();
        for i in 0..count {
            if let Some(child) = body.child(i) {
                walk_ast(child, ctx, false);
            }
        }
    }
}

/// 从 class body 提取方法为 Function 节点 + Contains 边。
///
/// `class C { method() {} handler = () => {}; data = 0 }` 的 class_body：
///   method_definition       name:(property_identifier)            // 总是方法
///   public_field_definition name: value:(arrow_function …)        // 字段值是函数 → 算方法
///   public_field_definition name: value:(number)                  // 纯数据字段 → 跳过
/// 故 public_field_definition 仅当其 value 为函数表达式时才生成 Function 节点。
fn extract_methods(body: Node, class_id: &str, class_name: &str, ctx: &mut AnalysisContext) {
    let count = body.child_count();
    for i in 0..count {
        let Some(child) = body.child(i) else {
            continue;
        };
        // method_definition 始终是方法；public_field_definition 仅当其值为函数
        // 表达式（如 `handler = () => {}`）时才算方法——纯数据字段（`count = 0`）
        // 不是函数，不应生成 Function 节点
        let is_method = child.kind() == "method_definition";
        let field_value = if child.kind() == "public_field_definition" {
            child.child_by_field_name("value")
        } else {
            None
        };
        let field_is_fn = field_value
            .map(|v| is_function_value(v.kind()))
            .unwrap_or(false);
        if !is_method && !field_is_fn {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let method_name = text(name_node, ctx.source);
        // 字段函数的 async 关键字位于值节点（箭头函数）内部，而非字段定义的直接子节点
        let is_async = match field_value {
            Some(v) => has_anon_child(v, "async"),
            None => has_anon_child(child, "async"),
        };
        let method_id = NodeId::symbol(
            NodeType::Function,
            ctx.rel_path,
            &format!("{class_name}.{method_name}"),
        );

        let mut sn = SourceNode::new(
            method_id.clone(),
            NodeType::Function,
            format!("{class_name}.{method_name}"),
            ctx.rel_path.to_string(),
        );
        sn.line_range = Some(node_span(child));
        sn.is_async = is_async;
        ctx.nodes.push(sn);

        ctx.edges.push(Dependency::new(
            NodeId::new(class_id),
            method_id,
            EdgeType::Contains,
        ));
    }
}

/// 取父类型节点的基础标识符名（剥离泛型参数与命名空间限定）。
/// `Bar` → "Bar"；`Bar<T>` → "Bar"；`ns.Base` → "Base"；`ns.Base<T>` → "Base"。
fn heritage_target_name(type_node: Node, source: &str) -> String {
    match type_node.kind() {
        // generic_type 与 nested_type_identifier 都有 `name` 字段
        "generic_type" => type_node
            .child_by_field_name("name")
            .map(|n| heritage_target_name(n, source))
            .unwrap_or_else(|| text(type_node, source)),
        "nested_type_identifier" => type_node
            .child_by_field_name("name")
            .map(|n| text(n, source))
            .unwrap_or_else(|| text(type_node, source)),
        _ => text(type_node, source),
    }
}

/// 提取 class 的 extends/implements 关系为 Extends 边（implements 以 sub_kind 区分）。
///
/// `class Foo extends Bar<T> implements Baz, ns.Q {}` 的 AST：
///   class_declaration
///     name: (type_identifier)                  // Foo
///     class_heritage
///       extends_clause
///         value: (identifier)                  // Bar（extends 处于表达式位置）
///         type_arguments: (type_arguments …)   // <T>
///       implements_clause                      // 无命名字段，类型节点为直接子节点
///         (type_identifier)                    // Baz
///         (nested_type_identifier              // ns.Q
///            module:(identifier) name:(type_identifier))
///     body: (class_body)
/// 故需三层下钻：class_heritage → extends/implements_clause → 类型节点，
/// 再由 heritage_target_name 归一化类型名。
fn extract_heritage(class_node: Node, class_id: &str, ctx: &mut AnalysisContext) {
    let count = class_node.child_count();
    for i in 0..count {
        let Some(child) = class_node.child(i) else {
            continue;
        };
        if child.kind() != "class_heritage" {
            continue;
        }
        let cc = child.child_count();
        for j in 0..cc {
            let Some(clause) = child.child(j) else {
                continue;
            };
            let is_implements = clause.kind() == "implements_clause";
            if clause.kind() != "extends_clause" && !is_implements {
                continue;
            }
            let tc = clause.child_count();
            for k in 0..tc {
                let Some(type_node) = clause.child(k) else {
                    continue;
                };
                // 跳过非父类型的子节点：extends/implements 关键字本身，以及
                // extends_clause 的 `type_arguments`（如 `extends Bar<T>` 的 `<T>`）。
                // type_arguments 是命名节点但不是父类型，若不跳过会被
                // heritage_target_name 取文本、生成一条 target 名为 "<T>" 的噪声
                // Extends 边（匹配不到真实节点）。
                if !type_node.is_named()
                    || type_node.kind() == "extends"
                    || type_node.kind() == "implements"
                    || type_node.kind() == "type_arguments"
                {
                    continue;
                }
                // 归一化父类型名：剥离泛型参数与命名空间限定，取基础类型标识符，
                // 否则 `extends Bar<T>` / `extends ns.Base` 的目标名为 "Bar<T>"/"ns.Base"，
                // 永远匹配不到真实节点（名为 "Bar"/"Base"），Extends 边会被丢弃
                let target_name = heritage_target_name(type_node, ctx.source);
                if target_name.is_empty() {
                    continue;
                }
                // 目标 ID 暂用 interface 前缀，build.rs 在图完成后会修正
                let target_id = NodeId::symbol(NodeType::Interface, ctx.rel_path, &target_name);
                let mut dep = Dependency::new(NodeId::new(class_id), target_id, EdgeType::Extends);
                if is_implements {
                    dep = dep.with_sub_kind(EdgeSubKind::Implements);
                }
                ctx.edges.push(dep);
            }
        }
    }
}

fn extract_interface(node: Node, ctx: &mut AnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = text(name_node, ctx.source);
    let is_exported = ctx.exported_names.contains(&name);
    let interface_id = NodeId::symbol(NodeType::Interface, ctx.rel_path, &name);
    let mut sn = SourceNode::new(
        interface_id.clone(),
        NodeType::Interface,
        name,
        ctx.rel_path.to_string(),
    );
    sn.line_range = Some(node_span(node));
    sn.is_exported = is_exported;
    ctx.nodes.push(sn);

    // `interface Foo extends Bar` → Extends 边。interface 的继承节点是
    // `extends_type_clause`（区别于 class 的 `class_heritage > extends_clause`），
    // 故单独提取，否则 interface extends interface 全部漏报（函数式/类型密集库重灾区）。
    extract_interface_heritage(node, interface_id.as_str(), ctx);
}

/// 提取 interface 的 extends 关系为 Extends 边。
///
/// `interface Foo extends Bar, Baz<T> {}` 的 AST：
///   interface_declaration
///     name: (type_identifier)                       // Foo
///     extends_type_clause                           // 'extends' commaSep1(type)
///       (type_identifier)                           // Bar
///       (generic_type name:(type_identifier) …)     // Baz<T>
///     body: (interface_body)
/// 目标父接口先以占位 Interface ID 记录，跨文件由 fixup_extends_in_edges 按名称唯一匹配修正。
fn extract_interface_heritage(node: Node, interface_id: &str, ctx: &mut AnalysisContext) {
    let count = node.child_count();
    for i in 0..count {
        let Some(clause) = node.child(i) else {
            continue;
        };
        if clause.kind() != "extends_type_clause" {
            continue;
        }
        let tc = clause.child_count();
        for k in 0..tc {
            let Some(type_node) = clause.child(k) else {
                continue;
            };
            // 跳过 `extends` 关键字、逗号等非父类型节点；type_arguments 在
            // generic_type 内部，由 heritage_target_name 归一化剥离。
            if !type_node.is_named()
                || type_node.kind() == "extends"
                || type_node.kind() == "type_arguments"
            {
                continue;
            }
            let target_name = heritage_target_name(type_node, ctx.source);
            if target_name.is_empty() {
                continue;
            }
            ctx.edges.push(Dependency::new(
                NodeId::new(interface_id),
                NodeId::symbol(NodeType::Interface, ctx.rel_path, &target_name),
                EdgeType::Extends,
            ));
        }
    }
}

fn extract_enum(node: Node, ctx: &mut AnalysisContext) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = text(name_node, ctx.source);
        let is_exported = ctx.exported_names.contains(&name);
        let mut sn = SourceNode::new(
            NodeId::symbol(NodeType::Enum, ctx.rel_path, &name),
            NodeType::Enum,
            name,
            ctx.rel_path.to_string(),
        );
        sn.line_range = Some(node_span(node));
        sn.is_exported = is_exported;
        ctx.nodes.push(sn);
    }
}

// === Import 提取 ===

fn extract_import(node: Node, ctx: &mut AnalysisContext) {
    let module_path = match get_source_string(node, ctx.source) {
        Some(s) => s,
        None => return,
    };

    let is_type_only = has_anon_child(node, "type")
        && node.child_count() > 0
        && (0..node.child_count()).any(|i| {
            node.child(i)
                .map(|c| c.kind() == "type" && text(c, ctx.source) == "type")
                .unwrap_or(false)
        });

    let mut symbols = Vec::new();
    let mut has_clause = false;

    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "import_clause" {
            has_clause = true;
            extract_import_clause(child, ctx.source, &mut symbols);
        }
    }

    // 无 import_clause 即 side-effect 导入（`import 'x'`）；type-only 必带 clause，
    // 二者互斥，故按优先级映射到单一 ImportKind。
    let kind = if !has_clause {
        ImportKind::SideEffect
    } else if is_type_only {
        ImportKind::StaticType
    } else {
        ImportKind::StaticValue
    };

    ctx.imports.push(ImportInfo {
        module_path,
        symbols,
        kind,
    });
}

/// 解析 import_clause 的三种绑定形态为 ImportedSymbol。
///
/// `import D, { a as b } from "x"` / `import * as ns from "y"` 的 import_clause：
///   (identifier)                          // D —— default import
///   (named_imports                        // { a as b }
///      (import_specifier name:(identifier) alias:(identifier)))
///   (namespace_import (identifier))        // * as ns
fn extract_import_clause(node: Node, source: &str, symbols: &mut Vec<ImportedSymbol>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else {
            continue;
        };
        match child.kind() {
            "identifier" => {
                symbols.push(ImportedSymbol {
                    name: text(child, source),
                    alias: None,
                    kind: SymbolKind::Default,
                });
            }
            "named_imports" => {
                let cc = child.child_count();
                for j in 0..cc {
                    let Some(spec) = child.child(j) else {
                        continue;
                    };
                    if spec.kind() == "import_specifier" {
                        let original = spec
                            .child_by_field_name("name")
                            .map(|n| text(n, source))
                            .unwrap_or_default();
                        let alias = spec.child_by_field_name("alias").map(|n| text(n, source));
                        symbols.push(ImportedSymbol {
                            name: original,
                            alias,
                            kind: SymbolKind::Named,
                        });
                    }
                }
            }
            "namespace_import" => {
                let cc = child.child_count();
                for j in 0..cc {
                    let Some(id) = child.child(j) else {
                        continue;
                    };
                    if id.kind() == "identifier" {
                        symbols.push(ImportedSymbol {
                            name: "*".to_string(),
                            alias: Some(text(id, source)),
                            kind: SymbolKind::Namespace,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

// === Call 提取 ===

fn extract_call(node: Node, ctx: &mut AnalysisContext) {
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };

    // 动态 import
    if func.kind() == "import" {
        if let Some(args) = node.child_by_field_name("arguments") {
            let ac = args.child_count();
            for i in 0..ac {
                let Some(arg) = args.child(i) else {
                    continue;
                };
                if arg.is_named() {
                    let src = text(arg, ctx.source);
                    let src = src.trim_matches(|c: char| c == '\'' || c == '"');
                    ctx.imports.push(ImportInfo {
                        module_path: src.to_string(),
                        symbols: Vec::new(),
                        kind: ImportKind::Dynamic,
                    });
                    break;
                }
            }
        }
        return;
    }

    let callee = callee_name(func, ctx.source);
    if !callee.is_empty() {
        ctx.calls.push(CallInfo {
            callee,
            is_constructor: false,
        });
    }
}

fn extract_new(node: Node, ctx: &mut AnalysisContext) {
    if let Some(ctor) = node.child_by_field_name("constructor") {
        let name = callee_name(ctor, ctx.source);
        if !name.is_empty() {
            ctx.calls.push(CallInfo {
                callee: name,
                is_constructor: true,
            });
        }
    }
}

fn callee_name(node: Node, source: &str) -> String {
    match node.kind() {
        "identifier" => text(node, source),
        "member_expression" => {
            let obj = node
                .child_by_field_name("object")
                .map(|n| callee_name(n, source))
                .unwrap_or_default();
            let prop = node
                .child_by_field_name("property")
                .map(|n| text(n, source))
                .unwrap_or_default();
            format!("{obj}.{prop}")
        }
        _ => String::new(),
    }
}

// === 工具函数 ===

fn text(node: Node, source: &str) -> String {
    source.get(node.byte_range()).unwrap_or("").to_string()
}

fn node_span(node: Node) -> Span {
    Span {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

fn has_anon_child(node: Node, keyword: &str) -> bool {
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == keyword {
                return true;
            }
        }
    }
    false
}

fn get_source_string(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("source").map(|s| {
        let raw = text(s, source);
        raw.trim_matches(|c| c == '\'' || c == '"').to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_exports_and_imports() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
import { readFile } from 'fs';
import type { User } from './types';

export function add(a: number, b: number) { return a + b; }
export class Calc {}
"#;
        let result = adapter.analyze_file(source, "test.ts").unwrap();

        let names: Vec<&str> = result.nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(
            names.contains(&"add"),
            "should have function add: {names:?}"
        );
        assert!(names.contains(&"Calc"), "should have class Calc: {names:?}");

        assert_eq!(result.imports.len(), 2);
        assert!(result.imports.iter().any(|i| i.module_path == "fs"));
        assert!(result
            .imports
            .iter()
            .any(|i| i.kind == ImportKind::StaticType));

        let export_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Exports)
            .collect();
        assert!(
            export_edges.len() >= 2,
            "should have exports edges: {export_edges:?}"
        );
    }

    #[test]
    fn analyze_class_methods_and_extends() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
export interface Serializable { serialize(): string; }
export class AuthService implements Serializable {
    authenticate() {}
    serialize() { return ""; }
}
"#;
        let result = adapter.analyze_file(source, "auth.ts").unwrap();

        let has_auth = result
            .nodes
            .iter()
            .any(|n| n.name == "AuthService" && n.node_type == NodeType::Class);
        assert!(has_auth, "should have AuthService class");

        let methods: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.name.starts_with("AuthService."))
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            methods.contains(&"AuthService.authenticate"),
            "methods: {methods:?}"
        );

        let extends_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Extends)
            .collect();
        assert!(!extends_edges.is_empty(), "should have extends edge");
        assert_eq!(
            extends_edges[0].sub_kind,
            Some(EdgeSubKind::Implements),
            "should be implements"
        );
    }

    #[test]
    fn extends_generic_base_no_type_arguments_noise_edge() {
        // `extends Base<T>` 的泛型参数 `<T>`（type_arguments 节点）不应生成
        // target 名含 "<" 的噪声 Extends 边；应只产生一条指向基础类型名 Base 的边。
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
class Base<T> {}
class Derived extends Base<number> {}
"#;
        let result = adapter.analyze_file(source, "generic.ts").unwrap();

        let extends_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Extends)
            .map(|e| e.target.as_str())
            .collect();

        assert!(
            !extends_targets.iter().any(|t| t.contains('<')),
            "不应有 type_arguments 噪声边: {extends_targets:?}"
        );
        assert!(
            extends_targets.iter().any(|t| t.ends_with(":Base")),
            "应有指向 Base 的 Extends 边: {extends_targets:?}"
        );
        assert_eq!(
            extends_targets.len(),
            1,
            "应恰好一条 Extends 边: {extends_targets:?}"
        );
    }

    #[test]
    fn analyze_calls() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
foo();
const x = new Bar();
obj.method();
"#;
        let result = adapter.analyze_file(source, "calls.ts").unwrap();
        let callees: Vec<&str> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"foo"), "callees: {callees:?}");
        assert!(callees.contains(&"obj.method"), "callees: {callees:?}");

        let ctors: Vec<&str> = result
            .calls
            .iter()
            .filter(|c| c.is_constructor)
            .map(|c| c.callee.as_str())
            .collect();
        assert!(ctors.contains(&"Bar"), "constructors: {ctors:?}");
    }

    #[test]
    fn analyze_dynamic_import() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = "const m = import('./module');";
        let result = adapter.analyze_file(source, "dyn.ts").unwrap();
        let dynamic: Vec<_> = result
            .imports
            .iter()
            .filter(|i| i.kind == ImportKind::Dynamic)
            .collect();
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].module_path, "./module");
    }

    #[test]
    fn generic_calls_are_known_limitation() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
import { fetchData } from './utils';
async function load() {
    const data = await fetchData<number[]>("url");
    return data;
}
"#;
        let result = adapter.analyze_file(source, "test.ts").unwrap();
        let callees: Vec<&str> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        // tree-sitter 将 fetchData<T>() 解析为比较表达式而非泛型调用
        // 这是已知限制（Spike S3 精度报告已记录）
        assert!(
            !callees.contains(&"fetchData"),
            "tree-sitter 的已知限制：泛型调用 f<T>() 不被识别为 call_expression"
        );
    }

    #[test]
    fn star_reexport_produces_import_edge() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        // `export *` / `export * as` 必须产生 ImportInfo，否则 build.rs 不建依赖边
        let result = adapter
            .analyze_file(
                "export * from './utils';\nexport * as types from './models';",
                "barrel.ts",
            )
            .unwrap();
        assert!(
            result.imports.iter().any(|i| i.module_path == "./utils"),
            "export * 应产生对 ./utils 的 import: {:?}",
            result.imports
        );
        assert!(
            result.imports.iter().any(|i| i.module_path == "./models"),
            "export * as 应产生对 ./models 的 import: {:?}",
            result.imports
        );
    }

    #[test]
    fn exported_arrow_const_produces_function_node() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let result = adapter
            .analyze_file(
                "export const handler = () => {};\nconst plain = 42;",
                "h.ts",
            )
            .unwrap();
        let handler = result
            .nodes
            .iter()
            .find(|n| n.name == "handler" && n.node_type == NodeType::Function);
        assert!(handler.is_some(), "箭头常量应产生 Function 节点");
        assert!(handler.unwrap().is_exported, "handler 应标记为导出");
        assert!(
            !result.nodes.iter().any(|n| n.name == "plain"),
            "非函数常量不应产生节点"
        );
    }

    #[test]
    fn class_data_field_is_not_a_function() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let result = adapter
            .analyze_file(
                "class C {\n  count: number = 0;\n  run = () => {};\n  method() {}\n}",
                "c.ts",
            )
            .unwrap();
        let fn_names: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            !fn_names.contains(&"C.count"),
            "数据字段 count 不应是 Function: {fn_names:?}"
        );
        assert!(
            fn_names.contains(&"C.run"),
            "箭头字段 run 应是 Function: {fn_names:?}"
        );
        assert!(
            fn_names.contains(&"C.method"),
            "method 应是 Function: {fn_names:?}"
        );
    }

    #[test]
    fn generic_supertype_name_is_normalized() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let result = adapter
            .analyze_file("class A extends Base<string> {}", "a.ts")
            .unwrap();
        let extends_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Extends);
        assert!(extends_edge.is_some(), "应有 Extends 边");
        // 目标 ID 暂用 interface 前缀，但名字必须归一化为 "Base"（非 "Base<string>"）
        assert!(
            extends_edge.unwrap().target.as_str().ends_with(":Base"),
            "父类型名应归一化为 Base，实际: {}",
            extends_edge.unwrap().target.as_str()
        );
    }

    #[test]
    fn interface_extends_generates_edge() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        // interface extends interface(s)：继承节点为 extends_type_clause（非 class_heritage）。
        let result = adapter
            .analyze_file("interface Foo extends Bar, Baz<T> {}", "f.ts")
            .unwrap();
        let extends_targets: Vec<String> = result
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Extends)
            .map(|e| e.target.as_str().to_string())
            .collect();
        assert_eq!(
            extends_targets.len(),
            2,
            "interface 多继承应有 2 条 Extends 边: {extends_targets:?}"
        );
        assert!(
            extends_targets.iter().any(|t| t.ends_with(":Bar")),
            "应有指向 Bar 的 Extends 边: {extends_targets:?}"
        );
        assert!(
            extends_targets.iter().any(|t| t.ends_with(":Baz")),
            "泛型父接口应归一化为 Baz（非 Baz<T>）: {extends_targets:?}"
        );
        assert!(
            !extends_targets.iter().any(|t| t.contains('<')),
            "不应有 type_arguments 噪声边: {extends_targets:?}"
        );
    }

    #[test]
    fn nested_class_method_dynamic_import_is_captured() {
        // 回归：walk_ast 必须递归进入 class body。嵌套在 class 方法内的
        // dynamic import（含嵌套 class 内部）应被采集为 is_dynamic 的 ImportInfo，
        // 嵌套 class 的方法也应生成 Function 节点。
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
class Outer {
    async load() {
        const a = await import('./feature-a');
        class Inner {
            run() { import('./feature-b'); }
        }
    }
}
"#;
        let result = adapter.analyze_file(source, "nested.ts").unwrap();

        let dynamic: Vec<&str> = result
            .imports
            .iter()
            .filter(|i| i.kind == ImportKind::Dynamic)
            .map(|i| i.module_path.as_str())
            .collect();
        assert!(
            dynamic.contains(&"./feature-a"),
            "class 方法内的 dynamic import 应被捕获: {dynamic:?}"
        );
        assert!(
            dynamic.contains(&"./feature-b"),
            "嵌套 class 方法内的 dynamic import 应被捕获: {dynamic:?}"
        );

        // 嵌套 class 的方法应作为 Function 节点入图（验证完整递归而非 calls-only）
        let methods: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .map(|n| n.name.as_str())
            .collect();
        assert!(
            methods.contains(&"Inner.run"),
            "嵌套 class Inner 的方法 run 应生成 Function 节点: {methods:?}"
        );
    }

    #[test]
    fn method_body_local_bindings_do_not_leak_to_file_scope() {
        let mut adapter = TypeScriptAdapter::new().unwrap();
        let source = r#"
class Service {
    init() {
        const helper = () => 42;
        function localFn() {}
        const svc = new OtherService();
    }
}
"#;
        let result = adapter.analyze_file(source, "scope.ts").unwrap();

        let fn_names: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .map(|n| n.name.as_str())
            .collect();
        // 方法体内的 const helper / function localFn 不应成为文件级节点
        assert!(
            !fn_names.contains(&"helper"),
            "方法体内 const helper 不应成为文件级 Function 节点: {fn_names:?}"
        );
        assert!(
            !fn_names.contains(&"localFn"),
            "方法体内 function localFn 不应成为文件级 Function 节点: {fn_names:?}"
        );
        // Service.init 应正常存在
        assert!(
            fn_names.contains(&"Service.init"),
            "Service.init 方法应为 Function 节点: {fn_names:?}"
        );
        // 方法体内的 constructor binding 不应泄漏到文件级
        assert!(
            result.constructor_bindings.is_empty(),
            "方法体内 const svc = new OtherService() 不应泄漏到 constructor_bindings: {:?}",
            result.constructor_bindings
        );
    }

    #[test]
    fn can_handle_ts_files() {
        let adapter = TypeScriptAdapter::new().unwrap();
        assert!(adapter.can_handle(Path::new("src/utils.ts")));
        assert!(!adapter.can_handle(Path::new("src/utils.d.ts")));
        assert!(!adapter.can_handle(Path::new("src/main.rs")));
        assert!(!adapter.can_handle(Path::new("src/app.py")));
        // 测试文件非迁移源，应排除
        assert!(!adapter.can_handle(Path::new("src/utils.test.ts")));
        assert!(!adapter.can_handle(Path::new("src/utils.spec.ts")));
        assert!(!adapter.can_handle(Path::new("src/Button.test.tsx")));
        assert!(!adapter.can_handle(Path::new("src/Button.spec.tsx")));
    }
}
