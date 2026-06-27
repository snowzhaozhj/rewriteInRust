//! Python 语言适配器。
//!
//! 基于 tree-sitter-python 的单文件分析。M3 Sprint B 逐步实现：
//! - PY-01: 结构体 + language/can_handle/detect_tier
//! - PY-02~06: analyze_file / resolve_import（后续 PR）

use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::SourceLang;
use crate::types::state::ModuleTier;

use super::{FileAnalysis, LanguageAdapter};

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

    fn analyze_file(&mut self, _source: &str, _rel_path: &str) -> Result<FileAnalysis> {
        Err(MigrateError::NotImplemented(
            "PythonAdapter::analyze_file 尚未实现（PR-B2）".into(),
        ))
    }

    fn resolve_import(
        &self,
        _specifier: &str,
        _current_file: &str,
        _exists: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
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
            "expression_statement" => {
                // 顶层简单赋值（`X = 42`）不算非平凡
                let inner = child.child(0);
                if inner.is_some_and(|n| n.kind() != "assignment") {
                    s.has_non_trivial_content = true;
                }
            }
            _ => {}
        }
    }
    s
}

fn check_danger_in_subtree(node: Node, source: &str, signals: &mut PyTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        match current.kind() {
            // async/await
            "async" => {
                signals.has_danger = true;
                return;
            }
            // try/except（异常处理 = 复杂控制流）
            "try_statement" => {
                signals.has_danger = true;
                return;
            }
            // metaclass / __getattr__ / __setattr__（动态属性）
            "decorator" => {
                let text = &source[current.byte_range()];
                if text.contains("metaclass") || text.contains("__getattr__") {
                    signals.has_danger = true;
                    return;
                }
            }
            // global/nonlocal（全局状态）
            "global_statement" | "nonlocal_statement" => {
                signals.has_danger = true;
                return;
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
}
