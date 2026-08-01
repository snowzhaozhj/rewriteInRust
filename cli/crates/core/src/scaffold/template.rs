//! Rust 项目骨架生成。
//!
//! 委托 `cargo init` 生成标准项目结构，避免硬编码模板。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{MigrateError, Result};
use crate::process::{run_with_timeout, CARGO_TIMEOUT};

/// 生成 Rust lib 项目骨架。
///
/// 委托 `cargo init --lib` 生成标准结构（Cargo.toml + src/lib.rs）。
/// 如果目标目录已有 Cargo.toml 则跳过（幂等）。
///
/// 返回**警告列表**（非空时调用方须降级 `status=warning` 并如实转达）：目前唯一来源是
/// 「`cargo init` 把新 crate 追加进外层 workspace 的 `members`」，见 [`warn_if_parent_workspace_mutated`]。
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<Vec<String>> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    // 已 scaffold（Cargo.toml 在）时仍确保 .gitignore——首次 cargo init 成功但
    // write_gitignore 失败（权限/磁盘/进程中断）后重跑须能补齐，否则 target/ 会漏进提交
    // （codex 审查指出的失败重试语义漏洞）。
    if target_dir.join("Cargo.toml").exists() {
        write_gitignore(target_dir)?;
        return Ok(Vec::new());
    }

    std::fs::create_dir_all(target_dir)?;

    // 先记下外层 workspace manifest 的原样，供事后比对（cargo 是否把新 crate 塞进 members）。
    let parent_manifest = find_enclosing_workspace_manifest(target_dir);
    let parent_before = snapshot_manifest(parent_manifest.as_deref());

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

    Ok(warn_if_parent_workspace_mutated(
        parent_manifest.as_deref(),
        parent_before.as_deref(),
    ))
}

/// 自 `target_dir` 向上寻找第一个含 `[workspace]` 段的 `Cargo.toml`。
///
/// 只做**词法**判断（是否有一行去空白后以 `[workspace]` 开头）——不解析 TOML：此处目的
/// 是「事后能否比对出被改动」，宁可多找一个候选也不该因 TOML 方言细节漏检。找不到返回
/// `None`（裸目录 / 非 workspace 父仓，此时 cargo 不会有 members 追加行为）。
fn find_enclosing_workspace_manifest(target_dir: &Path) -> Option<PathBuf> {
    // 从 target_dir 的父目录起找：target_dir 自己的 Cargo.toml 是本次要生成的产物。
    target_dir.parent()?.ancestors().find_map(|dir| {
        let manifest = dir.join("Cargo.toml");
        let content = std::fs::read_to_string(&manifest).ok()?;
        content
            .lines()
            .any(|line| line.trim_start().starts_with("[workspace]"))
            .then_some(manifest)
    })
}

/// 读取 manifest 原文用于事后比对；读不到（不存在/无权限）返回 `None`。
fn snapshot_manifest(path: Option<&Path>) -> Option<String> {
    std::fs::read_to_string(path?).ok()
}

