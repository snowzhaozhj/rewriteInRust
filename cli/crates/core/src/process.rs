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

    // 必须在 `wait_timeout` **之前**就开始抽 pipe：OS pipe buffer 满（stdout/stderr 各约
    // 64KB）后子进程会阻塞在 write 上永不退出，于是 `wait_timeout` 白等到超时才 kill——
    // 命令明明是好的，却报成超时。
    //
    // 这不是理论风险：`cargo metadata --no-deps` 的输出约 750B/成员，实测 114KB（约 150 个
    // 成员）起必然触发，而 `~/workspace/explore/oxc` 就是 55 成员 246KB。此前该函数只接
    // `cargo check/init` 与 `<tool> --version` 这类小输出命令，注释里把「远小于 64KB」当作
    // 前提写死；scaffold 的 workspace 成员检测打破了它，症状是每次 scaffold 白挂 60 秒并
    // 报出「成员 Cargo.toml 语法有误」的错误归因（workspace 其实完全健康）。
    //
    // 两个读线程各自读到 EOF。正常退出路径 join 它们取输出；超时路径**不 join**（理由见下）。
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(pipe, &mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = std::io::Read::read_to_end(pipe, &mut buf);
        }
        buf
    });

    // 读线程 panic 时取空输出而非连带 panic——命令的退出状态仍是有效信息。
    let join = |h: std::thread::JoinHandle<Vec<u8>>| h.join().unwrap_or_default();

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => Ok(Output {
            status,
            stdout: join(stdout_reader),
            stderr: join(stderr_reader),
        }),
        Ok(None) => {
            // 超时——kill 子进程后立即返回，**不 join 读线程**。
            //
            // `kill` 只终止直接子进程；它派生的孙进程会继承 pipe 写端，于是 EOF 要等到孙进程
            // 也结束才到来。join 在此会把「2 秒超时」变成「等满孙进程的 60 秒」——比原 bug
            // 更糟。实测 `sh -c 'yes | head -c 200000; sleep 60'` 正是这个形态。
            //
            // 两个读线程被留作 detached：它们只往自己的 Vec 写、拿不到任何共享状态，随进程
            // 退出被回收，不影响正确性（超时路径本就丢弃输出）。
            let _ = child.kill();
            let _ = child.wait();
            Err(MigrateError::Timeout {
                command: label.to_owned(),
                timeout_secs: timeout.as_secs(),
            })
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(MigrateError::Io(e))
        }
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

    /// 输出远超 OS pipe buffer（stdout/stderr 各约 64KB）时不得死锁。
    ///
    /// 曾经的实现先 `wait_timeout` 再读 pipe，子进程写满 buffer 后阻塞在 write 上永不退出，
    /// 于是好命令被报成超时。`scaffold workspace` 的成员检测调 `cargo metadata --no-deps`
    /// （约 750B/成员）时真实踩中：实测 114KB 起必然 60 秒超时，而 `oxc` 就有 246KB。
    ///
    /// 用 1MB（约 16 倍单 pipe 容量）确保任何平台的 buffer 都被灌满。超时给 30s 远宽于
    /// 正常耗时（实测毫秒级），故失败只可能是死锁而非机器慢。
    #[test]
    fn test_run_with_timeout_survives_output_exceeding_pipe_buffer() {
        const SIZE: usize = 1024 * 1024;

        let output = run_with_timeout(
            Command::new("sh").arg("-c").arg(format!(
                // stdout 与 stderr 同时灌满：两个 pipe 各自都得被并发抽走，
                // 只读一个仍会在另一个上死锁。
                "yes x | head -c {SIZE}; yes y | head -c {SIZE} >&2"
            )),
            Duration::from_secs(30),
            "big output",
        )
        .expect("大输出不该超时——超时即意味着 pipe 死锁回归");

        assert!(output.status.success());
        assert_eq!(
            output.stdout.len(),
            SIZE,
            "stdout 须完整收集，不能被 pipe 容量截断"
        );
        assert_eq!(output.stderr.len(), SIZE, "stderr 须完整收集");
    }

    /// 大输出命令超时时须**及时**返回 `Timeout`，不能等到子进程自然结束。
    ///
    /// `kill` 只终止直接子进程，孙进程继承 pipe 写端后 EOF 会迟到；若超时路径 join 读线程，
    /// 2 秒的超时会被拖成孙进程的 60 秒——实测这样写会 60.8s 通过，**断言全绿而超时语义已废**。
    /// 故这里断言的是**耗时**，不只是错误类型。
    #[test]
    fn test_run_with_timeout_returns_promptly_on_slow_big_output() {
        let started = std::time::Instant::now();
        let result = run_with_timeout(
            // 先吐一批把 pipe 灌到接近满，再让孙进程长睡：必须靠超时 kill 才能结束。
            Command::new("sh")
                .arg("-c")
                .arg("yes x | head -c 200000; sleep 60"),
            Duration::from_secs(2),
            "slow big output",
        );
        let elapsed = started.elapsed();

        assert!(
            matches!(result, Err(MigrateError::Timeout { .. })),
            "应报 Timeout: {result:?}"
        );
        assert!(
            elapsed < Duration::from_secs(20),
            "超时须及时返回，不得等满孙进程的 60s（实测 {elapsed:?}）——join 读线程会导致此回归"
        );
    }
}
