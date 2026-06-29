//! 语言检测 + 复杂度评估。
//!
//! 基于 tokei 扫描项目目录，按代码行数判定主语言和复杂度等级。

use std::collections::HashMap;
use std::path::Path;

use tokei::{Config, LanguageType, Languages};

use crate::error::{MigrateError, Result};
use crate::types::common::{Complexity, SourceLang, EXCLUDED_DIRS};

/// 单语言统计。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LangStats {
    /// 文件数。
    pub files: usize,
    /// 代码行数。
    pub lines: u64,
}

/// 项目画像信息。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectProfile {
    /// 检测到的主语言（代码行数最多的）。
    pub primary_language: SourceLang,
    /// 按语言分组的统计。
    pub languages: HashMap<SourceLang, LangStats>,
    /// 总文件数。
    pub total_files: usize,
    /// 总代码行数。
    pub total_lines: u64,
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

/// 在语言统计中选出主语言：代码行数最多者；行数相同时按语言名升序取稳定的唯一胜者
/// （避免 HashMap 迭代顺序导致平局时主语言不确定）。
fn pick_primary_language(lang_map: &HashMap<SourceLang, LangStats>) -> Option<SourceLang> {
    lang_map
        .iter()
        .max_by(|(la, sa), (lb, sb)| {
            sa.lines
                .cmp(&sb.lines)
                .then_with(|| lb.to_string().cmp(&la.to_string()))
        })
        .map(|(lang, _)| *lang)
}

/// 扫描项目目录，返回按语言分组的统计、总文件数和总行数。
fn scan_languages(root: &Path) -> Result<(HashMap<SourceLang, LangStats>, usize, u64)> {
    if !root.exists() {
        return Err(MigrateError::FileNotFound(root.to_path_buf()));
    }

    let mut languages = Languages::new();
    let config = Config::default();
    let excluded: &[&str] = EXCLUDED_DIRS;
    languages.get_statistics(&[root.to_string_lossy().as_ref()], excluded, &config);

    let mut total_files: usize = 0;
    let mut total_lines: u64 = 0;
    let mut lang_map: HashMap<SourceLang, LangStats> = HashMap::new();

    for (lang_type, lang) in &languages {
        if lang.is_empty() {
            continue;
        }
        let file_count = lang.reports.len();
        let code_lines = lang.code as u64;

        // 仅统计受支持的源语言：total_files/total_lines 反映"待迁移的源代码"规模，
        // 据此判定迁移复杂度。lockfile/JSON/Markdown 等非源文件不计入，否则会虚高复杂度。
        if let Some(source_lang) = map_language(lang_type) {
            total_files += file_count;
            total_lines += code_lines;

            let entry = lang_map
                .entry(source_lang)
                .or_insert(LangStats { files: 0, lines: 0 });
            entry.files += file_count;
            entry.lines += code_lines;
        }
    }

    Ok((lang_map, total_files, total_lines))
}

/// 检测项目主语言。
///
/// 扫描 `root` 目录下所有文件，按代码行数最多的支持语言判定。
/// 如果没有检测到任何支持的语言，返回错误。
pub fn detect_language(root: &Path) -> Result<SourceLang> {
    let (lang_map, _, _) = scan_languages(root)?;
    pick_primary_language(&lang_map)
        .ok_or_else(|| MigrateError::Config("未检测到支持的源语言".to_string()))
}

/// 源根探测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRootDetection {
    /// 推断出的 source_root（相对于项目根）。
    pub source_root: String,
    /// 探测依据说明。
    pub reason: &'static str,
}

/// 源文件扩展名（不含点）。
fn source_extensions(lang: SourceLang) -> &'static [&'static str] {
    match lang {
        SourceLang::TypeScript => &["ts", "tsx"],
        SourceLang::Python => &["py"],
        SourceLang::C => &["c", "h"],
        SourceLang::Go => &["go"],
    }
}

/// 目录下是否含有指定语言的源文件（非递归，仅直接子文件）。
fn dir_has_source_files(dir: &Path, lang: SourceLang) -> bool {
    let exts = source_extensions(lang);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
        .any(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| exts.contains(&ext))
        })
}

