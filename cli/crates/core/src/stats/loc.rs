//! 源码/Rust 代码行数统计（嵌入 tokei），对应 `rustmigrate stats loc`。
//!
//! 设计（`06-plugin-structure.md` § CLI）：`stats loc` 统计源码和 Rust 代码行数。
//! 不同于 [`crate::stats::coverage`] 的「迁移进度」（按模块状态），本模块度量的是
//! 磁盘上的真实代码体量（code/comments/blanks 行数 + 按语言明细），供进度量化与
//! `stats compare`（M2）复用。

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use tokei::{Config, Languages};

use crate::error::{MigrateError, Result};
use crate::types::common::EXCLUDED_DIRS;

/// 单语言的行数明细。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocLang {
    /// 文件数。
    pub files: usize,
    /// 代码行数。
    pub code: u64,
    /// 注释行数。
    pub comments: u64,
    /// 空白行数。
    pub blanks: u64,
}

/// 一个目录的 LOC 统计报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocReport {
    /// 统计根目录（字符串形式，便于序列化）。
    pub root: String,
    /// 总文件数。
    pub files: usize,
    /// 总代码行数。
    pub code: u64,
    /// 总注释行数。
    pub comments: u64,
    /// 总空白行数。
    pub blanks: u64,
    /// 按 tokei 语言名分组的明细（BTreeMap 保证确定性排序）。
    pub by_language: BTreeMap<String, LocLang>,
}

/// 用 tokei 统计 `root` 目录下所有识别语言的代码行数（含 Rust）。
///
/// 与 [`crate::profile::detect`] 的语言扫描不同：profile 只保留待迁移的**源语言**
/// （TS/Py/C/Go）以评估迁移规模；本函数统计**全部**语言（含 Rust 目标代码），
/// 因 `stats loc` 需同时度量源码与生成的 Rust 代码体量。
///
/// 目录不存在返回 `MigrateError::FileNotFound`。
pub fn count_loc(root: &Path) -> Result<LocReport> {
    if !root.exists() {
        return Err(MigrateError::FileNotFound(root.to_path_buf()));
    }

    let mut languages = Languages::new();
    let config = Config::default();
    languages.get_statistics(&[root.to_string_lossy().as_ref()], EXCLUDED_DIRS, &config);

    let mut report = LocReport {
        root: root.to_string_lossy().into_owned(),
        files: 0,
        code: 0,
        comments: 0,
        blanks: 0,
        by_language: BTreeMap::new(),
    };

    for (lang_type, lang) in &languages {
        if lang.is_empty() {
            continue;
        }
        let files = lang.reports.len();
        let code = lang.code as u64;
        let comments = lang.comments as u64;
        let blanks = lang.blanks as u64;

        report.files += files;
        report.code += code;
        report.comments += comments;
        report.blanks += blanks;
        report.by_language.insert(
            lang_type.to_string(),
            LocLang {
                files,
                code,
                comments,
                blanks,
            },
        );
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_count_loc_missing_root() {
        let err = count_loc(Path::new("/tmp/不存在的目录/loc")).unwrap_err();
        assert!(matches!(err, MigrateError::FileNotFound(_)));
    }

    #[test]
    fn test_count_loc_counts_rust_and_ts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("a.rs"),
            "// comment\nfn main() {\n    let x = 1;\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("b.ts"), "export const y = 2;\n").unwrap();
        let report = count_loc(dir.path()).unwrap();
        assert!(report.files >= 2);
        assert!(report.code >= 4);
        // Rust 与 TypeScript 均出现在按语言明细中。
        assert!(report.by_language.contains_key("Rust"));
        assert!(report.by_language.contains_key("TypeScript"));
        assert!(report.by_language["Rust"].comments >= 1);
    }
}
