//! Go 语言适配器骨架。
//!
//! M4 Sprint A 接线：language/can_handle/resolve_extensions 已实现，
//! 其余方法 Sprint C 逐步补全。

use std::path::Path;

use tree_sitter::Parser;

use crate::error::{MigrateError, Result};
use crate::types::common::SourceLang;
use crate::types::state::ModuleTier;

use super::{FileAnalysis, FileClassification, LanguageAdapter};

/// Go 语言适配器（基于 tree-sitter-go）。
pub struct GoAdapter {
    // M4 Sprint A 骨架阶段 analyze_file/detect_tier 等仍为 todo!()，parser 尚未消费。
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
        todo!("M4 Sprint C: GO-04~06 实现 Go 单文件分析")
    }

    fn resolve_import(
        &self,
        _specifier: &str,
        _current_file: &str,
        _exists: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        todo!("M4 Sprint C: GO-03 实现 Go 包 resolve")
    }

    fn detect_tier(&mut self, _source: &str) -> ModuleTier {
        todo!("M4 Sprint C: GO-01 实现 Go 复杂度分档")
    }

    fn classify_file(&mut self, _source: &str) -> FileClassification {
        todo!("M4 Sprint C: GO-02 实现 Go 文件分类")
    }

    fn detect_source_root(&self, project_root: &Path) -> Option<String> {
        // Go 项目：含 go.mod 的目录为 module 根，源码通常在根目录
        if project_root.join("go.mod").exists() {
            return None; // 根目录即源码根
        }
        // 回退到默认 src/ 检查
        let src_dir = project_root.join("src");
        if src_dir.is_dir() && super::dir_has_source_files(&src_dir, self.resolve_extensions(), 5) {
            return Some("src".to_string());
        }
        None
    }
}
