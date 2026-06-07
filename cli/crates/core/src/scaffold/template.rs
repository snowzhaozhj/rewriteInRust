//! Cargo.toml 骨架生成。
//!
//! 在目标目录生成 Rust 项目基本结构：Cargo.toml + src/lib.rs。

use std::fs;
use std::path::Path;

use crate::error::{MigrateError, Result};

/// 默认 Cargo.toml 模板。
fn cargo_toml_content(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
"#
    )
}

/// 默认 src/lib.rs 内容。
const LIB_RS_CONTENT: &str = "//! 由 rustmigrate 自动生成的库入口。\n";

/// 默认 src/main.rs 内容。
const MAIN_RS_CONTENT: &str = r#"fn main() {
    println!("Hello from rustmigrate scaffold!");
}
"#;

/// 生成 Rust 项目骨架。
///
/// 在 `target_dir` 下生成：
/// - `Cargo.toml`（项目名、edition 2021、空 dependencies）
/// - `src/lib.rs`（空的 lib 入口）
///
/// 如果目标目录不存在则自动创建。
/// 不覆盖已存在的文件。
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<()> {
    // 参数校验
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    // 创建目标目录
    fs::create_dir_all(target_dir)?;

    // 创建 src 目录
    let src_dir = target_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    // 生成 Cargo.toml（不覆盖）
    let cargo_path = target_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        fs::write(&cargo_path, cargo_toml_content(name))?;
    }

    // 生成 src/lib.rs（不覆盖）
    let lib_path = src_dir.join("lib.rs");
    if !lib_path.exists() {
        fs::write(&lib_path, LIB_RS_CONTENT)?;
    }

    Ok(())
}

/// 生成带有 bin target 的 Rust 项目骨架。
///
/// 除了 `scaffold_project` 的内容外，额外生成 `src/main.rs`。
/// 不覆盖已存在的文件。
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<()> {
    scaffold_project(name, target_dir)?;

    let main_path = target_dir.join("src").join("main.rs");
    if !main_path.exists() {
        fs::write(&main_path, MAIN_RS_CONTENT)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scaffold_project_basic() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");

        scaffold_project("my_project", &target).unwrap();

        // 验证文件存在
        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());

        // 验证 Cargo.toml 内容
        let cargo = fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"my_project\""));
        assert!(cargo.contains("edition = \"2021\""));
        assert!(cargo.contains("[dependencies]"));
    }

    #[test]
    fn test_scaffold_project_no_overwrite() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");

        // 第一次生成
        scaffold_project("my_project", &target).unwrap();

        // 手动修改 Cargo.toml
        let cargo_path = target.join("Cargo.toml");
        fs::write(&cargo_path, "# custom content").unwrap();

        // 再次生成，不应覆盖
        scaffold_project("my_project", &target).unwrap();

        let cargo = fs::read_to_string(&cargo_path).unwrap();
        assert_eq!(cargo, "# custom content");
    }

    #[test]
    fn test_scaffold_project_with_bin() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_bin");

        scaffold_project_with_bin("my_bin", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());
        assert!(target.join("src/main.rs").exists());

        let main = fs::read_to_string(target.join("src/main.rs")).unwrap();
        assert!(main.contains("fn main()"));
    }

    #[test]
    fn test_scaffold_project_with_bin_no_overwrite_main() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_bin");

        scaffold_project_with_bin("my_bin", &target).unwrap();

        // 修改 main.rs
        let main_path = target.join("src/main.rs");
        fs::write(&main_path, "// custom main").unwrap();

        // 再次生成
        scaffold_project_with_bin("my_bin", &target).unwrap();

        let main = fs::read_to_string(&main_path).unwrap();
        assert_eq!(main, "// custom main");
    }

    #[test]
    fn test_scaffold_project_empty_name() {
        let tmp = TempDir::new().unwrap();
        let result = scaffold_project("", tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_scaffold_project_with_bin_empty_name() {
        let tmp = TempDir::new().unwrap();
        let result = scaffold_project_with_bin("", tmp.path());
        assert!(result.is_err(), "空名称应返回错误");
    }

    #[test]
    fn test_scaffold_project_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("a").join("b").join("c");

        scaffold_project("nested", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());
    }

    #[test]
    fn test_cargo_toml_content_format() {
        let content = cargo_toml_content("test-crate");
        assert!(content.starts_with("[package]"));
        assert!(content.contains("name = \"test-crate\""));
        assert!(content.contains("version = \"0.1.0\""));
        assert!(content.contains("edition = \"2021\""));
        assert!(content.ends_with("[dependencies]\n"));
    }
}
