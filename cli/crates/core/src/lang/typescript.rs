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

impl Default for TypeScriptAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptAdapter {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .expect("tree-sitter TypeScript grammar");
        Self { parser }
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn language(&self) -> SourceLang {
        SourceLang::TypeScript
    }

    fn can_handle(&self, path: &Path) -> bool {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        name.ends_with(".ts") && !name.ends_with(".d.ts")
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
            id: NodeId::new(format!("file:{rel_path}")),
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
        let file_id = NodeId::new(format!("file:{rel_path}"));
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
                if !symbols.is_empty() {
                    ctx.imports.push(ImportInfo {
                        module_path,
                        symbols,
                        is_type_only,
                        is_side_effect: false,
                        is_dynamic: false,
                    });
                }
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
            id: NodeId::new(format!("function:{}:{name}", ctx.rel_path)),
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

fn extract_class(node: Node, ctx: &mut AnalysisContext) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = text(name_node, ctx.source);
    let is_exported = ctx.exported_names.contains(&name);
    let is_abstract = node.kind() == "abstract_class_declaration";
    let class_id = format!("class:{}:{name}", ctx.rel_path);

    ctx.nodes.push(SourceNode {
        id: NodeId::new(&class_id),
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
        extract_methods(body, &class_id, &name, ctx);
    }

    // extends / implements
    extract_heritage(node, &class_id, ctx);

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
        if child.kind() != "method_definition" && child.kind() != "public_field_definition" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let method_name = text(name_node, ctx.source);
        let is_async = has_anon_child(child, "async");
        let method_id = format!("function:{}:{class_name}.{method_name}", ctx.rel_path);

        ctx.nodes.push(SourceNode {
            id: NodeId::new(&method_id),
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
            target: NodeId::new(&method_id),
            edge_type: EdgeType::Contains,
            provenance: Provenance::TreeSitter,
            weight: 1.0,
            sub_kind: None,
            mapping_notes: None,
        });
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
                let target_name = text(type_node, ctx.source);
                if target_name.is_empty() {
                    continue;
                }
                // 目标 ID 暂用 interface 前缀，build.rs 在图完成后会修正
                let target_id = format!("interface:{}:{target_name}", ctx.rel_path);
                ctx.edges.push(Dependency {
                    source: NodeId::new(class_id),
                    target: NodeId::new(&target_id),
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
            id: NodeId::new(format!("interface:{}:{name}", ctx.rel_path)),
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
            id: NodeId::new(format!("enum:{}:{name}", ctx.rel_path)),
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
        let mut adapter = TypeScriptAdapter::new();
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
        let mut adapter = TypeScriptAdapter::new();
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
        let mut adapter = TypeScriptAdapter::new();
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
        let mut adapter = TypeScriptAdapter::new();
        let source = "const m = import('./module');";
        let result = adapter.analyze_file(source, "dyn.ts").unwrap();
        let dynamic: Vec<_> = result.imports.iter().filter(|i| i.is_dynamic).collect();
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].module_path, "./module");
    }

    #[test]
    fn generic_calls_are_known_limitation() {
        let mut adapter = TypeScriptAdapter::new();
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
    fn can_handle_ts_files() {
        let adapter = TypeScriptAdapter::new();
        assert!(adapter.can_handle(Path::new("src/utils.ts")));
        assert!(!adapter.can_handle(Path::new("src/utils.d.ts")));
        assert!(!adapter.can_handle(Path::new("src/main.rs")));
        assert!(!adapter.can_handle(Path::new("src/app.py")));
    }
}
