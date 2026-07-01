//! Go 语言适配器骨架。
//!
//! M4 Sprint A 接线：language/can_handle/resolve_extensions/detect_source_root 已实现，
//! 解析逻辑（analyze_file/resolve_import/detect_tier/classify_file）Sprint C 补全。
//!
//! **骨架阶段不 panic 约定**：`create_adapter(Go)` 已接线，而 `detect_language` 会把
//! Go 项目自动识别为 Go——若骨架方法用 `todo!()`，Go 项目跑 graph build/populate 会
//! panic 崩进程（违反 CLI 统一 JSON 输出）。故未实现方法走优雅降级：唯一真正解析入口
//! `analyze_file` 返回 `Err(NotImplemented)`（由上层产出 `{status:error}` JSON）；不返回
//! `Result` 的 `detect_tier`/`classify_file` 返回保守默认值（Full / conservative），绝不 panic。

use std::path::Path;

use tree_sitter::Parser;

use crate::error::{MigrateError, Result};
use crate::types::common::SourceLang;
use crate::types::state::ModuleTier;

use super::{FileAnalysis, LanguageAdapter};

/// Go 语言适配器（基于 tree-sitter-go）。
pub struct GoAdapter {
    // M4 Sprint A 骨架阶段 analyze_file 走 NotImplemented、parser 尚未消费。
    // Sprint C（GO-01~06）实现解析逻辑后即移除此 allow。
    #[allow(dead_code)]
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

    fn detect_tier(&mut self, _source: &str) -> ModuleTier {
        // 骨架阶段：保守返回 Full（不降档），与 trait 约定「解析失败返回 Full」一致，绝不 panic。
        // Sprint C GO-01 实现 Go 复杂度分档（goroutine/channel/cgo/unsafe.Pointer 等危险信号）。
        ModuleTier::Full
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 骨架阶段所有方法都不 panic（回归：防 todo!() 让 Go 项目 graph build 崩进程）。
    #[test]
    fn skeleton_methods_do_not_panic() {
        let mut adapter = GoAdapter::new().unwrap();
        // analyze_file 优雅报错而非 panic。
        let err = adapter
            .analyze_file("package main\n", "main.go")
            .unwrap_err();
        assert!(
            err.to_string().contains("尚未实现"),
            "analyze_file 应返回 NotImplemented，实际: {err}"
        );
        // detect_tier 保守返回 Full，不 panic。
        assert_eq!(adapter.detect_tier("package main\n"), ModuleTier::Full);
        // classify_file（trait 默认）返回保守分类，不 panic、不判机械。
        let cls = adapter.classify_file("package main\n");
        assert!(cls.danger.is_empty());
        assert!(!cls.is_mechanical());
        // resolve_import 返回 None，不 panic。
        assert_eq!(adapter.resolve_import("fmt", "main.go", &|_| false), None);
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
