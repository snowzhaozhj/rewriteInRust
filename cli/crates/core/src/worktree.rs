//! Git worktree 生命周期管理。
//!
//! 为 M2 并行翻译提供 worktree 的创建、查询、销毁编排。
//! 每个 SubAgent 在独立 worktree 中工作，避免并发写冲突。
//!
//! 设计决策见 `docs/decisions/003-m2-parallel-write-isolation.md`。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::error::{MigrateError, Result};
use crate::process::run_with_timeout;

// ── 常量 ────────────────────────────────────────────────────────

/// git worktree 命令默认超时。
const WORKTREE_TIMEOUT: Duration = Duration::from_secs(30);

/// worktree 默认基础目录名。
const DEFAULT_BASE_DIR: &str = ".wt";

// ── 配置 ────────────────────────────────────────────────────────

/// worktree 配置。
#[derive(Debug, Clone)]
pub struct WorktreeConfig {
    /// worktree 存放目录（默认 `.wt/`）。
    pub base_dir: PathBuf,
    /// 是否使用独立 CARGO_TARGET_DIR（避免并发编译冲突）。
    pub isolated_target_dir: bool,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from(DEFAULT_BASE_DIR),
            isolated_target_dir: true,
        }
    }
}

// ── 信息结构 ────────────────────────────────────────────────────

/// worktree 信息。
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// worktree 在文件系统上的绝对路径。
    pub path: PathBuf,
    /// 关联的模块名。
    pub module_name: String,
    /// 独立 cargo target 目录路径（仅 `isolated_target_dir=true` 时有值）。
    pub cargo_target_dir: Option<PathBuf>,
}

// ── 公共 API ────────────────────────────────────────────────────

/// 创建一个 worktree 用于翻译指定模块。
///
/// 从当前 HEAD 创建分支 `wt/{sanitized_name}`，worktree 放在
/// `{project_root}/{base_dir}/{sanitized_name}`。
///
/// 如果 `isolated_target_dir=true`，会在 worktree 内写入
/// `.cargo/config.toml` 设置独立的 `CARGO_TARGET_DIR`。
pub fn create_worktree(
    project_root: &Path,
    module_name: &str,
    config: &WorktreeConfig,
) -> Result<WorktreeInfo> {
    let sanitized = sanitize_module_name(module_name);
    let base_dir = resolve_base_dir(project_root, &config.base_dir);
    let wt_path = base_dir.join(&sanitized);
    let branch_name = format!("wt/{sanitized}");

    // 确保基础目录存在
    std::fs::create_dir_all(&base_dir)?;

    // 执行 git worktree add
    let output = run_with_timeout(
        Command::new("git")
            .args(["worktree", "add", "-b", &branch_name])
            .arg(&wt_path)
            .arg("HEAD")
            .current_dir(project_root),
        WORKTREE_TIMEOUT,
        &format!("git worktree add {sanitized}"),
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!(
            "git worktree add 失败: {stderr}"
        )));
    }

    // 可选：设置独立 target 目录
    let cargo_target_dir = if config.isolated_target_dir {
        let target_dir = wt_path.join("target");
        write_cargo_config(&wt_path, &target_dir)?;
        Some(target_dir)
    } else {
        None
    };

    Ok(WorktreeInfo {
        path: wt_path,
        module_name: module_name.to_owned(),
        cargo_target_dir,
    })
}

/// 清理 worktree（`git worktree remove --force`）。
///
/// 同时删除关联的本地分支。
pub fn remove_worktree(project_root: &Path, worktree_path: &Path) -> Result<()> {
    // 先尝试从 worktree 路径推断分支名以便后续清理
    let branch_name = infer_branch_name(project_root, worktree_path);

    // git worktree remove --force
    let output = run_with_timeout(
        Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(worktree_path)
            .current_dir(project_root),
        WORKTREE_TIMEOUT,
        "git worktree remove",
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!(
            "git worktree remove 失败: {stderr}"
        )));
    }

    // 尝试删除关联分支（失败不阻塞）
    if let Some(branch) = branch_name {
        let _ = run_with_timeout(
            Command::new("git")
                .args(["branch", "-D", &branch])
                .current_dir(project_root),
            WORKTREE_TIMEOUT,
            "git branch -D",
        );
    }

    Ok(())
}

