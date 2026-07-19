//! Rust 项目骨架生成。
//!
//! 委托 `cargo init` 生成标准项目结构，避免硬编码模板。

use std::path::Path;
use std::process::Command;

use crate::error::{MigrateError, Result};
use crate::process::{run_with_timeout, CARGO_TIMEOUT};

/// 生成 Rust lib 项目骨架。
///
/// 委托 `cargo init --lib` 生成标准结构（Cargo.toml + src/lib.rs）。
/// 如果目标目录已有 Cargo.toml 则跳过（幂等）。
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<()> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    if target_dir.join("Cargo.toml").exists() {
        return Ok(());
    }

    std::fs::create_dir_all(target_dir)?;

    let output = run_with_timeout(
        Command::new("cargo")
            .args(["init", "--lib", "--name", name, "--vcs", "none"])
            .arg(target_dir),
        CARGO_TIMEOUT,
        "cargo init --lib",
    )
    .map_err(|e| match e {
        MigrateError::Io(io_err) => MigrateError::Config(format!("cargo init 执行失败: {io_err}")),
        other => other,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!("cargo init 失败: {stderr}")));
    }

    write_gitignore(target_dir)?;

    Ok(())
}

/// 写入 crate 级 `.gitignore`（含 `/target`），幂等（已存在则跳过）。
///
/// `cargo init --vcs none` 不生成 `.gitignore`；即便用 `--vcs git`，cargo 在检测到
/// 外层已是 git 仓库时也会静默跳过。而并行编排在各 worktree 内跑 `cargo check` 自检
/// 会产生 `target/`，若无 `.gitignore` 则被 `git add -A` 吞进提交、污染合并（M4-ORCH-01
/// PR-5 演练撞出）。故显式生成，不依赖 cargo 的条件行为。
fn write_gitignore(target_dir: &Path) -> Result<()> {
    let path = target_dir.join(".gitignore");
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, "/target\n")?;
    Ok(())
}

/// 生成带有 bin target 的 Rust 项目骨架。
///
/// 委托 `cargo init` 生成（默认包含 src/main.rs）。
/// 如果目标目录已有 Cargo.toml 则跳过（幂等）。
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<()> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    if target_dir.join("Cargo.toml").exists() {
        return Ok(());
    }

    std::fs::create_dir_all(target_dir)?;

    let output = run_with_timeout(
        Command::new("cargo")
            .args(["init", "--name", name, "--vcs", "none"])
            .arg(target_dir),
        CARGO_TIMEOUT,
        "cargo init",
    )
    .map_err(|e| match e {
        MigrateError::Io(io_err) => MigrateError::Config(format!("cargo init 执行失败: {io_err}")),
        other => other,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!("cargo init 失败: {stderr}")));
    }

    write_gitignore(target_dir)?;

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

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());

        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("my_project"));

        // scaffold 须生成含 /target 的 .gitignore（cargo init --vcs none 不生成，
        // 否则并行 worktree 自检产物 target/ 会被 git add 吞入提交，M4-ORCH-01 PR-5）。
        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert!(gitignore.contains("/target"), "gitignore 应忽略 /target");
    }

    #[test]
    fn test_scaffold_gitignore_preserved_when_exists() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 预置自定义 .gitignore：scaffold 不得覆盖（幂等）。
        std::fs::write(target.join(".gitignore"), "/custom\n").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(gitignore, "/custom\n", "已存在的 .gitignore 不应被覆盖");
    }

    #[test]
    fn test_scaffold_project_idempotent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");

        scaffold_project("my_project", &target).unwrap();

        let cargo_path = target.join("Cargo.toml");
        std::fs::write(&cargo_path, "# custom content").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let cargo = std::fs::read_to_string(&cargo_path).unwrap();
        assert_eq!(cargo, "# custom content");
    }

    #[test]
    fn test_scaffold_project_with_bin() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_bin");

        scaffold_project_with_bin("my_bin", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/main.rs").exists());

        let main = std::fs::read_to_string(target.join("src/main.rs")).unwrap();
        assert!(main.contains("fn main()"));
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
        assert!(result.is_err());
    }

    #[test]
    fn test_scaffold_project_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("a").join("b").join("c");

        scaffold_project("nested", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());
    }
}
