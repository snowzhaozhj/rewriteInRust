//! 结构化合并器。
//!
//! 多 worktree 并行翻译的产出合并回主 `rust_root`，按文件类型分策略：
//! - **Cargo.toml**：使用 `toml_edit` 做语义级 union 合并（保留注释和格式）
//! - **lib.rs**：`mod` 声明追加（去重）
//! - **own files**：直接复制（检测同名碰撞）

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};

/// 合并结果。
pub struct MergeResult {
    /// 合并后的文件内容。
    pub content: String,
    /// 合并过程中的 warning（如字段冲突等）。
    pub warnings: Vec<String>,
}

/// Cargo.toml 结构化 union 合并。
///
/// 将 overlay（worktree 产出）中新增的依赖、dev-dependencies、features 合并到 base（主 Cargo.toml）。
/// 合并是追加式：只加不删、不改已有项（除同名依赖版本冲突取 overlay 并记 warning）。
pub fn merge_cargo_toml(base: &str, overlay: &str) -> Result<MergeResult> {
    let mut base_doc = base
        .parse::<toml_edit::DocumentMut>()
        .map_err(MigrateError::TomlEdit)?;
    let overlay_doc = overlay
        .parse::<toml_edit::DocumentMut>()
        .map_err(MigrateError::TomlEdit)?;

    let mut warnings = Vec::new();

    // 合并 [dependencies] 和 [dev-dependencies]
    for section in &["dependencies", "dev-dependencies"] {
        merge_dep_table(&mut base_doc, &overlay_doc, section, &mut warnings)?;
    }

    // 合并 [features]
    merge_features(&mut base_doc, &overlay_doc, &mut warnings)?;

    Ok(MergeResult {
        content: base_doc.to_string(),
        warnings,
    })
}