/// 列出当前活跃的 worktree（仅返回 `.wt/` 下的）。
pub fn list_worktrees(project_root: &Path) -> Result<Vec<WorktreeInfo>> {
    let output = run_with_timeout(
        Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(project_root),
        WORKTREE_TIMEOUT,
        "git worktree list",
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!(
            "git worktree list 失败: {stderr}"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // canonicalize 处理 symlink（macOS /tmp → /private/tmp）
    let raw_base = project_root.join(DEFAULT_BASE_DIR);
    let wt_base = if raw_base.exists() {
        raw_base.canonicalize().unwrap_or(raw_base)
    } else {
        // 基础目录不存在则无 worktree
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    let mut current_path: Option<PathBuf> = None;

    for line in stdout.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path_str));
        } else if line.is_empty() {
            // 空行分隔各 worktree 条目
            if let Some(path) = current_path.take() {
                // 只返回 .wt/ 下的 worktree
                if path.starts_with(&wt_base) {
                    let dir_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let cargo_target_dir = if path.join(".cargo").join("config.toml").exists() {
                        Some(path.join("target"))
                    } else {
                        None
                    };

                    result.push(WorktreeInfo {
                        path,
                        module_name: dir_name,
                        cargo_target_dir,
                    });
                }
            }
        }
    }

    // 处理末尾没有空行的情况
    if let Some(path) = current_path.take() {
        if path.starts_with(&wt_base) {
            let dir_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let cargo_target_dir = if path.join(".cargo").join("config.toml").exists() {
                Some(path.join("target"))
            } else {
                None
            };

            result.push(WorktreeInfo {
                path,
                module_name: dir_name,
                cargo_target_dir,
            });
        }
    }

    Ok(result)
}

// ── 内部辅助 ────────────────────────────────────────────────────

/// 将模块名中的 `/` 替换为 `_`，避免目录嵌套。
fn sanitize_module_name(name: &str) -> String {
    name.replace('/', "_")
}

/// 解析基础目录为绝对路径。
fn resolve_base_dir(project_root: &Path, base_dir: &Path) -> PathBuf {
    if base_dir.is_absolute() {
        base_dir.to_owned()
    } else {
        project_root.join(base_dir)
    }
}

/// 在 worktree 内写入 `.cargo/config.toml` 设置独立 target 目录。
fn write_cargo_config(wt_path: &Path, target_dir: &Path) -> Result<()> {
    let cargo_dir = wt_path.join(".cargo");
    std::fs::create_dir_all(&cargo_dir)?;

    let config_path = cargo_dir.join("config.toml");
    let content = format!(
        "# 自动生成——worktree 独立编译目录，避免并发冲突\n\
         [build]\n\
         target-dir = \"{}\"\n",
        target_dir.display()
    );

    std::fs::write(&config_path, content)?;
    Ok(())
}