/// 若外层 workspace manifest 被 `cargo init` 改动，产出一条警告。
///
/// **为何需要**：用户的典型场景恰是「已有 Rust workspace 的仓库里迁模块进来」。cargo
/// 在检测到外层 `[workspace]` 时会把新 crate 追加进 `members`（stderr 打
/// `Adding ... as member of workspace`），此后父仓 `cargo build` / `cargo test` 会连带
/// 编译迁移产物——而迁移中的 crate 常处于不可编译的中间态（`unimplemented!()`、
/// `TODO(port)`），足以把用户原本绿的构建搞红。CLI 不该静默改用户仓库的构建配置
/// （#86 记账 TODO ②，编排器 2026-08-01 实测：CLI 返回 `status:ok` 零 warning，
/// 而父 `members` 已被改）。
///
/// **判据用「改动前后比对内容」而非匹配 cargo 的 stderr 文案**：文案随 cargo 版本变动、
/// 且可能被本地化，比对内容对版本无假设。副作用是任何原因导致的父 manifest 变化都会
/// 报——这个方向是安全的（宁可多提醒，不可漏报改用户文件）。
///
/// 不报错只告警：追加 member 本身未破坏任何东西，且用户可能确实想要这个结果；能否接受
/// 由用户判断，CLI 的职责是不让它静默发生。
fn warn_if_parent_workspace_mutated(path: Option<&Path>, before: Option<&str>) -> Vec<String> {
    let (Some(path), Some(before)) = (path, before) else {
        return Vec::new();
    };
    let Ok(after) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    if after == before {
        return Vec::new();
    }
    vec![format!(
        "cargo init 改动了外层 workspace 清单 {}（通常是把新 crate 追加进 `members`）——\
         此后该仓库的 `cargo build`/`cargo test` 会连带编译迁移产物，而迁移中的 crate \
         常处于不可编译的中间态。如不需要，请从 `members` 移除该条目（或改用仓库外的 \
         --target 路径）",
        path.display()
    )]
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
///
/// 返回警告列表，语义同 [`scaffold_project`]。
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<Vec<String>> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    // 见 scaffold_project：已有 Cargo.toml 仍确保 .gitignore（失败重试补齐）。
    if target_dir.join("Cargo.toml").exists() {
        write_gitignore(target_dir)?;
        return Ok(Vec::new());
    }

    std::fs::create_dir_all(target_dir)?;

    let parent_manifest = find_enclosing_workspace_manifest(target_dir);
    let parent_before = snapshot_manifest(parent_manifest.as_deref());

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

    Ok(warn_if_parent_workspace_mutated(
        parent_manifest.as_deref(),
        parent_before.as_deref(),
    ))
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

    // -----------------------------------------------------------------------
    // 外层 workspace 检测（#86 记账 TODO ②）
    //
    // 此前全部 scaffold 测试都在裸 tempdir 跑，而用户典型场景恰是「已有 Rust
    // workspace 的仓库里迁模块进来」——那条路径零覆盖、零告警。
    // -----------------------------------------------------------------------

    /// 造一个含 `[workspace]` 的父仓，返回 (tmp, 父 manifest 路径)。
    fn workspace_parent() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let existing = tmp.path().join("crates/existing/src");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/existing\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("crates/existing/Cargo.toml"),
            "[package]\nname = \"existing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(existing.join("lib.rs"), "").unwrap();
        let manifest = tmp.path().join("Cargo.toml");
        (tmp, manifest)
    }

    #[test]
    fn test_scaffold_warns_when_parent_workspace_mutated() {
        let (tmp, manifest) = workspace_parent();
        let before = std::fs::read_to_string(&manifest).unwrap();

        let warnings = scaffold_project("probe", &tmp.path().join("crates/probe")).unwrap();

        // cargo 把新 crate 塞进 members——先证明改动真的发生（不然断言告警就是空转）。
        let after = std::fs::read_to_string(&manifest).unwrap();
        assert_ne!(
            after, before,
            "前置假设不成立：cargo 未改动父 manifest（cargo 行为变了？），\
             此时告警断言无意义——请重新核对本测试的前提"
        );
        assert!(
            after.contains("crates/probe"),
            "父 members 应含新 crate: {after}"
        );

        assert_eq!(warnings.len(), 1, "应恰好一条告警: {warnings:?}");
        let w = &warnings[0];
        assert!(
            w.contains("workspace") && w.contains("members"),
            "告警须点明改的是 workspace members: {w}"
        );
        assert!(
            w.contains(&manifest.display().to_string()),
            "告警须给出被改文件的路径（用户要据此去修）: {w}"
        );
    }

    #[test]
    fn test_scaffold_no_warning_in_bare_dir() {
        // 裸目录（无外层 workspace）：cargo 不会追加 member，不该告警。
        let tmp = TempDir::new().unwrap();
        let warnings = scaffold_project("solo", &tmp.path().join("solo")).unwrap();
        assert!(warnings.is_empty(), "裸目录不应告警: {warnings:?}");
    }

    #[test]
    fn test_scaffold_no_warning_when_parent_is_plain_package() {
        // 父目录有 Cargo.toml 但是普通 [package]（非 workspace）：cargo 不改它，不该告警。
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let before = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();

        let warnings = scaffold_project("inner", &tmp.path().join("inner")).unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap(),
            before,
            "普通 package 父 manifest 不该被改动"
        );
        assert!(
            warnings.is_empty(),
            "非 workspace 父目录不应告警: {warnings:?}"
        );
    }

    #[test]
    fn test_scaffold_with_bin_also_warns_on_parent_workspace() {
        // 两个 scaffold 函数须行为一致——否则删掉其中一个的检测调用，测试不会红。
        let (tmp, _manifest) = workspace_parent();
        let warnings =
            scaffold_project_with_bin("probe_bin", &tmp.path().join("crates/probe_bin")).unwrap();
        assert_eq!(warnings.len(), 1, "with_bin 也应告警: {warnings:?}");
    }

    #[test]
    fn test_scaffold_idempotent_rerun_does_not_warn() {
        // 幂等重跑走早返回路径、不调 cargo init，故不会再改父 manifest → 不该告警
        // （否则编排器每次重入都收到一条无动作可做的噪声告警）。
        let (tmp, _manifest) = workspace_parent();
        let target = tmp.path().join("crates/probe");

        let first = scaffold_project("probe", &target).unwrap();
        assert_eq!(first.len(), 1, "首次应告警");

        let second = scaffold_project("probe", &target).unwrap();
        assert!(second.is_empty(), "幂等重跑不应重复告警: {second:?}");
    }

    #[test]
    fn test_find_enclosing_workspace_manifest_skips_target_own_manifest() {
        // 目标目录自己的 Cargo.toml（哪怕含 [workspace]）不算「外层」——否则会把
        // 本次要生成的产物当成父仓。
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("selfws");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();

        assert_eq!(
            find_enclosing_workspace_manifest(&target),
            None,
            "不应把目标目录自身的 manifest 当作外层 workspace"
        );
    }

    #[test]
    fn test_find_enclosing_workspace_manifest_finds_grandparent() {
        // 多级嵌套：跳过中间无 workspace 的层，找到更上层的。
        let (tmp, manifest) = workspace_parent();
        let deep = tmp.path().join("crates/a/b");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(
            find_enclosing_workspace_manifest(&deep),
            Some(manifest),
            "应沿 ancestors 找到祖辈 workspace manifest"
        );
    }
}
