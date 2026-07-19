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

    // 已 scaffold（Cargo.toml 在）时仍确保 .gitignore——首次 cargo init 成功但
    // write_gitignore 失败（权限/磁盘/进程中断）后重跑须能补齐，否则 target/ 会漏进提交
    // （codex 审查指出的失败重试语义漏洞）。
    if target_dir.join("Cargo.toml").exists() {
        return write_gitignore(target_dir);
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

/// 确保 crate 级 `.gitignore` 忽略 `/target`。
///
/// `cargo init --vcs none` 不生成 `.gitignore`；即便用 `--vcs git`，cargo 在检测到
/// 外层已是 git 仓库时也会静默跳过。而并行编排在各 worktree 内跑 `cargo check` 自检
/// 会产生 `target/`，若无 `.gitignore` 则被 `git add -A` 吞进提交、污染合并（M4-ORCH-01
/// PR-5 演练撞出）。故显式确保，不依赖 cargo 的条件行为。
///
/// 后置条件式幂等（而非「文件存在即跳过」）：
/// - 无 `.gitignore` → 新建，写 `/target`。
/// - 有 `.gitignore` 但无有效 `/target` 规则 → 追加一行 `/target`，保留用户既有内容。
/// - 已有有效 `/target` 规则 → 不动。
///
/// 「有效规则」指非注释、去空白后恰为 `/target` 的行——避免把 `#/target`、`/target-old`
/// 等误判为已忽略（codex 审查指出）。
fn write_gitignore(target_dir: &Path) -> Result<()> {
    let path = target_dir.join(".gitignore");
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let already_ignored = existing
        .as_deref()
        .is_some_and(|content| content.lines().map(str::trim).any(|line| line == "/target"));
    if already_ignored {
        return Ok(());
    }

    match existing {
        // 追加：保留用户既有内容，末尾无换行时先补一个再加规则。
        Some(mut content) => {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str("/target\n");
            std::fs::write(&path, content)?;
        }
        None => std::fs::write(&path, "/target\n")?,
    }
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

    // 见 scaffold_project：已有 Cargo.toml 仍确保 .gitignore（失败重试补齐）。
    if target_dir.join("Cargo.toml").exists() {
        return write_gitignore(target_dir);
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
        assert_eq!(gitignore, "/target\n", "新建 .gitignore 应恰为 /target");
    }

    #[test]
    fn test_scaffold_gitignore_appends_when_target_missing() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 预置无 /target 的自定义 .gitignore：应保留用户内容 + 追加 /target
        // （codex 审查 Important 1：文件存在但不含 /target 时不能跳过）。
        std::fs::write(target.join(".gitignore"), "/custom\n").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/custom\n/target\n",
            "既有内容应保留，/target 追加在后"
        );
    }

    #[test]
    fn test_scaffold_gitignore_no_dup_when_target_present() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 已有有效 /target 规则：不得重复追加。
        std::fs::write(target.join(".gitignore"), "/foo\n/target\n/bar\n").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/foo\n/target\n/bar\n",
            "已有 /target 规则不应重复追加"
        );
    }

    #[test]
    fn test_scaffold_gitignore_backfilled_when_cargo_exists() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 模拟「首次 cargo init 成功但 write_gitignore 失败」后的残缺态：
        // Cargo.toml 在、.gitignore 缺。重跑须补齐（codex 审查 Important 2：
        // 早返回路径不能跳过 .gitignore 后置条件）。
        std::fs::write(target.join("Cargo.toml"), "# existing").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/target\n",
            "已有 Cargo.toml、缺 .gitignore 应补齐"
        );
        // Cargo.toml 不被触碰。
        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert_eq!(cargo, "# existing");
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

        // 第二处调用点也须生成 .gitignore（否则删掉 scaffold_project_with_bin 的
        // write_gitignore 调用测试不会红——codex 审查指出）。
        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(gitignore, "/target\n");
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
