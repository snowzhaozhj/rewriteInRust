use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// TypeScript 源码提取器，基于 tree-sitter 解析 AST 并提取导出、导入和调用信息。
pub struct TsExtractor {
    parser: Parser,
}

/// 单个 TypeScript 文件的提取结果，包含导出、导入和调用三类符号集合。
#[derive(Debug, Default)]
pub struct Extraction {
    pub exports: HashSet<String>,
    pub imports: HashSet<String>,
    pub calls: HashSet<String>,
}

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

fn has_anonymous_child(node: Node, keyword: &str) -> bool {
    let mut c = node.walk();
    let found = node.children(&mut c).any(|ch| ch.kind() == keyword);
    found
}

impl Default for TsExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl TsExtractor {
    /// 创建新的提取器实例，初始化 tree-sitter TypeScript 解析器。
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .expect("tree-sitter TypeScript language");
        Self { parser }
    }

    /// 解析 TypeScript 源码并提取导出、导入和调用信息。
    ///
    /// 解析失败时返回 `MigrateError::Parse` 错误。
    pub fn extract(
        &mut self,
        source: &str,
        path: &std::path::Path,
    ) -> crate::error::Result<Extraction> {
        let tree =
            self.parser
                .parse(source, None)
                .ok_or_else(|| crate::error::MigrateError::Parse {
                    path: path.to_path_buf(),
                })?;
        let mut result = Extraction::default();
        self.walk(tree.root_node(), source, &mut result);
        Ok(result)
    }

    fn walk(&self, node: Node, source: &str, result: &mut Extraction) {
        match node.kind() {
            "export_statement" => self.extract_export(node, source, result),
            "import_statement" => self.extract_import(node, source, result),
            "call_expression" => self.extract_call(node, source, result),
            "new_expression" => self.extract_new(node, source, result),
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child, source, result);
        }
    }

    fn extract_export(&self, node: Node, source: &str, result: &mut Extraction) {
        let is_default = has_anonymous_child(node, "default");

        if let Some(decl) = node.child_by_field_name("declaration") {
            let names = self.declaration_names(decl, source);
            for n in names {
                result.exports.insert(n);
            }
            if is_default {
                result.exports.insert("default".to_string());
            }
            return;
        }

        if node.child_by_field_name("value").is_some() || is_default {
            result.exports.insert("default".to_string());
            return;
        }

        let mut c = node.walk();
        for child in node.children(&mut c) {
            match child.kind() {
                "export_clause" => self.extract_export_clause(child, source, result),
                "*" => {
                    let src = self.get_source_string(node, source);
                    let label = match src {
                        Some(s) => format!("*<-{s}"),
                        None => "*".to_string(),
                    };
                    result.exports.insert(label);
                }
                "namespace_export" => {
                    let mut nc = child.walk();
                    let id = child.children(&mut nc).find(|ch| ch.kind() == "identifier");
                    if let Some(id) = id {
                        result.exports.insert(node_text(id, source).to_string());
                    }
                }
                _ => {}
            }
        }
    }

    fn declaration_names(&self, decl: Node, source: &str) -> Vec<String> {
        match decl.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration" => decl
                .child_by_field_name("name")
                .map(|n| vec![node_text(n, source).to_string()])
                .unwrap_or_default(),
            "lexical_declaration" | "variable_declaration" => {
                let mut names = Vec::new();
                let mut c = decl.walk();
                for child in decl.children(&mut c) {
                    if child.kind() == "variable_declarator" {
                        if let Some(n) = child.child_by_field_name("name") {
                            names.push(node_text(n, source).to_string());
                        }
                    }
                }
                names
            }
            _ => Vec::new(),
        }
    }

    fn extract_export_clause(&self, node: Node, source: &str, result: &mut Extraction) {
        let mut c = node.walk();
        for spec in node.children(&mut c) {
            if spec.kind() == "export_specifier" {
                let name = spec
                    .child_by_field_name("alias")
                    .or_else(|| spec.child_by_field_name("name"))
                    .map(|n| node_text(n, source).to_string());
                if let Some(n) = name {
                    result.exports.insert(n);
                }
            }
        }
    }

    fn extract_import(&self, node: Node, source: &str, result: &mut Extraction) {
        let source_mod = self
            .get_source_string(node, source)
            .unwrap_or_else(|| "<unknown>".to_string());

        let is_type_only = {
            let mut c = node.walk();
            let found = node
                .children(&mut c)
                .any(|ch| ch.kind() == "type" && node_text(ch, source) == "type");
            found
        };

        let mut found_clause = false;
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "import_clause" {
                found_clause = true;
                self.extract_import_clause(child, source, &source_mod, is_type_only, result);
            }
        }

        if !found_clause {
            result.imports.insert(format!("<-{source_mod}"));
        }
    }

    fn extract_import_clause(
        &self,
        node: Node,
        source: &str,
        source_mod: &str,
        is_type_only: bool,
        result: &mut Extraction,
    ) {
        let prefix = if is_type_only { "type:" } else { "" };

        let mut c = node.walk();
        for child in node.children(&mut c) {
            match child.kind() {
                "identifier" => {
                    let name = node_text(child, source);
                    result
                        .imports
                        .insert(format!("{prefix}default:{name}<-{source_mod}"));
                }
                "named_imports" => {
                    let mut nc = child.walk();
                    for spec in child.children(&mut nc) {
                        if spec.kind() == "import_specifier" {
                            let original = spec
                                .child_by_field_name("name")
                                .map(|n| node_text(n, source))
                                .unwrap_or("");
                            result
                                .imports
                                .insert(format!("{prefix}{original}<-{source_mod}"));
                        }
                    }
                }
                "namespace_import" => {
                    let mut nc = child.walk();
                    let alias = child
                        .children(&mut nc)
                        .find(|ch| ch.kind() == "identifier")
                        .map(|n| node_text(n, source))
                        .unwrap_or("*");
                    result
                        .imports
                        .insert(format!("{prefix}*:{alias}<-{source_mod}"));
                }
                _ => {}
            }
        }
    }

    fn extract_call(&self, node: Node, source: &str, result: &mut Extraction) {
        if let Some(func) = node.child_by_field_name("function") {
            if func.kind() == "import" {
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut c = args.walk();
                    let first_arg = args.children(&mut c).find(|ch| ch.is_named());
                    if let Some(first_arg) = first_arg {
                        let src = node_text(first_arg, source);
                        let src = src.trim_matches(|c: char| c == '\'' || c == '"');
                        result.imports.insert(format!("dynamic<-{src}"));
                    }
                }
                return;
            }

            let callee = self.callee_name(func, source);
            if !callee.is_empty() {
                result.calls.insert(format!("call:{callee}"));
            }
        }
    }

    fn extract_new(&self, node: Node, source: &str, result: &mut Extraction) {
        if let Some(ctor) = node.child_by_field_name("constructor") {
            let name = self.callee_name(ctor, source);
            if !name.is_empty() {
                result.calls.insert(format!("new:{name}"));
            }
        }
    }

    fn callee_name(&self, node: Node, source: &str) -> String {
        match node.kind() {
            "identifier" => node_text(node, source).to_string(),
            "member_expression" => {
                let obj = node
                    .child_by_field_name("object")
                    .map(|n| self.callee_name(n, source))
                    .unwrap_or_default();
                let prop = node
                    .child_by_field_name("property")
                    .map(|n| node_text(n, source))
                    .unwrap_or("");
                format!("{obj}.{prop}")
            }
            _ => String::new(),
        }
    }

    fn get_source_string(&self, node: Node, source: &str) -> Option<String> {
        node.child_by_field_name("source").map(|s| {
            let raw = node_text(s, source);
            raw.trim_matches(|c| c == '\'' || c == '"').to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 测试辅助：用固定路径调用 extract 并 unwrap。
    fn ex(ext: &mut TsExtractor, source: &str) -> Extraction {
        ext.extract(source, Path::new("<test>")).unwrap()
    }

    #[test]
    fn export_named_function() {
        let mut ext = TsExtractor::new();
        let r = ex(
            &mut ext,
            "export function add(a: number, b: number) { return a + b; }",
        );
        assert!(r.exports.contains("add"));
        assert!(!r.exports.contains("default"));
    }

    #[test]
    fn export_default_function() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "export default function main() {}");
        assert!(r.exports.contains("default"));
        assert!(r.exports.contains("main"));
    }

    #[test]
    fn export_default_expression() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "const x = 1;\nexport default x;");
        assert!(r.exports.contains("default"));
    }

    #[test]
    fn export_reexport_named() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "export { foo, bar as baz } from './source';");
        assert!(r.exports.contains("foo"));
        assert!(r.exports.contains("baz"));
    }

    #[test]
    fn export_reexport_star() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "export * from './source';");
        assert!(r.exports.contains("*<-./source"));
    }

    #[test]
    fn import_named() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "import { readFile, writeFile } from 'fs';");
        assert!(r.imports.contains("readFile<-fs"));
        assert!(r.imports.contains("writeFile<-fs"));
    }

    #[test]
    fn import_default() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "import express from 'express';");
        assert!(r.imports.contains("default:express<-express"));
    }

    #[test]
    fn import_star() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "import * as path from 'path';");
        assert!(r.imports.contains("*:path<-path"));
    }

    #[test]
    fn import_type_only() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "import type { User } from './types';");
        assert!(r.imports.contains("type:User<-./types"));
    }

    #[test]
    fn import_side_effect() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "import './polyfill';");
        assert!(r.imports.contains("<-./polyfill"));
    }

    #[test]
    fn call_simple() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "foo();\nbar(1, 2);");
        assert!(r.calls.contains("call:foo"));
        assert!(r.calls.contains("call:bar"));
    }

    #[test]
    fn call_method() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "obj.method();\nconsole.log('hi');");
        assert!(r.calls.contains("call:obj.method"));
        assert!(r.calls.contains("call:console.log"));
    }

    #[test]
    fn call_constructor() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "const x = new EventEmitter();");
        assert!(r.calls.contains("new:EventEmitter"));
    }

    #[test]
    fn dynamic_import() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "const m = import('./module');");
        assert!(
            r.imports.contains("dynamic<-./module"),
            "dynamic import should contain 'dynamic<-./module': {:?}",
            r.imports
        );
    }

    #[test]
    fn export_multiple_const() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "export const PI = 3.14;\nexport let counter = 0;");
        assert!(r.exports.contains("PI"));
        assert!(r.exports.contains("counter"));
    }

    #[test]
    fn export_multi_variable_declaration() {
        let mut ext = TsExtractor::new();
        let r = ex(&mut ext, "export const a = 1, b = 2;");
        assert!(
            r.exports.contains("a"),
            "should export 'a': {:?}",
            r.exports
        );
        assert!(
            r.exports.contains("b"),
            "should export 'b': {:?}",
            r.exports
        );
    }

    #[test]
    fn export_types() {
        let mut ext = TsExtractor::new();
        let r = ex(
            &mut ext,
            "export interface Config { host: string; }\nexport type Handler = () => void;\nexport enum Level { A, B }",
        );
        assert!(r.exports.contains("Config"));
        assert!(r.exports.contains("Handler"));
        assert!(r.exports.contains("Level"));
    }

    #[test]
    fn extract_empty_source() {
        let mut ext = TsExtractor::new();
        let r = ext.extract("", Path::new("empty.ts")).unwrap();
        assert!(r.exports.is_empty());
        assert!(r.imports.is_empty());
        assert!(r.calls.is_empty());
    }
}