/// 目录下是否含有指定语言的源文件（递归检查子目录）。
fn dir_has_source_files_recursive(dir: &Path, lang: SourceLang) -> bool {
    if dir_has_source_files(dir, lang) {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .any(|e| dir_has_source_files_recursive(&e.path(), lang))
}

/// 探测项目的源码根目录。
///
/// 策略（按优先级）：
/// 1. `src/` 存在且含该语言源文件 → `"src"`
/// 2. Python 项目：找顶层含 `__init__.py` 的唯一包目录 → 该目录名
/// 3. 回退 `"."`
pub fn detect_source_root(project_root: &Path, lang: SourceLang) -> SourceRootDetection {
    let src_dir = project_root.join("src");
    if src_dir.is_dir() && dir_has_source_files_recursive(&src_dir, lang) {
        return SourceRootDetection {
            source_root: "src".to_string(),
            reason: "src/ 目录含源文件",
        };
    }

    if lang == SourceLang::Python {
        let packages: Vec<String> = std::fs::read_dir(project_root)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .filter(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                !name_str.starts_with('.')
                    && !EXCLUDED_DIRS.contains(&name_str.as_ref())
                    && e.path().join("__init__.py").exists()
            })
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        if packages.len() == 1 {
            return SourceRootDetection {
                source_root: packages.into_iter().next().unwrap(),
                reason: "唯一顶层 Python 包（含 __init__.py）",
            };
        }
    }

    SourceRootDetection {
        source_root: ".".to_string(),
        reason: "未找到 src/ 或语言特定包目录，回退项目根",
    }
}

/// 完整项目画像分析。
///
/// 返回 `ProjectProfile`，包含主语言、文件数、行数和复杂度。
pub fn profile_project(root: &Path) -> Result<ProjectProfile> {
    let (lang_map, total_files, total_lines) = scan_languages(root)?;

    let primary_language = pick_primary_language(&lang_map)
        .ok_or_else(|| MigrateError::Config("未检测到支持的源语言".to_string()))?;

    let complexity = assess_complexity(total_lines);

    Ok(ProjectProfile {
        primary_language,
        languages: lang_map,
        total_files,
        total_lines,
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
        assert_eq!(profile.primary_language, SourceLang::TypeScript);
        assert!(profile.total_files >= 1);
        assert!(profile.total_lines >= 50);
        let ts = profile.languages.get(&SourceLang::TypeScript).unwrap();
        assert!(ts.files >= 1);
        assert!(ts.lines >= 50);
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
    fn detect_source_root_ts_with_src() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/index.ts", &ts_code(10));
        let det = detect_source_root(tmp.path(), SourceLang::TypeScript);
        assert_eq!(det.source_root, "src");
    }

    #[test]
    fn detect_source_root_ts_no_src() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "index.ts", &ts_code(10));
        let det = detect_source_root(tmp.path(), SourceLang::TypeScript);
        assert_eq!(det.source_root, ".");
    }

    #[test]
    fn detect_source_root_python_flat_package() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "mypackage/__init__.py", "");
        write_file(tmp.path(), "mypackage/core.py", "x = 1");
        write_file(tmp.path(), "setup.py", "from setuptools import setup");
        let det = detect_source_root(tmp.path(), SourceLang::Python);
        assert_eq!(det.source_root, "mypackage");
    }

    #[test]
    fn detect_source_root_python_src_layout() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/mypackage/__init__.py", "");
        write_file(tmp.path(), "src/mypackage/core.py", "x = 1");
        let det = detect_source_root(tmp.path(), SourceLang::Python);
        assert_eq!(det.source_root, "src");
    }

    #[test]
    fn detect_source_root_python_multiple_packages_fallback() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "pkg_a/__init__.py", "");
        write_file(tmp.path(), "pkg_a/a.py", "x = 1");
        write_file(tmp.path(), "pkg_b/__init__.py", "");
        write_file(tmp.path(), "pkg_b/b.py", "y = 2");
        let det = detect_source_root(tmp.path(), SourceLang::Python);
        assert_eq!(det.source_root, ".", "多顶层包应回退 \".\"");
    }

    #[test]
    fn detect_source_root_python_no_init_py() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "scripts/run.py", "print('hello')");
        let det = detect_source_root(tmp.path(), SourceLang::Python);
        assert_eq!(det.source_root, ".", "无 __init__.py 应回退 \".\"");
    }

    #[test]
    fn detect_source_root_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let det = detect_source_root(tmp.path(), SourceLang::TypeScript);
        assert_eq!(det.source_root, ".");
    }

    #[test]
    fn detect_source_root_src_exists_but_no_source_files() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "src/README.md", "# Hello");
        let det = detect_source_root(tmp.path(), SourceLang::TypeScript);
        assert_eq!(det.source_root, ".", "src/ 无源文件应回退");
    }

    #[test]
    fn detect_source_root_excludes_hidden_and_vendor() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), ".venv/__init__.py", "");
        write_file(tmp.path(), ".venv/lib.py", "x = 1");
        write_file(tmp.path(), "main.py", "print('hello')");
        let det = detect_source_root(tmp.path(), SourceLang::Python);
        assert_eq!(det.source_root, ".", ".venv 不应被当作源包");
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
