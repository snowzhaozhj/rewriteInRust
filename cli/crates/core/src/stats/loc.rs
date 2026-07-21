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

/// 各源语言测试文件的 `.gitignore` 风格 glob 模式。
///
/// 用于「翻译对照」场景（如 `stats quality` 的 `project_loc_ratio`）排除源侧测试代码：
/// 翻译只对照非测试源码，测试文件不应进 LOC 比分母，否则稀释比率（见 issue #78）。
/// 注意：这些模式仅覆盖**命名约定**（各语言主流测试文件命名），无法识别放在
/// 独立 `tests/` 目录但不带命名标记的测试；后者需目录级配置，超出本常量范围。
pub const TEST_FILE_PATTERNS: &[&str] = &[
    // Go
    "**/*_test.go",
    // TypeScript / JavaScript
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/*.spec.ts",
    "**/*.spec.tsx",
    "**/*.test.js",
    "**/*.spec.js",
    // Python
    "**/test_*.py",
    "**/*_test.py",
    "**/conftest.py",
    // C
    "**/*_test.c",
    "**/test_*.c",
];

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
///
/// 纯数据结构体，字段 `pub`（Rust 惯例）。totals 经 [`LocReport::from_languages`]
/// 由 `by_language` 累加派生——把累加逻辑收成单一入口，避免各处手动累加分散出错。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocReport {
    /// 统计根目录（字符串形式，便于序列化）。
    pub root: String,
    /// 总文件数（派生自 `by_language`）。
    pub files: usize,
    /// 总代码行数（派生自 `by_language`）。
    pub code: u64,
    /// 总注释行数（派生自 `by_language`）。
    pub comments: u64,
    /// 总空白行数（派生自 `by_language`）。
    pub blanks: u64,
    /// 按 tokei 语言名分组的明细（BTreeMap 保证确定性排序）。
    pub by_language: BTreeMap<String, LocLang>,
}

impl LocReport {
    /// 由按语言明细构造报告，totals 全部从 `by_language` 累加派生（保证一致）。
    pub fn from_languages(root: String, by_language: BTreeMap<String, LocLang>) -> Self {
        let mut files = 0usize;
        let mut code = 0u64;
        let mut comments = 0u64;
        let mut blanks = 0u64;
        for lang in by_language.values() {
            files += lang.files;
            code += lang.code;
            comments += lang.comments;
            blanks += lang.blanks;
        }
        Self {
            root,
            files,
            code,
            comments,
            blanks,
            by_language,
        }
    }
}

/// 用 tokei 统计 `root` 目录下所有识别语言的代码行数（含 Rust）。
///
/// 与 [`crate::profile::detect`] 的语言扫描不同：profile 只保留待迁移的**源语言**
/// （TS/Py/C/Go）以评估迁移规模；本函数统计**全部**语言（含 Rust 目标代码），
/// 因 `stats loc` 需同时度量源码与生成的 Rust 代码体量。**统计全部文件（含测试）**——
/// `stats loc`/`stats compare` 度量磁盘真实体量，不排除测试。翻译对照场景需排除
/// 测试请用 [`count_loc_excluding_tests`]。
///
/// 目录不存在返回 `MigrateError::FileNotFound`。
pub fn count_loc(root: &Path) -> Result<LocReport> {
    count_loc_with_ignored(root, &[])
}

/// 与 [`count_loc`] 相同，但额外排除各语言测试文件（[`TEST_FILE_PATTERNS`]）。
///
/// 用于「翻译对照」场景（`stats quality` 的 `project_loc_ratio`）：翻译只对照非测试
/// 源码，测试文件计入 LOC 分母会稀释迁移体量比率（见 issue #78）。
pub fn count_loc_excluding_tests(root: &Path) -> Result<LocReport> {
    count_loc_with_ignored(root, TEST_FILE_PATTERNS)
}

