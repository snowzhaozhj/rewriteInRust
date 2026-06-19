//! 子进程超时执行。
//!
//! 统一封装 `std::process::Command` 的超时机制，避免子进程挂死导致 CLI 永久卡住。
//! 使用 `wait-timeout` crate 实现跨平台超时等待。

use std::process::{Command, Output, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::error::{MigrateError, Result};

// ── 默认超时常量 ──────────────────────────────────────────────

/// cargo 命令默认超时（check / clippy / init 等）。
pub const CARGO_TIMEOUT: Duration = Duration::from_secs(60);

/// 工具版本探测默认超时（`<tool> --version`）。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

// ── 公共 API ─────────────────────────────────────────────────

/// 带超时执行命令并收集输出（等价于 `Command::output()` + 超时保护）。
///
/// - 超时到达时自动 kill 子进程并返回 `MigrateError::Timeout`。
/// - `stdin` 固定置 null，防止子进程误等交互输入。
///
/// # 参数
/// - `cmd`: 已配置好 args / env / current_dir 的 Command（stdin 会被覆盖为 null）。
/// - `timeout`: 超时时长。
/// - `label`: 用于错误消息的命令描述（如 `"cargo check"`）。
pub fn run_with_timeout(cmd: &mut Command, timeout: Duration, label: &str) -> Result<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            // 保留 IO 错误原始类型，让调用方可以区分 NotFound 等
            MigrateError::Io(e)
        })?;

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            // 子进程已退出，收集 stdout/stderr
            let mut stdout_buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                std::io::Read::read_to_end(&mut out, &mut stdout_buf)?;
            }
            let mut stderr_buf = Vec::new();
            if let Some(mut err) = child.stderr.take() {
                std::io::Read::read_to_end(&mut err, &mut stderr_buf)?;
            }

            Ok(Output {
                status,
                stdout: stdout_buf,
                stderr: stderr_buf,
            })
        }
        Ok(None) => {
            // 超时——kill 子进程
            let _ = child.kill();
            let _ = child.wait();
            Err(MigrateError::Timeout {
                command: label.to_owned(),
                timeout_secs: timeout.as_secs(),
            })
        }
        Err(e) => Err(MigrateError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_with_timeout_success() {
        // echo 应在超时内完成
        let output = run_with_timeout(
            Command::new("echo").arg("hello"),
            Duration::from_secs(5),
            "echo hello",
        )
        .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello"));
    }

    #[test]
    fn test_run_with_timeout_timeout() {
        // sleep 60 应在 1s 内超时
        let result = run_with_timeout(
            Command::new("sleep").arg("60"),
            Duration::from_secs(1),
            "sleep 60",
        );

        match result {
            Err(MigrateError::Timeout {
                command,
                timeout_secs,
            }) => {
                assert_eq!(command, "sleep 60");
                assert_eq!(timeout_secs, 1);
            }
            other => panic!("期望 Timeout 错误，实际: {other:?}"),
        }
    }

    #[test]
    fn test_run_with_timeout_not_found() {
        // 不存在的命令应返回 IO 错误
        let result = run_with_timeout(
            &mut Command::new("definitely-not-a-real-command-xyz"),
            Duration::from_secs(5),
            "ghost",
        );

        assert!(matches!(result, Err(MigrateError::Io(_))));
    }

    #[test]
    fn test_run_with_timeout_nonzero_exit() {
        // false 命令退出码非零但不超时
        let output =
            run_with_timeout(&mut Command::new("false"), Duration::from_secs(5), "false").unwrap();

        assert!(!output.status.success());
    }

    #[test]
    fn test_timeout_constants() {
        assert_eq!(CARGO_TIMEOUT.as_secs(), 60);
        assert_eq!(PROBE_TIMEOUT.as_secs(), 30);
    }
}
