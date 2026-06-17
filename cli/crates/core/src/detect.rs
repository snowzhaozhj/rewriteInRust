//! 模块复杂度分档检测。
//!
//! 基于 AST 语义特征（非 LOC）评估单个 TypeScript 源文件的翻译复杂度，
//! 产出 `ModuleTier`（Trivial / Standard / Full）。
//!
//! 判据对齐 `docs/design/03-execution-model.md § 4.3.2`：
//! - **Trivial**：纯类型 / 常量 / barrel（仅 re-export）
//! - **Full**：含任一危险信号（async/try-catch/I·O/数值/全局状态等）
//! - **Standard**：其余

use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::state::ModuleTier;

/// 对单个源文件进行复杂度分档。
///
/// 读取文件内容 → tree-sitter 解析 → AST 特征扫描 → 返回分档结果。
/// 解析失败（语法错误文件）默认归 Full（不降档）。
pub fn detect_tier(file_path: &Path) -> Result<ModuleTier> {
    let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;
    detect_tier_from_source(&source)
}

/// 从源码字符串分档（供测试和已加载源码的场景使用）。
pub fn detect_tier_from_source(source: &str) -> Result<ModuleTier> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::language_typescript())
        .map_err(|e| MigrateError::Config(format!("tree-sitter TypeScript 语法加载失败: {e}")))?;

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Ok(ModuleTier::Full),
    };

    let root = tree.root_node();
    if root.has_error() {
        return Ok(ModuleTier::Full);
    }

    let signals = scan_ast(root, source);

    if signals.has_danger_signals {
        Ok(ModuleTier::Full)
    } else if signals.is_trivial {
        Ok(ModuleTier::Trivial)
    } else {
        Ok(ModuleTier::Standard)
    }
}

struct TierSignals {
    has_danger_signals: bool,
    is_trivial: bool,
}

/// 扫描 AST 顶层结构，收集分档信号。
fn scan_ast(root: Node, source: &str) -> TierSignals {
    let mut has_danger = false;
    let mut has_non_trivial_export = false;

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                if has_non_trivial_declaration(child, source) {
                    has_non_trivial_export = true;
                }
                if scan_node_for_danger(child, source) {
                    has_danger = true;
                }
            }
            "import_statement" => {
                if is_io_import(child, source) {
                    has_danger = true;
                }
            }
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {}
            "function_declaration" | "class_declaration" | "abstract_class_declaration" => {
                has_non_trivial_export = true;
                if scan_node_for_danger(child, source) {
                    has_danger = true;
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                if is_mutable_or_complex_variable(child, source) {
                    has_non_trivial_export = true;
                    let text = node_text(child, source);
                    if text.starts_with("let ") || text.starts_with("var ") {
                        has_danger = true;
                    }
                }
                if scan_node_for_danger(child, source) {
                    has_danger = true;
                }
            }
            "expression_statement" => {
                has_non_trivial_export = true;
                has_danger = true;
            }
            "try_statement" | "if_statement" | "for_statement" | "for_in_statement"
            | "while_statement" | "do_statement" | "switch_statement" => {
                has_non_trivial_export = true;
                has_danger = true;
            }
            "comment" | "ERROR" => {}
            _ => {}
        }
    }

    let is_trivial = !has_non_trivial_export && !has_danger;

    TierSignals {
        has_danger_signals: has_danger,
        is_trivial,
    }
}

/// 检查导出声明中是否包含非 trivial 内容。
///
/// trivial 导出：type/interface/enum 声明、const 纯字面量、re-export。
/// 非 trivial 导出：函数声明、类声明、含逻辑的变量。
fn has_non_trivial_declaration(export_node: Node, source: &str) -> bool {
    let mut cursor = export_node.walk();
    for child in export_node.children(&mut cursor) {
        match child.kind() {
            "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {}
            "function_declaration" | "class_declaration" | "abstract_class_declaration" => {
                return true;
            }
            "lexical_declaration" | "variable_declaration" => {
                if is_mutable_or_complex_variable(child, source) {
                    return true;
                }
            }
            // export { ... } from '...' 或 export * from '...' → re-export, trivial
            "export_clause" | "namespace_export" => {}
            // export default ... → 检查内容
            "identifier" | "string" | "number" | "true" | "false" | "null" => {}
            _ => {}
        }
    }
    false
}

/// 检查变量声明是否为可变或复杂（非纯常量）。
fn is_mutable_or_complex_variable(decl_node: Node, source: &str) -> bool {
    let text = node_text(decl_node, source);
    // let/var 是可变绑定 → 非 trivial
    if text.starts_with("let ") || text.starts_with("var ") {
        return true;
    }
    // const 中包含函数表达式或箭头函数 → 非 trivial
    if contains_function_expression(decl_node) {
        return true;
    }
    false
}

