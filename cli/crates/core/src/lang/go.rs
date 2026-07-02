//! Go 语言适配器。
//!
//! M4 Sprint A 接线：language/can_handle/resolve_extensions/detect_source_root。
//! M4 Sprint C PR-C1（GO-01）：detect_tier 复杂度分档（并发/反射/cgo/unsafe 危险信号）。
//! 待补（PR-C2/C3）：analyze_file（符号/调用/签名提取，GO-04~06）、resolve_import + 扩 trait
//! 目录列举（GO-03）、classify_file（GO-02）。
//!
//! **未实现方法不 panic 约定**：`create_adapter(Go)` 已接线，`detect_language` 会把 Go 项目
//! 自动识别为 Go——若用 `todo!()`，Go 项目跑 graph build/populate 会 panic 崩进程（违反 CLI
//! 统一 JSON 输出）。故 `analyze_file` 返回 `Err(NotImplemented)`（上层产出 `{status:error}`
//! JSON）；不返回 `Result` 的 `classify_file` 用 trait 默认 `conservative()`，绝不 panic。

use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::{MigrateError, Result};
use crate::types::common::SourceLang;
use crate::types::state::ModuleTier;

use super::{FileAnalysis, LanguageAdapter};

/// Go 语言适配器（基于 tree-sitter-go）。
pub struct GoAdapter {
    parser: Parser,
}

impl GoAdapter {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::language())
            .map_err(|e| MigrateError::Config(format!("tree-sitter Go 语法加载失败: {e}")))?;
        Ok(Self { parser })
    }
}

impl LanguageAdapter for GoAdapter {
    fn language(&self) -> SourceLang {
        SourceLang::Go
    }

    fn can_handle(&self, path: &Path) -> bool {
        path.extension().unwrap_or_default() == "go"
    }

    fn resolve_extensions(&self) -> &[&str] {
        &["go"]
    }

    fn analyze_file(&mut self, _source: &str, _rel_path: &str) -> Result<FileAnalysis> {
        // 骨架阶段：返回优雅错误而非 panic（见模块头「不 panic 约定」）。
        // Sprint C GO-04~06 实现真正的 Go 单文件分析。
        Err(MigrateError::NotImplemented(
            "Go 源码解析尚未实现（M4 Sprint C）".to_string(),
        ))
    }

    fn resolve_import(
        &self,
        _specifier: &str,
        _current_file: &str,
        _exists: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        // 骨架阶段：无解析能力，返回 None（视作外部/未解析依赖，安全）。
        // Sprint C GO-03 实现 Go 包 resolve（含扩 trait 目录列举）。
        None
    }

    fn detect_tier(&mut self, source: &str) -> ModuleTier {
        // 对齐 python.rs：parse 失败/含语法错误 → 保守 Full；否则按危险信号 + 内容分档。
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return ModuleTier::Full,
        };
        let root = tree.root_node();
        if root.has_error() {
            return ModuleTier::Full;
        }
        let signals = scan_go_tier_signals(root, source);
        if signals.has_danger {
            ModuleTier::Full
        } else if signals.has_non_trivial_content {
            ModuleTier::Standard
        } else {
            ModuleTier::Trivial
        }
    }

    // classify_file 不 override——用 trait 默认 conservative()（Normal + 无危险），
    // 保证 Go 文件绝不会被误判为机械合批，也不 panic。Sprint C GO-02 实现真正分类。

    fn detect_source_root(&self, project_root: &Path) -> Option<String> {
        // Go 项目：含 go.mod 的目录为 module 根，源码即在根目录。返回 Some(".")（探测成功）
        // 而非 None——None 语义是「未探测到，调用方回退 . 并告警」，对正确识别的 Go 项目
        // 吐 fallback warning 会误导（专项审查 nit）。
        if project_root.join("go.mod").exists() {
            return Some(".".to_string());
        }
        // 无 go.mod：回退默认 src/ 检查。
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && super::dir_has_source_files(&src_dir, self.resolve_extensions(), 5) {
            return Some("src".to_string());
        }
        None
    }
}

/// Go 复杂度分档信号（仿 python.rs `PyTierSignals`）。
#[derive(Default)]
struct GoTierSignals {
    /// 含并发/反射/cgo/unsafe 等语义无法机械翻译的危险信号 → Full。
    has_danger: bool,
    /// 含函数/方法/类型定义等实质内容 → 至少 Standard（否则纯 const/var/import → Trivial）。
    has_non_trivial_content: bool,
}

/// 扫描顶层节点分档：import 单独查危险包，函数/方法/类型体递归查并发危险。
fn scan_go_tier_signals(root: Node, source: &str) -> GoTierSignals {
    let mut s = GoTierSignals::default();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        // Go 把 `\n` 等终结符作为 source_file 的匿名子节点吐出（Python grammar 无此行为），
        // 若不跳过，`_ =>` 兜底会把纯换行误判为实质内容。
        if !child.is_named() {
            continue;
        }
        match child.kind() {
            "package_clause" | "comment" => {}
            // import 危险包（reflect/unsafe/C）在顶层 import 声明，不在函数子树。
            "import_declaration" => check_import_danger(child, source, &mut s),
            "function_declaration" | "method_declaration" | "type_declaration" => {
                s.has_non_trivial_content = true;
                check_danger_in_subtree(child, &mut s);
            }
            // 纯 const/var 声明本身算 trivial，但初始化表达式可能含 make(chan)/go 等危险。
            "const_declaration" | "var_declaration" => check_danger_in_subtree(child, &mut s),
            _ => s.has_non_trivial_content = true,
        }
    }
    s
}

