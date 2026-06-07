//! 语言检测 + 复杂度评估。
//!
//! 基于 tokei 扫描项目目录，按代码行数判定主语言和复杂度等级。

use std::path::Path;

use tokei::{Config, LanguageType, Languages};

use crate::error::{MigrateError, Result};
use crate::types::common::{Complexity, SourceLang};

/// 项目画像信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectProfile {
    /// 检测到的主语言。
    pub language: SourceLang,
    /// 总文件数。
    pub total_files: usize,
    /// 总代码行数。
    pub total_lines: u64,
    /// TypeScript 文件数（.ts + .tsx）。
    pub ts_files: usize,
    /// 复杂度等级（按行数阈值判定）。
    pub complexity: Complexity,
}

/// 将 tokei 的 `LanguageType` 映射到 `SourceLang`。
///
/// 仅映射当前支持的语言，其他语言返回 `None`。
fn map_language(lang: &LanguageType) -> Option<SourceLang> {
    match lang {
        LanguageType::TypeScript | LanguageType::Tsx => Some(SourceLang::TypeScript),
        LanguageType::Python => Some(SourceLang::Python),
        LanguageType::C | LanguageType::CHeader => Some(SourceLang::C),
        LanguageType::Go => Some(SourceLang::Go),
        _ => None,
    }
}

/// 根据代码行数判定复杂度等级。
///
/// - <1000 行: Simple
/// - 1000-5000 行: Moderate
/// - >5000 行: Complex
fn assess_complexity(lines: u64) -> Complexity {
    if lines < 1000 {
        Complexity::Simple
    } else if lines <= 5000 {
        Complexity::Moderate
    } else {
        Complexity::Complex
    }
}

/// 检测项目主语言。
///
/// 扫描 `root` 目录下所有文件，按代码行数最多的支持语言判定。
/// 如果没有检测到任何支持的语言，返回错误。
pub fn detect_language(root: &Path) -> Result<SourceLang> {
    if !root.exists() {
        return Err(MigrateError::FileNotFound(root.to_path_buf()));
    }

    let mut languages = Languages::new();
    let config = Config::default();
    let excluded: &[&str] = &["node_modules", "target", "dist", "build", ".git"];
    languages.get_statistics(&[root.to_string_lossy().as_ref()], excluded, &config);

    // 按代码行数聚合到 SourceLang
    let mut lang_lines: Vec<(SourceLang, u64)> = Vec::new();
    for (lang_type, lang) in &languages {
        if lang.is_empty() {
            continue;
        }
        if let Some(source_lang) = map_language(lang_type) {
            // 查找是否已有该 SourceLang 的记录（如 TypeScript 和 Tsx 合并）
            if let Some(entry) = lang_lines.iter_mut().find(|(sl, _)| *sl == source_lang) {
                entry.1 += lang.code as u64;
            } else {
                lang_lines.push((source_lang, lang.code as u64));
            }
        }
    }

    lang_lines
        .into_iter()
        .max_by_key(|(_, lines)| *lines)
        .map(|(lang, _)| lang)
        .ok_or_else(|| MigrateError::Config("未检测到支持的源语言".to_string()))
}

