//! reconcile 冲突检测与轮次上限。
//!
//! 多 worktree 合并到主 `rust_root` 后，共享 `.rs` 文件可能产生 git 冲突。
//! 本模块提供**检测+报告**能力，不自动修复——重译由上层编排器驱动 SubAgent。
//!
//! 设计参考：
//! - `docs/decisions/003-m2-parallel-write-isolation.md` 约束 #7
//! - `docs/PLAN-M2.md` §7 SCALE-02 通信协议第⑤步

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::error::Result;
use crate::process::run_with_timeout;

// ── 常量 ─────────────────────────────────────────────────────

/// git 冲突检测命令超时（秒）。
const GIT_CONFLICT_TIMEOUT: Duration = Duration::from_secs(30);

// ── 配置 ─────────────────────────────────────────────────────

/// reconcile 配置。
#[derive(Debug, Clone)]
pub struct ReconcileConfig {
    /// 最大重试轮次（默认 3，对齐 M1 `max_compile_retries`）。
    pub max_rounds: u32,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self { max_rounds: 3 }
    }
}

// ── 结果类型 ─────────────────────────────────────────────────

/// reconcile 单轮结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// 全部合并成功，无冲突。
    Clean,
    /// 存在冲突，需要重译的模块列表。
    Conflicts { modules: Vec<String> },
    /// 超过最大轮次，降级处理（串行 / 转人工）。
    ExhaustedRetries { remaining_conflicts: Vec<String> },
}

// ── 公共 API ─────────────────────────────────────────────────

