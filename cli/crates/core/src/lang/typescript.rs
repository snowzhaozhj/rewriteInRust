//! TypeScript 语言适配器。
//!
//! 基于 tree-sitter-typescript 的单文件完整分析：
//! 解析 AST → 提取节点（File/Function/Class/Interface/Enum）
//! + 边（Contains/Extends/Exports）+ 导入/调用信息。

use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::{NodeId, SourceLang, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, Provenance, SourceNode};

use super::{CallInfo, FileAnalysis, ImportInfo, ImportedSymbol, LanguageAdapter};

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
        (name.ends_with(".ts") || name.ends_with(".tsx")) && !name.ends_with(".d.ts")
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
        };

        // 第一遍：收集 export 名称
        collect_exports(tree.root_node(), source, &mut ctx.exported_names);

        // 添加 File 节点
        ctx.nodes.push(SourceNode {
            id: NodeId::file(rel_path),
            node_type: NodeType::File,
            name: rel_path.to_string(),
            file_path: rel_path.to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });

        // 第二遍：提取符号节点、边、导入、调用
        walk_ast(tree.root_node(), &mut ctx);

        // 生成 Exports 边
        let file_id = NodeId::file(rel_path);
        for node in &ctx.nodes {
            if node.is_exported && node.node_type != NodeType::File {
                ctx.edges.push(Dependency {
                    source: file_id.clone(),
                    target: node.id.clone(),
                    edge_type: EdgeType::Exports,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: None,
                    mapping_notes: None,
                });
            }
        }

        Ok(FileAnalysis {
            nodes: ctx.nodes,
            edges: ctx.edges,
            imports: ctx.imports,
            calls: ctx.calls,
            exported_names: ctx.exported_names,
        })
    }
}

struct AnalysisContext<'a> {
    rel_path: &'a str,
    source: &'a str,
    nodes: Vec<SourceNode>,
    edges: Vec<Dependency>,
    imports: Vec<ImportInfo>,
    calls: Vec<CallInfo>,
    exported_names: std::collections::HashSet<String>,
}

// === Export 收集 ===

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

fn walk_ast(node: Node, ctx: &mut AnalysisContext) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
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
        // `const f = () => {}` / `const f = function() {}` 等函数表达式绑定
        // （不 return，继续递归以提取箭头体内的 call/new）
        "lexical_declaration" | "variable_declaration" => {
            extract_var_functions(node, ctx);
        }
        "export_statement" => {
            if let Some(decl) = node.child_by_field_name("declaration") {
                walk_ast(decl, ctx);
                return;
            }
            // re-export: `export { x } from './module'` — 也生成 ImportInfo
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
                                            is_default: false,
                                            is_namespace: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                // 即使 symbols 为空（`export * from`、`export * as ns from`）也要
                // 产生 ImportInfo，否则 build.rs 不会建立 file→file 的 Imports 边，
                // barrel 文件的依赖会对拓扑排序不可见
                ctx.imports.push(ImportInfo {
                    module_path,
                    symbols,
                    is_type_only,
                    is_side_effect: false,
                    is_dynamic: false,
                });
            }
        }
        "import_statement" => {
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
            walk_ast(child, ctx);
        }
    }
}

fn walk_ast_calls_only(node: Node, ctx: &mut AnalysisContext) {
    match node.kind() {
        "call_expression" => extract_call(node, ctx),
        "new_expression" => extract_new(node, ctx),
        _ => {}
    }
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            walk_ast_calls_only(child, ctx);
        }
    }
}

fn extract_function(node: Node, ctx: &mut AnalysisContext) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = text(name_node, ctx.source);
        let is_async = has_anon_child(node, "async");
        let is_exported = ctx.exported_names.contains(&name);
        ctx.nodes.push(SourceNode {
            id: NodeId::symbol(NodeType::Function, ctx.rel_path, &name),
            node_type: NodeType::Function,
            name,
            file_path: ctx.rel_path.to_string(),
            line_range: Some(node_span(node)),
            is_exported,
            complexity: None,
            is_async,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
    }
}

/// 判断一个值节点是否为函数表达式（箭头/函数/生成器）。
fn is_function_value(kind: &str) -> bool {
    matches!(
        kind,
        "arrow_function" | "function" | "function_expression" | "generator_function"
    )
}