/// 从 worktree 路径推断对应的分支名。
///
/// porcelain 输出格式：
/// ```text
/// worktree /path/to/.wt/foo
/// HEAD abc123
/// branch refs/heads/wt/foo
/// ```
fn infer_branch_name(project_root: &Path, worktree_path: &Path) -> Option<String> {
    let output = run_with_timeout(
        Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(project_root),
        WORKTREE_TIMEOUT,
        "git worktree list (infer branch)",
    )
    .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let wt_str = worktree_path.to_string_lossy();

    let mut found = false;
    for line in stdout.lines() {
        if let Some(path_str) = line.strip_prefix("worktree ") {
            found = path_str == wt_str.as_ref();
        } else if found {
            if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
                return Some(branch_ref.to_owned());
            }
        }
    }

    None
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 创建一个临时 git 仓库用于测试。
    fn setup_test_repo() -> TempDir {
        let tmp = TempDir::new().expect("创建临时目录失败");
        let root = tmp.path();

        // git init
        let output = Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init 失败");
        assert!(output.status.success(), "git init 失败");

        // 配置 user 信息（CI 环境可能未配置）
        let _ = Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(root)
            .output();

        // 创建初始提交（否则 HEAD 不存在）
        std::fs::write(root.join("README.md"), "# test").expect("写文件失败");
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output();
        let output = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .expect("git commit 失败");
        assert!(output.status.success(), "git commit 失败");

        tmp
    }

    #[test]
    fn test_sanitize_module_name() {
        assert_eq!(sanitize_module_name("src/utils"), "src_utils");
        assert_eq!(sanitize_module_name("simple"), "simple");
        assert_eq!(sanitize_module_name("a/b/c"), "a_b_c");
    }

    #[test]
    fn test_create_and_remove_worktree() {
        let tmp = setup_test_repo();
        let root = tmp.path();
        let config = WorktreeConfig::default();

        // 创建 worktree
        let info = create_worktree(root, "my_module", &config).expect("创建 worktree 失败");

        // 验证路径
        assert!(info.path.exists(), "worktree 目录应存在");
        assert_eq!(info.module_name, "my_module");
        assert!(info.path.ends_with(".wt/my_module"));

        // 验证 README 被复制（从 HEAD checkout）
        assert!(info.path.join("README.md").exists());

        // 验证 cargo target dir
        assert!(info.cargo_target_dir.is_some());
        let cargo_config = info.path.join(".cargo").join("config.toml");
        assert!(cargo_config.exists(), ".cargo/config.toml 应存在");
        let content = std::fs::read_to_string(&cargo_config).unwrap();
        assert!(content.contains("[build]"));
        assert!(content.contains("target-dir"));

        // 清理
        remove_worktree(root, &info.path).expect("删除 worktree 失败");
        assert!(!info.path.exists(), "worktree 目录应已删除");
    }

    #[test]
    fn test_create_worktree_with_slash_in_name() {
        let tmp = setup_test_repo();
        let root = tmp.path();
        let config = WorktreeConfig::default();

        // 模块名含 / 应被替换为 _
        let info =
            create_worktree(root, "src/utils", &config).expect("创建含斜杠模块名的 worktree 失败");

        assert!(info.path.ends_with(".wt/src_utils"));
        assert!(info.path.exists());

        remove_worktree(root, &info.path).expect("清理失败");
    }

    #[test]
    fn test_create_worktree_no_isolated_target() {
        let tmp = setup_test_repo();
        let root = tmp.path();
        let config = WorktreeConfig {
            isolated_target_dir: false,
            ..Default::default()
        };

        let info = create_worktree(root, "no_target", &config)
            .expect("创建 worktree（无隔离 target）失败");

        assert!(info.cargo_target_dir.is_none());
        let cargo_config = info.path.join(".cargo").join("config.toml");
        assert!(!cargo_config.exists(), "不应生成 .cargo/config.toml");

        remove_worktree(root, &info.path).expect("清理失败");
    }

    #[test]
    fn test_list_worktrees() {
        let tmp = setup_test_repo();
        let root = tmp.path();
        let config = WorktreeConfig::default();

        // 创建两个 worktree
        let info_a = create_worktree(root, "mod_a", &config).expect("创建 mod_a 失败");
        let info_b = create_worktree(root, "mod_b", &config).expect("创建 mod_b 失败");

        // 列出
        let list = list_worktrees(root).expect("列出 worktree 失败");
        assert_eq!(list.len(), 2, "应有 2 个 worktree");

        let names: Vec<&str> = list.iter().map(|w| w.module_name.as_str()).collect();
        assert!(names.contains(&"mod_a"), "应包含 mod_a");
        assert!(names.contains(&"mod_b"), "应包含 mod_b");

        // 清理
        remove_worktree(root, &info_a.path).expect("清理 mod_a 失败");
        remove_worktree(root, &info_b.path).expect("清理 mod_b 失败");

        // 清理后应为空
        let list_after = list_worktrees(root).expect("列出失败");
        assert!(list_after.is_empty(), "清理后不应有 worktree");
    }

    #[test]
    fn test_worktree_config_default() {
        let config = WorktreeConfig::default();
        assert_eq!(config.base_dir, PathBuf::from(".wt"));
        assert!(config.isolated_target_dir);
    }
}