/// 合并依赖表（dependencies / dev-dependencies）。
///
/// overlay 有而 base 没有的依赖 → 追加到 base。
/// 同名依赖版本不同 → 取 overlay 版本并记 warning。
fn merge_dep_table(
    base_doc: &mut toml_edit::DocumentMut,
    overlay_doc: &toml_edit::DocumentMut,
    section: &str,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let overlay_table = match overlay_doc.get(section).and_then(|i| i.as_table()) {
        Some(t) => t,
        None => return Ok(()),
    };

    // 确保 base 中存在该 section
    if base_doc.get(section).is_none() {
        base_doc[section] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    for (key, overlay_item) in overlay_table.iter() {
        let base_table = base_doc[section]
            .as_table_mut()
            .ok_or_else(|| MigrateError::Merge(format!("[{section}] section 不是 table")))?;
        if base_table.contains_key(key) {
            // 同名依赖已存在——检查版本是否不同
            let base_item = &base_table[key];
            let base_ver = extract_version(base_item);
            let overlay_ver = extract_version(overlay_item);

            if base_ver != overlay_ver {
                // 版本冲突：取 overlay 版本
                warnings.push(format!(
                    "[{section}] 依赖 `{key}` 版本冲突: base={}, overlay={} → 采用 overlay",
                    base_ver.as_deref().unwrap_or("?"),
                    overlay_ver.as_deref().unwrap_or("?"),
                ));
                base_table.insert(key, overlay_item.clone());
            }
            // 版本相同则跳过，保留 base 原样
        } else {
            // 新增依赖
            base_table.insert(key, overlay_item.clone());
        }
    }
    Ok(())
}

/// 从依赖 Item 中提取版本字符串。
///
/// 支持两种格式：
/// - 简单字符串：`"1.0"`
/// - 表格形式：`{ version = "1.0", features = [...] }`
fn extract_version(item: &toml_edit::Item) -> Option<String> {
    match item {
        toml_edit::Item::Value(v) => match v {
            toml_edit::Value::String(s) => Some(s.value().to_string()),
            toml_edit::Value::InlineTable(t) => t
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            _ => None,
        },
        toml_edit::Item::Table(t) => t
            .get("version")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// 合并 [features] 表。
///
/// overlay 中有而 base 没有的 feature → 追加。
/// 同名 feature → 数组 union（去重追加 overlay 中 base 没有的条目）。
fn merge_features(
    base_doc: &mut toml_edit::DocumentMut,
    overlay_doc: &toml_edit::DocumentMut,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let overlay_features = match overlay_doc.get("features").and_then(|i| i.as_table()) {
        Some(t) => t,
        None => return Ok(()),
    };

    if base_doc.get("features").is_none() {
        base_doc["features"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    for (key, overlay_item) in overlay_features.iter() {
        let base_features = base_doc["features"]
            .as_table_mut()
            .ok_or_else(|| MigrateError::Merge("[features] section 不是 table".into()))?;
        if base_features.contains_key(key) {
            // 同名 feature：做数组 union
            let base_arr = match base_features.get_mut(key).and_then(|i| i.as_array_mut()) {
                Some(a) => a,
                None => continue,
            };
            let overlay_arr = match overlay_item.as_array() {
                Some(a) => a,
                None => continue,
            };

            // 收集 base 中已有的值
            let existing: HashSet<String> = base_arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            let mut added = Vec::new();
            for v in overlay_arr.iter() {
                if let Some(s) = v.as_str() {
                    if !existing.contains(s) {
                        base_arr.push(s);
                        added.push(s.to_string());
                    }
                }
            }

            if !added.is_empty() {
                warnings.push(format!(
                    "[features] `{key}` 追加了条目: {}",
                    added.join(", ")
                ));
            }
        } else {
            // 新增 feature
            base_features.insert(key, overlay_item.clone());
        }
    }
    Ok(())
}

/// lib.rs mod 声明追加合并。
///
/// 将 overlay（worktree 的 lib.rs）中新增的 `mod xxx;` / `pub mod xxx;` 声明追加到 base 末尾。
/// 不重复追加已存在的 mod 声明。
pub fn merge_lib_rs(base: &str, overlay: &str) -> Result<MergeResult> {
    let base_mods = parse_mod_declarations(base);
    let overlay_mods = parse_mod_declarations(overlay);

    // 收集 base 中已有的模块名
    let existing: HashSet<&str> = base_mods.iter().map(|m| m.name.as_str()).collect();

    // 找出 overlay 中新增的 mod 声明
    let mut new_mods: Vec<&ModDecl> = Vec::new();
    for m in &overlay_mods {
        if !existing.contains(m.name.as_str()) {
            new_mods.push(m);
        }
    }

    let mut warnings = Vec::new();
    let mut content = base.to_string();

    if !new_mods.is_empty() {
        // 确保末尾有换行
        if !content.ends_with('\n') {
            content.push('\n');
        }

        for m in &new_mods {
            content.push_str(&m.full_line);
            content.push('\n');
        }

        warnings.push(format!(
            "追加了 {} 个 mod 声明: {}",
            new_mods.len(),
            new_mods
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(MergeResult { content, warnings })
}

/// mod 声明信息。
struct ModDecl {
    /// 模块名（不含 pub/mod/分号）。
    name: String,
    /// 原始完整行文本（如 `pub mod foo;`）。
    full_line: String,
}

/// 解析文本中的 mod 声明。
///
/// 匹配 `mod xxx;` 和 `pub mod xxx;`（忽略 `mod xxx { ... }` 块形式）。
fn parse_mod_declarations(source: &str) -> Vec<ModDecl> {
    let mut result = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        // 匹配 `mod name;` 或 `pub mod name;`（可能含 `pub(crate)` 等）
        if let Some(name) = extract_mod_name(trimmed) {
            result.push(ModDecl {
                name,
                full_line: trimmed.to_string(),
            });
        }
    }
    result
}

/// 从单行中提取 mod 声明的模块名。
///
/// 支持格式：`mod foo;`、`pub mod foo;`、`pub(crate) mod foo;`
/// 不匹配块形式（如 `mod foo { ... }`）。
fn extract_mod_name(line: &str) -> Option<String> {
    let line = line.trim();

    // 必须以分号结尾（排除块形式）
    if !line.ends_with(';') {
        return None;
    }

    // 去掉末尾分号
    let line = line.trim_end_matches(';').trim();

    // 找到 "mod" 关键字的位置
    let mod_idx = if line.starts_with("mod ") {
        Some(4)
    } else if let Some(rest) = line.strip_prefix("pub") {
        let rest = rest.trim_start();
        if let Some(rest) = rest.strip_prefix("mod ") {
            Some(line.len() - rest.len())
        } else if rest.starts_with('(') {
            // pub(crate) mod xxx 等
            rest.find(')').and_then(|close| {
                let after_vis = rest[close + 1..].trim_start();
                after_vis
                    .strip_prefix("mod ")
                    .map(|rest| line.len() - rest.len())
            })
        } else {
            None
        }
    } else {
        None
    };

    mod_idx.map(|idx| line[idx..].trim().to_string())
}

/// own files 复制（worktree 的模块文件复制到主目录）。
///
/// 将 `own_files` 列表中的文件从 `worktree_root` 复制到 `target_root`，
/// 保持相对路径结构。如果目标文件已存在，视为同名碰撞并记 warning（不覆盖）。
///
/// 返回碰撞 warnings 列表。
pub fn copy_own_files(
    own_files: &[PathBuf],
    worktree_root: &Path,
    target_root: &Path,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    for file in own_files {
        // 计算相对路径
        let rel_path = file.strip_prefix(worktree_root).map_err(|_| {
            MigrateError::Merge(format!(
                "文件 {} 不在 worktree_root {} 下",
                file.display(),
                worktree_root.display()
            ))
        })?;

        let src = worktree_root.join(rel_path);
        let dst = target_root.join(rel_path);

        if dst.exists() {
            // 同名碰撞——不覆盖，记 warning
            warnings.push(format!(
                "同名碰撞: {} 已存在于目标目录，跳过复制",
                rel_path.display()
            ));
            continue;
        }

        // 确保目标目录存在
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(&src, &dst)?;
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Cargo.toml 合并测试
    // =========================================================================

    #[test]
    fn test_merge_cargo_toml_new_deps() {
        let base = r#"[package]
name = "my-project"
version = "0.1.0"

# 项目依赖
[dependencies]
serde = "1"
"#;

        let overlay = r#"[package]
name = "my-project"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = { version = "1", features = ["full"] }
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();

        // 新增的 tokio 应该被合并进去
        let merged_doc = result.content.parse::<toml_edit::DocumentMut>().unwrap();
        let deps = merged_doc["dependencies"].as_table().unwrap();
        assert!(deps.contains_key("serde"));
        assert!(deps.contains_key("tokio"));

        // 应保留 base 的注释
        assert!(result.content.contains("# 项目依赖"));

        // 无冲突 warning
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_merge_cargo_toml_version_conflict() {
        let base = r#"[dependencies]
serde = "1.0"
"#;

        let overlay = r#"[dependencies]
serde = "1.5"
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();

        // 应取 overlay 版本
        let merged_doc = result.content.parse::<toml_edit::DocumentMut>().unwrap();
        let serde_ver = merged_doc["dependencies"]["serde"].as_str().unwrap();
        assert_eq!(serde_ver, "1.5");

        // 应有冲突 warning
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("版本冲突"));
        assert!(result.warnings[0].contains("serde"));
    }

    #[test]
    fn test_merge_cargo_toml_same_version_no_warning() {
        let base = r#"[dependencies]
serde = "1.0"
"#;

        let overlay = r#"[dependencies]
serde = "1.0"
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_merge_cargo_toml_features_union() {
        let base = r#"[features]
default = ["json"]
json = ["serde_json"]
"#;

        let overlay = r#"[features]
default = ["json", "yaml"]
yaml = ["serde_yaml"]
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();
        let merged_doc = result.content.parse::<toml_edit::DocumentMut>().unwrap();
        let features = merged_doc["features"].as_table().unwrap();

        // 新增的 yaml feature 应该出现
        assert!(features.contains_key("yaml"));

        // default 应该包含 json 和 yaml（union）
        let default_arr = features["default"].as_array().unwrap();
        let default_vals: Vec<&str> = default_arr.iter().filter_map(|v| v.as_str()).collect();
        assert!(default_vals.contains(&"json"));
        assert!(default_vals.contains(&"yaml"));
    }

    #[test]
    fn test_merge_cargo_toml_dev_deps() {
        let base = r#"[dev-dependencies]
tempfile = "3"
"#;

        let overlay = r#"[dev-dependencies]
tempfile = "3"
insta = "1"
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();
        let merged_doc = result.content.parse::<toml_edit::DocumentMut>().unwrap();
        let dev_deps = merged_doc["dev-dependencies"].as_table().unwrap();
        assert!(dev_deps.contains_key("tempfile"));
        assert!(dev_deps.contains_key("insta"));
    }

    #[test]
    fn test_merge_cargo_toml_overlay_has_new_section() {
        // base 没有 dev-dependencies，overlay 有
        let base = r#"[dependencies]
serde = "1"
"#;

        let overlay = r#"[dependencies]
serde = "1"

[dev-dependencies]
tempfile = "3"
"#;

        let result = merge_cargo_toml(base, overlay).unwrap();
        let merged_doc = result.content.parse::<toml_edit::DocumentMut>().unwrap();
        assert!(merged_doc
            .get("dev-dependencies")
            .and_then(|i| i.as_table())
            .unwrap()
            .contains_key("tempfile"));
    }

    // =========================================================================
    // lib.rs 合并测试
    // =========================================================================

    #[test]
    fn test_merge_lib_rs_new_mods() {
        let base = "pub mod graph;\npub mod error;\n";
        let overlay = "pub mod graph;\npub mod error;\npub mod merge;\nmod utils;\n";

        let result = merge_lib_rs(base, overlay).unwrap();
        assert!(result.content.contains("pub mod merge;"));
        assert!(result.content.contains("mod utils;"));
        // 原有的不应重复
        assert_eq!(result.content.matches("pub mod graph;").count(), 1);

        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("merge"));
        assert!(result.warnings[0].contains("utils"));
    }

    #[test]
    fn test_merge_lib_rs_no_duplicate() {
        let base = "pub mod graph;\npub mod error;\n";
        let overlay = "pub mod graph;\npub mod error;\n";

        let result = merge_lib_rs(base, overlay).unwrap();
        assert_eq!(result.content, base);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_merge_lib_rs_pub_crate_mod() {
        let base = "pub mod graph;\n";
        let overlay = "pub mod graph;\npub(crate) mod internal;\n";

        let result = merge_lib_rs(base, overlay).unwrap();
        assert!(result.content.contains("pub(crate) mod internal;"));
    }

    #[test]
    fn test_merge_lib_rs_preserves_non_mod_content() {
        let base = "//! 核心库。\n\npub mod graph;\npub mod error;\n";
        let overlay = "pub mod graph;\npub mod error;\npub mod merge;\n";

        let result = merge_lib_rs(base, overlay).unwrap();
        // 应保留注释
        assert!(result.content.starts_with("//! 核心库。"));
        // 新 mod 追加到末尾
        assert!(result.content.contains("pub mod merge;"));
    }

    // =========================================================================
    // extract_mod_name 单元测试
    // =========================================================================

    #[test]
    fn test_extract_mod_name_simple() {
        assert_eq!(extract_mod_name("mod foo;"), Some("foo".to_string()));
        assert_eq!(extract_mod_name("pub mod bar;"), Some("bar".to_string()));
        assert_eq!(
            extract_mod_name("pub(crate) mod baz;"),
            Some("baz".to_string())
        );
    }

    #[test]
    fn test_extract_mod_name_block_form_ignored() {
        // 块形式不应匹配
        assert_eq!(extract_mod_name("mod foo {"), None);
        assert_eq!(extract_mod_name("pub mod bar {"), None);
    }

    #[test]
    fn test_extract_mod_name_non_mod() {
        assert_eq!(extract_mod_name("use std::io;"), None);
        assert_eq!(extract_mod_name("fn main() {"), None);
        assert_eq!(extract_mod_name("// mod commented;"), None);
    }

    // =========================================================================
    // own files 复制测试
    // =========================================================================

    #[test]
    fn test_copy_own_files_basic() {
        let worktree_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();

        // 在 worktree 中创建文件
        let src_dir = worktree_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("foo.rs"), "// foo module").unwrap();
        std::fs::write(src_dir.join("bar.rs"), "// bar module").unwrap();

        let own_files = vec![
            worktree_dir.path().join("src/foo.rs"),
            worktree_dir.path().join("src/bar.rs"),
        ];

        let warnings = copy_own_files(&own_files, worktree_dir.path(), target_dir.path()).unwrap();
        assert!(warnings.is_empty());

        // 验证文件已复制
        assert!(target_dir.path().join("src/foo.rs").exists());
        assert!(target_dir.path().join("src/bar.rs").exists());
        assert_eq!(
            std::fs::read_to_string(target_dir.path().join("src/foo.rs")).unwrap(),
            "// foo module"
        );
    }

    #[test]
    fn test_copy_own_files_collision() {
        let worktree_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();

        // worktree 中有文件
        let src_dir = worktree_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("foo.rs"), "// new content").unwrap();

        // target 中已有同名文件
        let dst_dir = target_dir.path().join("src");
        std::fs::create_dir_all(&dst_dir).unwrap();
        std::fs::write(dst_dir.join("foo.rs"), "// old content").unwrap();

        let own_files = vec![worktree_dir.path().join("src/foo.rs")];

        let warnings = copy_own_files(&own_files, worktree_dir.path(), target_dir.path()).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("同名碰撞"));

        // 原有文件不应被覆盖
        assert_eq!(
            std::fs::read_to_string(target_dir.path().join("src/foo.rs")).unwrap(),
            "// old content"
        );
    }

    #[test]
    fn test_copy_own_files_nested_dirs() {
        let worktree_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();

        // 创建嵌套目录结构
        let nested = worktree_dir.path().join("src/graph/query");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("deep.rs"), "// deep").unwrap();

        let own_files = vec![worktree_dir.path().join("src/graph/query/deep.rs")];

        let warnings = copy_own_files(&own_files, worktree_dir.path(), target_dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert!(target_dir.path().join("src/graph/query/deep.rs").exists());
    }
}