/// 从 `lexical_declaration`/`variable_declaration` 中提取函数表达式绑定为 Function 节点。
/// 例如 `export const handler = () => {}` —— 仅 `export function` 会被 walk_ast 的
/// function_declaration 分支处理，箭头/函数表达式常量此前完全不入图。
fn extract_var_functions(node: Node, ctx: &mut AnalysisContext) {
    let count = node.child_count();
    for i in 0..count {
        let Some(declarator) = node.child(i) else {
            continue;
        };
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(value) = declarator.child_by_field_name("value") else {
            continue;
        };
        if !is_function_value(value.kind()) {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        // 仅处理简单标识符绑定（跳过解构 `const { a } = ...`）
        if name_node.kind() != "identifier" {
            continue;
        }
        let name = text(name_node, ctx.source);
        if name.is_empty() {
            continue;
        }
        let is_async = has_anon_child(value, "async");
        let is_exported = ctx.exported_names.contains(&name);
        ctx.nodes.push(SourceNode {
            id: NodeId::symbol(NodeType::Function, ctx.rel_path, &name),
            node_type: NodeType::Function,
            name,
            file_path: ctx.rel_path.to_string(),
            line_range: Some(node_span(declarator)),
            is_exported,
            complexity: None,
            is_async,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
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

    ctx.nodes.push(SourceNode {
        id: class_id.clone(),
        node_type: NodeType::Class,
        name: name.clone(),
        file_path: ctx.rel_path.to_string(),
        line_range: Some(node_span(node)),
        is_exported,
        complexity: None,
        is_async: false,
        visibility: None,
        is_abstract,
        decorators: Vec::new(),
        migration_status: None,
        migration_priority: None,
        rust_kind: None,
        rust_path: None,
        crate_name: None,
    });

    // 方法
    if let Some(body) = node.child_by_field_name("body") {
        extract_methods(body, class_id.as_str(), &name, ctx);
    }

    // extends / implements
    extract_heritage(node, class_id.as_str(), ctx);

    // 递归处理 body 内所有子节点（方法体内的 call/new 也要提取）
    if let Some(body) = node.child_by_field_name("body") {
        let count = body.child_count();
        for i in 0..count {
            if let Some(child) = body.child(i) {
                walk_ast_calls_only(child, ctx);
            }
        }
    }
}

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

        ctx.nodes.push(SourceNode {
            id: method_id.clone(),
            node_type: NodeType::Function,
            name: format!("{class_name}.{method_name}"),
            file_path: ctx.rel_path.to_string(),
            line_range: Some(node_span(child)),
            is_exported: false,
            complexity: None,
            is_async,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });

        ctx.edges.push(Dependency {
            source: NodeId::new(class_id),
            target: method_id,
            edge_type: EdgeType::Contains,
            provenance: Provenance::TreeSitter,
            weight: 1.0,
            sub_kind: None,
            mapping_notes: None,
        });
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
                if !type_node.is_named()
                    || type_node.kind() == "extends"
                    || type_node.kind() == "implements"
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
                ctx.edges.push(Dependency {
                    source: NodeId::new(class_id),
                    target: target_id,
                    edge_type: EdgeType::Extends,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: if is_implements {
                        Some("implements".to_string())
                    } else {
                        None
                    },
                    mapping_notes: None,
                });
            }
        }
    }
}

fn extract_interface(node: Node, ctx: &mut AnalysisContext) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = text(name_node, ctx.source);
        let is_exported = ctx.exported_names.contains(&name);
        ctx.nodes.push(SourceNode {
            id: NodeId::symbol(NodeType::Interface, ctx.rel_path, &name),
            node_type: NodeType::Interface,
            name,
            file_path: ctx.rel_path.to_string(),
            line_range: Some(node_span(node)),
            is_exported,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
    }
}

fn extract_enum(node: Node, ctx: &mut AnalysisContext) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = text(name_node, ctx.source);
        let is_exported = ctx.exported_names.contains(&name);
        ctx.nodes.push(SourceNode {
            id: NodeId::symbol(NodeType::Enum, ctx.rel_path, &name),
            node_type: NodeType::Enum,
            name,
            file_path: ctx.rel_path.to_string(),
            line_range: Some(node_span(node)),
            is_exported,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
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

    let is_side_effect = !has_clause;

    ctx.imports.push(ImportInfo {
        module_path,
        symbols,
        is_type_only,
        is_side_effect,
        is_dynamic: false,
    });
}

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
                    is_default: true,
                    is_namespace: false,
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
                            is_default: false,
                            is_namespace: false,
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
                            is_default: false,
                            is_namespace: true,
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
                        is_type_only: false,
                        is_side_effect: false,
                        is_dynamic: true,
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
        assert!(result.imports.iter().any(|i| i.is_type_only));

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
            extends_edges[0].sub_kind.as_deref(),
            Some("implements"),
            "should be implements"
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
        let dynamic: Vec<_> = result.imports.iter().filter(|i| i.is_dynamic).collect();
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
    fn can_handle_ts_files() {
        let adapter = TypeScriptAdapter::new().unwrap();
        assert!(adapter.can_handle(Path::new("src/utils.ts")));
        assert!(!adapter.can_handle(Path::new("src/utils.d.ts")));
        assert!(!adapter.can_handle(Path::new("src/main.rs")));
        assert!(!adapter.can_handle(Path::new("src/app.py")));
    }
}
