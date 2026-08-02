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

/// 子进程退出后，等读线程抽完 pipe 的宽限期。
///
/// 只在「孙进程继承了 pipe 写端」时才会真正用到——那时 EOF 永远不来（`sh -c 'sleep 60 &
/// echo done'`：子进程毫秒级退出、后台孙进程攥着写端不放），读线程会一直阻塞在 read 上。
///
/// 取值很小（200ms）而非「宽裕」值，理由是：子进程退出时它写出的数据**已经在内核 pipe
/// 缓冲区里**，读线程只差最后那个读不到的 EOF；宽限期要覆盖的仅是内核把已有数据送达读端
/// 的延迟，不是等孙进程结束。给大值只会让「后台任务持有 pipe」这个正常场景每次都白等
/// （实测 5s 的版本在这类命令上每次罚满 5 秒），并不会让输出更完整。
///
/// 大输出不受影响：那种情形下子进程写完才退出，读线程早已在并发抽取，`wait_timeout` 返回
/// 时通常已到 EOF，根本走不到宽限期（有 1MB 的回归测试钉住）。
///
/// 超期即放弃**尚未读到**的部分（已读内容仍会返回），退出状态始终准确。两个流共享同一截止
/// 时刻，故这也是总延迟上限。
const PIPE_DRAIN_GRACE: Duration = Duration::from_millis(200);

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
    // 两个读线程各自读到 EOF。**两条路径都不能无条件 join**：`kill` 只终止直接子进程，它
    // 派生的孙进程会继承 pipe 写端，EOF 要等到孙进程也结束才到来。故取输出一律带截止时刻
    // （见 `recv_until`），否则本函数会在「子进程早已退出、孙进程还攥着 pipe」
    // 时无限期挂住——而它的全部存在意义正是「避免子进程挂死导致 CLI 永久卡住」。
    let stdout_reader = spawn_pipe_reader(child.stdout.take());
    let stderr_reader = spawn_pipe_reader(child.stderr.take());

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            // 子进程已退出。读线程通常瞬间收到 EOF；宽限期只兜住「孙进程仍持 pipe 写端」的
            // 情形——宁可返回不完整输出，也不能让调用方永久卡住。
            //
            // 两个流共享**同一个截止时刻**：串行地各给一份宽限期会让最坏耗时翻倍（孙进程同时
            // 持有两个写端时实测正好 2 倍），而宽限期的意义是限制总延迟。
            let deadline = std::time::Instant::now() + PIPE_DRAIN_GRACE;
            let stdout = recv_until(stdout_reader, deadline);
            let stderr = recv_until(stderr_reader, deadline);
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        Ok(None) => {
            // 超时——kill 子进程后立即返回，输出本就丢弃，不必等读线程。
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

/// 读线程与调用方共享的缓冲区：`(已读字节, 是否已到 EOF)`。
type PipeBuffer = std::sync::Arc<(std::sync::Mutex<Vec<u8>>, std::sync::atomic::AtomicBool)>;

/// 起一个线程把 pipe 读到 EOF，字节边读边并入共享缓冲区，读完置 EOF 标志。
///
/// **边读边存**而不是读完才整块交付：孙进程持有 pipe 写端时 EOF 永远不来，若等「读完」才
/// 能拿到数据，超期就只能返回空——而子进程写出的内容其实早已读到手。共享缓冲区让超期路径
/// 仍能取出已读部分（`sh -c 'sleep 30 & echo done'` 的 `done` 拿得到）。
///
/// 线程可能长期阻塞在 read 上，取不到 EOF 时把它留作 detached 即可——它只往这块缓冲区
/// 追加，调用方读取时加锁，无数据竞争。
fn spawn_pipe_reader<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> PipeBuffer {
    let shared: PipeBuffer = std::sync::Arc::new((
        std::sync::Mutex::new(Vec::new()),
        std::sync::atomic::AtomicBool::new(false),
    ));
    let worker = std::sync::Arc::clone(&shared);
    std::thread::spawn(move || {
        if let Some(mut pipe) = pipe {
            // 分块读：每块立即并入共享缓冲区，故超期时已读部分不丢。
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut buf) = worker.0.lock() {
                            buf.extend_from_slice(&chunk[..n]);
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        worker.1.store(true, std::sync::atomic::Ordering::Release);
    });
    shared
}

/// 取读线程的输出：读到 EOF 就立即返回全部；否则等到 `deadline` 再返回**已读部分**。
///
/// 收 `Instant` 而非 `Duration`，以便多个流**共享同一截止时刻**——否则串行等待会让最坏总
/// 延迟按流数量翻倍（各等一份宽限期的版本在两个流上实测正好罚满 2 倍）。
///
/// 超期返回的输出**可能不完整**。这是有意的取舍：调用方拿到的退出状态仍然准确、已读内容
/// 也不丢，而「永不返回」是更坏的结果。
fn recv_until(shared: PipeBuffer, deadline: std::time::Instant) -> Vec<u8> {
    while !shared.1.load(std::sync::atomic::Ordering::Acquire) {
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    shared.0.lock().map(|buf| buf.clone()).unwrap_or_default()
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
            "超时须及时返回，不得等满孙进程的 60s（实测 {elapsed:?}）"
        );
    }

    /// 子进程秒退但**孙进程仍持有 pipe 写端**时，本函数不得挂到孙进程结束。
    ///
    /// `wait_timeout` 只管子进程；EOF 却要等最后一个持有写端的进程退出。若正常退出路径无条件
    /// `join` 读线程，这里会硬等孙进程——实测子进程 4.7ms 退出而 join 花了 5.0s，且孙进程若是
    /// 守护进程就是**永久挂死**，恰好废掉本函数「避免子进程挂死导致 CLI 永久卡住」的全部意义。
    /// 超时路径当初防了这一点，正常路径漏了，故补此测试。
    ///
    /// 断言用 3s 上限：孙进程睡 30s，通过即证明没在等它（`PIPE_DRAIN_GRACE` 为 5s 且被两个流共享，
    /// 正常输出早已抽完，不会撞到宽限期）。
    #[test]
    fn test_run_with_timeout_not_blocked_by_grandchild_holding_pipe() {
        let started = std::time::Instant::now();
        let output = run_with_timeout(
            // 子进程 echo 完立即退出；后台 sleep 是孙进程，继承 pipe 写端 30 秒。
            Command::new("sh").arg("-c").arg("sleep 30 & echo done"),
            Duration::from_secs(60),
            "grandchild holds pipe",
        )
        .expect("子进程正常退出，不该报错");
        let elapsed = started.elapsed();

        assert!(output.status.success());
        assert!(
            elapsed < Duration::from_secs(3),
            "子进程已退出即须返回，不得等孙进程释放 pipe（实测 {elapsed:?}）"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("done"),
            "及时返回的同时仍须拿到已写出的输出: {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