/// import 声明里若引入 reflect（反射）/unsafe（unsafe.Pointer）/C（cgo），标危险。
fn check_import_danger(node: Node, source: &str, signals: &mut GoTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "import_spec" {
            if let Some(path) = current.child_by_field_name("path") {
                // interpreted_string_literal 文本含引号，如 "reflect"。
                let raw = &source[path.byte_range()];
                let pkg = raw.trim_matches(|c| c == '"' || c == '`');
                if matches!(pkg, "reflect" | "unsafe" | "C") {
                    signals.has_danger = true;
                    return;
                }
            }
        }
        cursor.reset(current);
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// 递归子树找并发危险：goroutine（go_statement）/select/channel/send。
fn check_danger_in_subtree(node: Node, signals: &mut GoTierSignals) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "go_statement" | "select_statement" | "channel_type" | "send_statement" => {
                signals.has_danger = true;
                return;
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
    use std::path::Path;

    /// 未实现方法不 panic（回归：防 todo!() 让 Go 项目 graph build 崩进程）。
    #[test]
    fn unimplemented_methods_do_not_panic() {
        let mut adapter = GoAdapter::new().unwrap();
        // analyze_file 优雅报错而非 panic。
        let err = adapter
            .analyze_file("package main\n", "main.go")
            .unwrap_err();
        assert!(
            err.to_string().contains("尚未实现"),
            "analyze_file 应返回 NotImplemented，实际: {err}"
        );
        // classify_file（trait 默认）返回保守分类，不 panic、不判机械。
        let cls = adapter.classify_file("package main\n");
        assert!(cls.danger.is_empty());
        assert!(!cls.is_mechanical());
        // resolve_import 返回 None，不 panic。
        assert_eq!(adapter.resolve_import("fmt", "main.go", &|_| false), None);
    }

    /// 纯 package 声明 / 纯常量 → Trivial（无实质内容、无危险）。
    #[test]
    fn detect_tier_trivial() {
        let mut adapter = GoAdapter::new().unwrap();
        assert_eq!(adapter.detect_tier("package main\n"), ModuleTier::Trivial);
        assert_eq!(
            adapter.detect_tier("package config\n\nconst Version = \"1.0\"\nvar Debug = false\n"),
            ModuleTier::Trivial
        );
    }

    /// 普通函数/类型定义、无危险信号 → Standard。
    #[test]
    fn detect_tier_standard() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n\ntype Point struct {\n\tX int\n\tY int\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Standard);
    }

    /// goroutine → Full。
    #[test]
    fn detect_tier_goroutine_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc run() {\n\tgo work()\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// channel（send + chan 类型）→ Full。
    #[test]
    fn detect_tier_channel_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc pipe(ch chan int) {\n\tch <- 1\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// select → Full。
    #[test]
    fn detect_tier_select_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nfunc pick(a, b chan int) {\n\tselect {\n\tcase <-a:\n\tcase <-b:\n\t}\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Full);
    }

    /// import "reflect" / "unsafe" / "C"（cgo）→ Full。
    #[test]
    fn detect_tier_danger_imports_are_full() {
        let mut adapter = GoAdapter::new().unwrap();
        for pkg in ["reflect", "unsafe", "C"] {
            let src = format!("package m\n\nimport \"{pkg}\"\n\nfunc f() {{}}\n");
            assert_eq!(
                adapter.detect_tier(&src),
                ModuleTier::Full,
                "import {pkg} 应判 Full"
            );
        }
        // 分组 import 形式也应命中。
        let grouped =
            "package m\n\nimport (\n\t\"fmt\"\n\t\"reflect\"\n)\n\nfunc f() { fmt.Println() }\n";
        assert_eq!(adapter.detect_tier(grouped), ModuleTier::Full);
    }

    /// 无害 import（fmt/strings）不触发危险 → Standard。
    #[test]
    fn detect_tier_safe_imports_not_full() {
        let mut adapter = GoAdapter::new().unwrap();
        let src = "package m\n\nimport \"fmt\"\n\nfunc greet() {\n\tfmt.Println(\"hi\")\n}\n";
        assert_eq!(adapter.detect_tier(src), ModuleTier::Standard);
    }

    /// 语法错误 → 保守 Full。
    #[test]
    fn detect_tier_syntax_error_is_full() {
        let mut adapter = GoAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_tier("package m\n\nfunc broken( {\n"),
            ModuleTier::Full
        );
    }

    /// 含 go.mod 的目录探测为源码根 Some(".")（探测成功，不触发 fallback warning）。
    #[test]
    fn detect_source_root_with_go_mod_returns_dot() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/x\n").unwrap();
        let adapter = GoAdapter::new().unwrap();
        assert_eq!(
            adapter.detect_source_root(tmp.path()),
            Some(".".to_string())
        );
    }

    /// 无 go.mod 且无 src/ 时返回 None（回退由调用方处理）。
    #[test]
    fn detect_source_root_without_go_mod_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let adapter = GoAdapter::new().unwrap();
        assert_eq!(adapter.detect_source_root(Path::new(tmp.path())), None);
    }
}