/// 检测 worktree 合并后的 git 冲突文件，返回冲突的模块名列表。
///
/// 通过 `git diff --name-only --diff-filter=U` 获取未合并（Unmerged）文件，
/// 再将 `.rs` 文件路径映射到模块名。
pub fn detect_merge_conflicts(project_root: &Path) -> Result<Vec<String>> {
    let output = run_with_timeout(
        Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=U"])
            .current_dir(project_root),
        GIT_CONFLICT_TIMEOUT,
        "git diff --diff-filter=U",
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let conflicted_files: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if conflicted_files.is_empty() {
        return Ok(Vec::new());
    }

    // 将冲突文件映射到模块名（去重）
    let mut modules: Vec<String> = conflicted_files
        .iter()
        .filter_map(|path| file_path_to_module(path))
        .collect();

    modules.sort();
    modules.dedup();

    Ok(modules)
}

/// 执行 reconcile 单轮：检测冲突 → 判断是否超限 → 返回结果。
///
/// 由上层编排器循环驱动：每轮调用此函数，若返回 `Conflicts` 则重译对应模块后再调用下一轮。
pub fn reconcile_round(
    project_root: &Path,
    round: u32,
    config: &ReconcileConfig,
) -> Result<ReconcileOutcome> {
    // 超过轮次上限直接返回（含 max_rounds=0 的边界情况）
    if round >= config.max_rounds {
        let conflicts = detect_merge_conflicts(project_root)?;
        if conflicts.is_empty() {
            return Ok(ReconcileOutcome::Clean);
        }
        return Ok(ReconcileOutcome::ExhaustedRetries {
            remaining_conflicts: conflicts,
        });
    }

    let conflicts = detect_merge_conflicts(project_root)?;

    if conflicts.is_empty() {
        Ok(ReconcileOutcome::Clean)
    } else {
        Ok(ReconcileOutcome::Conflicts { modules: conflicts })
    }
}

// ── 内部辅助 ─────────────────────────────────────────────────

/// 将文件路径映射到模块名。
///
/// 规则：
/// - `src/<name>.rs` → `<name>`（去掉扩展名）
/// - `src/<name>/mod.rs` → `<name>`
/// - `src/<name>/<sub>.rs` → `<name>/<sub>`
/// - 非 `.rs` 文件返回 None（Cargo.toml 等由结构化合并处理，不参与 reconcile）
fn file_path_to_module(path: &str) -> Option<String> {
    // 只处理 .rs 文件
    if !path.ends_with(".rs") {
        return None;
    }

    // 尝试剥离 src/ 前缀（可能嵌套在子目录中，取最后一个 src/）
    let src_relative = if let Some(idx) = path.rfind("src/") {
        &path[idx + 4..]
    } else {
        // 非 src/ 下的 .rs 文件，直接用文件名作为模块名
        return path
            .rsplit('/')
            .next()
            .map(|f| f.trim_end_matches(".rs").to_owned());
    };

    // src/lib.rs 和 src/main.rs 不映射为普通模块
    if src_relative == "lib.rs" || src_relative == "main.rs" {
        return Some(src_relative.trim_end_matches(".rs").to_owned());
    }

    // src/<name>/mod.rs → <name>
    if src_relative.ends_with("/mod.rs") {
        let module = src_relative.trim_end_matches("/mod.rs");
        return Some(module.to_owned());
    }

    // src/<name>.rs → <name>
    // src/<name>/<sub>.rs → <name>/<sub>
    Some(src_relative.trim_end_matches(".rs").to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── file_path_to_module 单元测试 ─────────────────────────

    #[test]
    fn test_module_from_simple_rs() {
        assert_eq!(
            file_path_to_module("src/parser.rs"),
            Some("parser".to_owned())
        );
    }

    #[test]
    fn test_module_from_mod_rs() {
        assert_eq!(
            file_path_to_module("src/graph/mod.rs"),
            Some("graph".to_owned())
        );
    }

    #[test]
    fn test_module_from_nested_rs() {
        assert_eq!(
            file_path_to_module("src/graph/builder.rs"),
            Some("graph/builder".to_owned())
        );
    }

    #[test]
    fn test_module_from_lib_rs() {
        assert_eq!(file_path_to_module("src/lib.rs"), Some("lib".to_owned()));
    }

    #[test]
    fn test_non_rs_file_returns_none() {
        assert_eq!(file_path_to_module("Cargo.toml"), None);
    }

    #[test]
    fn test_module_from_deeply_nested_src() {
        // 嵌套项目路径中仍取最后一个 src/
        assert_eq!(
            file_path_to_module("rust_root/src/error.rs"),
            Some("error".to_owned())
        );
    }

    // ── ReconcileConfig 默认值 ───────────────────────────────

    #[test]
    fn test_reconcile_config_default() {
        let config = ReconcileConfig::default();
        assert_eq!(config.max_rounds, 3);
    }

    // ── detect_merge_conflicts（真实 git 仓库）───────────────

    #[test]
    fn test_detect_no_conflicts_in_clean_repo() {
        // 当前工作区应该没有合并冲突
        let repo_root = std::env::current_dir().unwrap();
        // 找到 git 仓库根目录
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&repo_root)
            .output()
            .unwrap();
        let git_root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if git_root.is_empty() {
            return; // 非 git 仓库环境，跳过
        }

        let conflicts = detect_merge_conflicts(Path::new(&git_root)).unwrap();
        assert!(conflicts.is_empty(), "干净仓库应无冲突");
    }

    // ── reconcile_round 逻辑测试 ─────────────────────────────

    #[test]
    fn test_reconcile_round_clean_repo_returns_clean() {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .unwrap();
        let git_root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if git_root.is_empty() {
            return;
        }

        let config = ReconcileConfig::default();
        let outcome = reconcile_round(Path::new(&git_root), 0, &config).unwrap();
        assert_eq!(outcome, ReconcileOutcome::Clean);
    }

    #[test]
    fn test_reconcile_round_max_rounds_zero_exhausted() {
        // max_rounds=0 时，即使第 0 轮也直接返回 ExhaustedRetries（或 Clean）
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .unwrap();
        let git_root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if git_root.is_empty() {
            return;
        }

        let config = ReconcileConfig { max_rounds: 0 };
        let outcome = reconcile_round(Path::new(&git_root), 0, &config).unwrap();
        // 干净仓库 max_rounds=0 时，检测到无冲突应返回 Clean
        assert_eq!(outcome, ReconcileOutcome::Clean);
    }

    #[test]
    fn test_reconcile_round_exceeds_max_rounds() {
        // round >= max_rounds 时走 ExhaustedRetries 路径（干净仓库实际返回 Clean）
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .unwrap();
        let git_root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if git_root.is_empty() {
            return;
        }

        let config = ReconcileConfig { max_rounds: 2 };
        // round=2 >= max_rounds=2
        let outcome = reconcile_round(Path::new(&git_root), 2, &config).unwrap();
        assert_eq!(outcome, ReconcileOutcome::Clean);

        // round=5 > max_rounds=2
        let outcome = reconcile_round(Path::new(&git_root), 5, &config).unwrap();
        assert_eq!(outcome, ReconcileOutcome::Clean);
    }

    // ── ReconcileOutcome 构造验证 ────────────────────────────

    #[test]
    fn test_outcome_conflicts_construction() {
        let outcome = ReconcileOutcome::Conflicts {
            modules: vec!["parser".to_owned(), "error".to_owned()],
        };
        match outcome {
            ReconcileOutcome::Conflicts { modules } => {
                assert_eq!(modules.len(), 2);
                assert!(modules.contains(&"parser".to_owned()));
            }
            _ => panic!("期望 Conflicts 变体"),
        }
    }

    #[test]
    fn test_outcome_exhausted_construction() {
        let outcome = ReconcileOutcome::ExhaustedRetries {
            remaining_conflicts: vec!["graph".to_owned()],
        };
        match outcome {
            ReconcileOutcome::ExhaustedRetries {
                remaining_conflicts,
            } => {
                assert_eq!(remaining_conflicts, vec!["graph"]);
            }
            _ => panic!("期望 ExhaustedRetries 变体"),
        }
    }
}