/// 完整项目画像分析。
///
/// 返回 `ProjectProfile`，包含主语言、文件数、行数和复杂度。
pub fn profile_project(root: &Path) -> Result<ProjectProfile> {
    if !root.exists() {
        return Err(MigrateError::FileNotFound(root.to_path_buf()));
    }

    let mut languages = Languages::new();
    let config = Config::default();
    let excluded: &[&str] = &["node_modules", "target", "dist", "build", ".git"];
    languages.get_statistics(&[root.to_string_lossy().as_ref()], excluded, &config);

    let mut total_files: usize = 0;
    let mut total_lines: u64 = 0;
    let mut ts_files: usize = 0;

    // 按代码行数聚合到 SourceLang
    let mut lang_lines: Vec<(SourceLang, u64)> = Vec::new();

    for (lang_type, lang) in &languages {
        if lang.is_empty() {
            continue;
        }
        let file_count = lang.reports.len();
        let code_lines = lang.code as u64;

        total_files += file_count;
        total_lines += code_lines;

        // 统计 TS 文件数
        if matches!(lang_type, LanguageType::TypeScript | LanguageType::Tsx) {
            ts_files += file_count;
        }

        if let Some(source_lang) = map_language(lang_type) {
            if let Some(entry) = lang_lines.iter_mut().find(|(sl, _)| *sl == source_lang) {
                entry.1 += code_lines;
            } else {
                lang_lines.push((source_lang, code_lines));
            }
        }
    }

    let language = lang_lines
        .into_iter()
        .max_by_key(|(_, lines)| *lines)
        .map(|(lang, _)| lang)
        .ok_or_else(|| MigrateError::Config("未检测到支持的源语言".to_string()))?;

    let complexity = assess_complexity(total_lines);

    Ok(ProjectProfile {
        language,
        total_files,
        total_lines,
        ts_files,
        complexity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// 辅助函数：创建含有指定内容的文件。
    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    /// 辅助函数：生成指定行数的 TypeScript 代码。
    fn ts_code(lines: usize) -> String {
        (0..lines)
            .map(|i| format!("const x{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_detect_language_typescript() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/index.ts", &ts_code(50));
        write_file(tmp.path(), "src/utils.ts", &ts_code(30));

        let lang = detect_language(tmp.path()).unwrap();
        assert_eq!(lang, SourceLang::TypeScript);
    }

    #[test]
    fn test_detect_language_python() {
        let tmp = TempDir::new().unwrap();
        // 写一些 Python 代码，行数多于 TS
        let py_code: String = (0..100)
            .map(|i| format!("x{i} = {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        write_file(tmp.path(), "main.py", &py_code);
        write_file(tmp.path(), "helper.py", &py_code);
        // 少量 TS
        write_file(tmp.path(), "index.ts", &ts_code(10));

        let lang = detect_language(tmp.path()).unwrap();
        assert_eq!(lang, SourceLang::Python);
    }

    #[test]
    fn test_detect_language_nonexistent_dir() {
        let result = detect_language(Path::new("/tmp/nonexistent_dir_rustmigrate_test"));
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_language_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let result = detect_language(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_assess_complexity() {
        assert_eq!(assess_complexity(0), Complexity::Simple);
        assert_eq!(assess_complexity(999), Complexity::Simple);
        assert_eq!(assess_complexity(1000), Complexity::Moderate);
        assert_eq!(assess_complexity(5000), Complexity::Moderate);
        assert_eq!(assess_complexity(5001), Complexity::Complex);
        assert_eq!(assess_complexity(100_000), Complexity::Complex);
    }

    #[test]
    fn test_profile_project_simple() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/index.ts", &ts_code(50));

        let profile = profile_project(tmp.path()).unwrap();
        assert_eq!(profile.language, SourceLang::TypeScript);
        assert!(profile.total_files >= 1);
        assert!(profile.total_lines >= 50);
        assert!(profile.ts_files >= 1);
        assert_eq!(profile.complexity, Complexity::Simple);
    }

    #[test]
    fn test_profile_project_nonexistent() {
        let result = profile_project(Path::new("/tmp/nonexistent_dir_rustmigrate_test"));
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_project_empty() {
        let tmp = TempDir::new().unwrap();
        let result = profile_project(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_map_language() {
        assert_eq!(
            map_language(&LanguageType::TypeScript),
            Some(SourceLang::TypeScript)
        );
        assert_eq!(
            map_language(&LanguageType::Tsx),
            Some(SourceLang::TypeScript)
        );
        assert_eq!(
            map_language(&LanguageType::Python),
            Some(SourceLang::Python)
        );
        assert_eq!(map_language(&LanguageType::C), Some(SourceLang::C));
        assert_eq!(map_language(&LanguageType::CHeader), Some(SourceLang::C));
        assert_eq!(map_language(&LanguageType::Go), Some(SourceLang::Go));
        assert_eq!(map_language(&LanguageType::Rust), None);
        assert_eq!(map_language(&LanguageType::Java), None);
    }
}