/// 检查节点子树中是否包含函数表达式。
fn contains_function_expression(node: Node) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "arrow_function" | "function" | "function_expression" => return true,
            _ => {
                cursor.reset(current);
                for child in current.children(&mut cursor) {
                    stack.push(child);
                }
            }
        }
    }
    false
}

/// 递归扫描节点子树，检查是否包含危险信号。
fn scan_node_for_danger(node: Node, source: &str) -> bool {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if is_danger_node(current, source) {
            return true;
        }
        let mut cursor = current.walk();
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// 判断单个节点是否为危险信号。
fn is_danger_node(node: Node, source: &str) -> bool {
    match node.kind() {
        // 异步：async/await
        "await_expression" => true,
        // try-catch 错误路径
        "try_statement" | "throw_statement" => true,
        // 并发模式
        "call_expression" => is_concurrent_call(node, source),
        // 全局状态：顶层 let/var 赋值
        // （已在上层 scan_ast 中处理 expression_statement）
        // I/O 模式：特定 import
        "import_statement" => is_io_import(node, source),
        // 条件类型
        "conditional_type" => true,
        // never / unknown 类型注解
        "predefined_type" => {
            let text = node_text(node, source);
            text == "never" || text == "unknown"
        }
        // async 函数/方法声明
        "function_declaration" | "method_definition" | "arrow_function" => {
            node.child_by_field_name("async").is_some()
                || node_text(node, source).starts_with("async ")
                || node_text(node, source).starts_with("async\n")
        }
        _ => false,
    }
}

/// 检查是否为并发相关调用（Promise.all / Promise.race / setTimeout 等）。
fn is_concurrent_call(node: Node, source: &str) -> bool {
    if let Some(func) = node.child_by_field_name("function") {
        let text = node_text(func, source);
        matches!(
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
        )
    } else {
        false
    }
}

/// 检查 import 是否引入 I/O 模块。
fn is_io_import(node: Node, source: &str) -> bool {
    let text = node_text(node, source);
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

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_pure_types() {
        let source = r#"
export interface User {
    id: string;
    name: string;
}

export type UserId = string;

export enum Role {
    Admin,
    User,
    Guest,
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn trivial_barrel_reexport() {
        let source = r#"
export { User, UserId } from './types';
export * from './constants';
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn trivial_const_literals() {
        let source = r#"
export const MAX_RETRIES = 3;
export const API_URL = "https://api.example.com";
export const ENABLED = true;
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn standard_simple_function() {
        let source = r#"
export function add(a: number, b: number): number {
    return a + b;
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn standard_class_no_async() {
        let source = r#"
export class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn full_async_function() {
        let source = r#"
export async function fetchData(url: string): Promise<string> {
    const response = await fetch(url);
    return response.text();
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_try_catch() {
        let source = r#"
export function safeParse(json: string): unknown {
    try {
        return JSON.parse(json);
    } catch (e) {
        return null;
    }
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_promise_all() {
        let source = r#"
export function fetchAll(urls: string[]) {
    return Promise.all(urls.map(u => fetch(u)));
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_io_import() {
        let source = r#"
import * as fs from 'fs';

export function readConfig(): string {
    return fs.readFileSync('config.json', 'utf-8');
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_global_mutable_state() {
        let source = r#"
let counter = 0;

export function increment(): number {
    counter += 1;
    return counter;
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_throw_statement() {
        let source = r#"
export function validate(x: number): void {
    if (x < 0) {
        throw new Error("negative");
    }
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn full_conditional_type() {
        let source = r#"
export type IsString<T> = T extends string ? true : false;

export function check<T>(value: T): IsString<T> {
    return (typeof value === 'string') as any;
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn trivial_non_exported_types() {
        let source = r#"
interface Internal {
    x: number;
}

type Id = string;
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Trivial
        );
    }

    #[test]
    fn standard_arrow_function_export() {
        let source = r#"
export const greet = (name: string): string => {
    return `Hello, ${name}`;
};
"#;
        assert_eq!(
            detect_tier_from_source(source).unwrap(),
            ModuleTier::Standard
        );
    }

    #[test]
    fn full_set_timeout() {
        let source = r#"
export function delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
}
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }

    #[test]
    fn empty_file_is_trivial() {
        assert_eq!(detect_tier_from_source("").unwrap(), ModuleTier::Trivial);
    }

    #[test]
    fn full_top_level_expression() {
        let source = r#"
console.log("side effect");
"#;
        assert_eq!(detect_tier_from_source(source).unwrap(), ModuleTier::Full);
    }
}