/// `count_loc` 的实现内核：在默认排除目录之上追加 `extra_ignored` 模式
/// （`.gitignore` 语法）后用 tokei 统计。
fn count_loc_with_ignored(root: &Path, extra_ignored: &[&str]) -> Result<LocReport> {
    if !root.exists() {
        return Err(MigrateError::FileNotFound(root.to_path_buf()));
    }

    let mut ignored: Vec<&str> = EXCLUDED_DIRS.to_vec();
    ignored.extend_from_slice(extra_ignored);

    let mut languages = Languages::new();
    let config = Config::default();
    languages.get_statistics(&[root.to_string_lossy().as_ref()], &ignored, &config);

    let mut by_language: BTreeMap<String, LocLang> = BTreeMap::new();
    for (lang_type, lang) in &languages {
        if lang.is_empty() {
            continue;
        }
        by_language.insert(
            lang_type.to_string(),
            LocLang {
                files: lang.reports.len(),
                code: lang.code as u64,
                comments: lang.comments as u64,
                blanks: lang.blanks as u64,
            },
        );
    }

    // totals 由 by_language 派生，避免手动累加与明细脱节。
    Ok(LocReport::from_languages(
        root.to_string_lossy().into_owned(),
        by_language,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_from_languages_derives_consistent_totals_and_json_shape() {
        let mut by_lang = BTreeMap::new();
        by_lang.insert(
            "Rust".to_owned(),
            LocLang {
                files: 2,
                code: 10,
                comments: 3,
                blanks: 1,
            },
        );
        by_lang.insert(
            "TypeScript".to_owned(),
            LocLang {
                files: 1,
                code: 5,
                comments: 0,
                blanks: 2,
            },
        );
        let report = LocReport::from_languages("/tmp/x".to_owned(), by_lang);
        // totals 为明细之和。
        assert_eq!(report.files, 3);
        assert_eq!(report.code, 15);
        assert_eq!(report.comments, 3);
        assert_eq!(report.blanks, 3);
        // JSON 顶层字段名保持 files/code/comments/blanks（私有派生字段不改输出契约）。
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["files"], 3);
        assert_eq!(json["code"], 15);
        assert_eq!(json["comments"], 3);
        assert_eq!(json["blanks"], 3);
        assert!(json["by_language"].is_object());
    }

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

    #[test]
    fn test_count_loc_excluding_tests_drops_test_files() {
        let dir = tempfile::tempdir().unwrap();
        // 非测试源码。
        fs::write(
            dir.path().join("humanize.go"),
            "package h\nfunc F() int {\n\treturn 1\n}\n",
        )
        .unwrap();
        // Go 测试文件——命中 **/*_test.go，应被 excluding_tests 排除。
        fs::write(
            dir.path().join("humanize_test.go"),
            "package h\nfunc TestF(t *testing.T) {\n\t_ = F()\n\t_ = F()\n}\n",
        )
        .unwrap();

        let all = count_loc(dir.path()).unwrap();
        let no_tests = count_loc_excluding_tests(dir.path()).unwrap();

        // 全量统计包含两个文件；排除测试后只剩非测试文件，code 行更少。
        assert_eq!(all.by_language["Go"].files, 2);
        assert_eq!(no_tests.by_language["Go"].files, 1);
        assert!(
            no_tests.code < all.code,
            "排除测试后 code 应减少：all={} no_tests={}",
            all.code,
            no_tests.code
        );
    }

    #[test]
    fn test_count_loc_excluding_tests_keeps_non_test_files() {
        let dir = tempfile::tempdir().unwrap();
        // 纯非测试源码——两种统计结果应完全一致。
        fs::write(
            dir.path().join("lib.go"),
            "package l\nfunc A() {}\nfunc B() {}\n",
        )
        .unwrap();
        fs::write(dir.path().join("mod.ts"), "export const x = 1;\n").unwrap();

        let all = count_loc(dir.path()).unwrap();
        let no_tests = count_loc_excluding_tests(dir.path()).unwrap();
        assert_eq!(all.code, no_tests.code);
        assert_eq!(all.files, no_tests.files);
    }

    #[test]
    fn test_count_loc_excluding_tests_python_and_ts_conventions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("app.py"), "def f():\n    return 1\n").unwrap();
        fs::write(
            dir.path().join("test_app.py"),
            "def test_f():\n    assert f() == 1\n",
        )
        .unwrap();
        fs::write(dir.path().join("comp.ts"), "export const c = 1;\n").unwrap();
        fs::write(
            dir.path().join("comp.spec.ts"),
            "it('c', () => {\n  expect(c).toBe(1);\n});\n",
        )
        .unwrap();

        let no_tests = count_loc_excluding_tests(dir.path()).unwrap();
        // Python：test_*.py 被排除，只剩 app.py（1 文件）。
        assert_eq!(no_tests.by_language["Python"].files, 1);
        // TS：*.spec.ts 被排除，只剩 comp.ts（1 文件）。
        assert_eq!(no_tests.by_language["TypeScript"].files, 1);
    }

    #[test]
    fn test_count_loc_excluding_tests_missing_root() {
        let err = count_loc_excluding_tests(Path::new("/tmp/不存在的目录/loc")).unwrap_err();
        assert!(matches!(err, MigrateError::FileNotFound(_)));
    }
}
