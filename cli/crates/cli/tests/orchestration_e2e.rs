//! 并行 + worktree 编排端到端集成测试（M4-ORCH-01 PR-4）。
//!
//! 补 [MDR-018](../../../../docs/decisions/018-keep-parallel-migration.md):47 指出的
//! 「端到端集成测试完全不存在」硬门。编排本身是 LLM 散文驱动（CLI-vs-Plugin 边界：
//! 确定性给 CLI、编排判断给 LLM），故本测试用 **Rust harness 扮演编排器角色**：
//! 确定性驱动真实 CLI（`run_with_args`）+ 真实 git（worktree/merge）+ 真实 cargo（整组 check），
//! 用预置的「翻译产物」fixture 冒充 SubAgent 回传，**不跑真 LLM**。
//!
//! 覆盖 workflow.md 步骤 2a-2d 全链：
//! 分层 → 每模块 worktree 隔离 → 逐层 git merge → 整组 `cargo check` 真门 → 两层 done。
//!
//! 三类用例：
//! - `happy_path`：多模块并行、写盘不冲突（模拟契约门已冻结共享写面）→ 整组 check 绿 → 全升 done。
//! - `merge_conflict`：两模块同改共享文件 → 第二个 merge 冲突 → abort + 标记重译（MDR-003 约束7）。
//! - `whole_group_check_gate`：一模块产物编译不过 → 整组 check 失败 → 真门拦下，无一升 done。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use rustmigrate_cli::run_with_args;
use rustmigrate_core::types::parallel::{
    AgentStatus, CheckStatus, DependencyInterface, PortingRules, TranslationDispatch,
    TranslationResult,
};
use serde_json::Value;

// =========================================================================
// 通用测试基建（cwd 锁 + CLI 直调 + git/cargo 子进程）
// =========================================================================

/// 进程级 cwd 锁：CLI 的 `.rust-migration/` 相对 cwd 解析，切 cwd 的测试须串行化。
fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 在指定目录内执行闭包（持 cwd 锁，结束恢复原 cwd）。
fn with_cwd<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::set_current_dir(&original).unwrap();
    match result {
        Ok(v) => v,
        Err(p) => std::panic::resume_unwind(p),
    }
}

/// 进程内直调 CLI，返回 `(exit_code, json)`。
fn cli(args: &[&str]) -> (i32, Value) {
    let mut full: Vec<&str> = vec!["rustmigrate"];
    full.extend_from_slice(args);
    let mut buf: Vec<u8> = Vec::new();
    let code = run_with_args(full, &mut buf);
    let text = String::from_utf8(buf).expect("CLI 输出应为 UTF-8");
    let json: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("CLI 输出应为合法 JSON: {e}\n原文: {text}"));
    (code, json)
}

/// 在指定 repo 目录跑 git 子命令，断言成功；返回 stdout。
fn git_ok(repo: &Path, args: &[&str]) -> String {
    let out = git_raw(repo, args);
    assert!(
        out.status.success(),
        "git {args:?} 应成功\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 跑 git 子命令，返回原始 Output（调用方自行判 status，用于可能失败的 merge）。
fn git_raw(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(repo)
        // 显式注入身份 + 关钩子/签名，避免 CI 环境 git 未配置身份而失败。
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=orch-test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("应能执行 git")
}

/// 在指定目录跑 `cargo check`，返回 `(是否通过, 合并的 stdout+stderr)`（真门）。
///
/// 显式隔离 `CARGO_TARGET_DIR` 到 crate 自己的 target/：对齐设计（run.md/workflow.md
/// 要求 per-worktree target 防并行编译锁争用），也避免外部环境导出的共享 target 让
/// 多个同名 `demo` crate 相互踩踏。返回诊断文本供调用方断言命中的具体 rustc 错误
/// （而非只看退出码——防基础设施故障导致的非预期非零退出被误判为「真门拦截」）。
fn cargo_check(crate_dir: &Path) -> (bool, String) {
    let out = Command::new("cargo")
        .current_dir(crate_dir)
        .env("CARGO_TARGET_DIR", crate_dir.join("target"))
        .args(["check", "--quiet"])
        .output()
        .expect("应能执行 cargo");
    let mut diag = String::from_utf8_lossy(&out.stdout).into_owned();
    diag.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), diag)
}

/// 建一个含初始提交的 git 仓库 + 最小可编译 lib crate。
///
/// 结构：`Cargo.toml`（lib crate `demo`）+ `src/lib.rs`（声明 mod）+ 各 mod 的空 stub 文件。
/// `mods` 为要预声明的模块名（对应后续每个并行模块填空的 own 文件）。
fn init_repo_with_crate(repo: &Path, mods: &[&str]) {
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();

    // lib.rs 预声明所有 mod（模拟契约门冻结的骨架），各 mod 先给空 stub（合法可编译）。
    let lib_rs: String = mods.iter().map(|m| format!("pub mod {m};\n")).collect();
    std::fs::write(repo.join("src/lib.rs"), lib_rs).unwrap();
    for m in mods {
        std::fs::write(
            repo.join(format!("src/{m}.rs")),
            format!("// stub for {m}, awaiting translation\n"),
        )
        .unwrap();
    }

    git_ok(repo, &["init", "-q", "-b", "main"]);
    git_ok(repo, &["add", "-A"]);
    git_ok(repo, &["commit", "-q", "-m", "init crate skeleton"]);
}

/// `.rust-migration/migration-state.json` 路径（相对 repo）。
fn state_file(repo: &Path) -> PathBuf {
    repo.join(".rust-migration/migration-state.json")
}

/// 直接注入一个 sprint 1 的模块（status=translating，模拟已派发）。
/// 绕过 populate（无需真实 graph），对齐 cli_e2e 的 inject 辅助风格。
fn inject_module(repo: &Path, name: &str, status: &str) {
    let path = state_file(repo);
    let mut state: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    state["modules"][name] = serde_json::json!({ "status": status, "sprint": 1 });
    std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

/// 读取某模块当前 status。
fn module_status(repo: &Path, name: &str) -> String {
    let path = state_file(repo);
    let state: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    state["modules"][name]["status"]
        .as_str()
        .unwrap_or("<none>")
        .to_owned()
}

// =========================================================================
// mock SubAgent：在 worktree 内产出「翻译产物」并回传 TranslationResult
// =========================================================================

/// 模拟编排器派发 + SubAgent 在隔离 worktree 内翻译的一路。
///
/// 1. 编排器构造 `TranslationDispatch`（让协议类型获得非测试消费者）。
/// 2. `git worktree add .wt/{module} -b wt/{module}`。
/// 3. SubAgent 把 `product`（Rust 代码）写进 worktree 内 `rel_path`，worktree 内 commit。
/// 4. 回传 `TranslationResult { agent_done, ... }`。
///
/// 返回回传结果 + 分支名。
fn dispatch_and_translate(
    repo: &Path,
    module: &str,
    rel_path: &str,
    product: &str,
) -> (TranslationResult, String) {
    let wt_dir = repo.join(format!(".wt/{module}"));
    let branch = format!("wt/{module}");

    // ① 编排器派发数据（消费协议类型）。
    let dispatch = TranslationDispatch {
        module_key: module.to_owned(),
        worktree_path: wt_dir.clone(),
        dependency_interfaces: vec![DependencyInterface {
            module_key: "file:src/lib.rs".to_owned(),
            exports: vec![],
        }],
        porting_rules: PortingRules::default(),
    };
    assert_eq!(dispatch.module_key, module);

    // ② 手动 worktree（编排器掌握固定路径 + 分支名，PR-2 决策）。
    git_ok(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            wt_dir.to_str().unwrap(),
            "main",
        ],
    );

    // ③ SubAgent 写产物 + worktree 内提交（代码留盘，只回传 touched-list）。
    std::fs::write(wt_dir.join(rel_path), product).unwrap();
    git_ok(&wt_dir, &["add", "-A"]);
    git_ok(
        &wt_dir,
        &["commit", "-q", "-m", &format!("translate {module}")],
    );

    // ④ 回传（编排器解析 TranslationResult）。
    let result = TranslationResult {
        module_key: module.to_owned(),
        status: AgentStatus::AgentDone,
        own_files: vec![PathBuf::from(rel_path)],
        shared_touched: vec![],
        self_check: CheckStatus::Pass,
        test: CheckStatus::Skipped,
    };
    (result, branch)
}

// =========================================================================
// 用例 1：happy path —— 并行、写盘不冲突、整组 check 绿、两层 done 全升 done
// =========================================================================

#[test]
fn orch_happy_path_two_modules_reach_done() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo_with_crate(repo, &["a", "b"]);

    with_cwd(repo, || {
        let (code, _) = cli(&["init"]);
        assert_eq!(code, 0, "init 应成功");
        inject_module(repo, "a", "translating");
        inject_module(repo, "b", "translating");

        // 2a. 同层并行派发：两模块各自 worktree 翻译，写盘不冲突（各填自己的 own 文件）。
        let (ra, br_a) = dispatch_and_translate(repo, "a", "src/a.rs", "pub fn a() -> i32 { 1 }\n");
        let (rb, br_b) = dispatch_and_translate(repo, "b", "src/b.rs", "pub fn b() -> i32 { 2 }\n");
        let results = [(ra, br_a), (rb, br_b)];

        // 回传后编排器**据 TranslationResult 驱动**：只对 AgentStatus::AgentDone 的模块标 agent_done
        // substatus（非终态）。用 result.module_key 而非硬编码模块名——协议类型真正驱动编排决策。
        let agent_done: Vec<&str> = results
            .iter()
            .filter(|(r, _)| r.status == AgentStatus::AgentDone)
            .map(|(r, _)| r.module_key.as_str())
            .collect();
        assert_eq!(agent_done, vec!["a", "b"], "两模块均应回传 agent_done");
        for m in &agent_done {
            let (code, _) = cli(&[
                "state",
                "transition",
                "--module",
                m,
                "--substatus",
                "agent_done",
            ]);
            assert_eq!(code, 0);
        }

        // 2c. 逐层合并（写盘不冲突 → 均成功）。合并顺序取回传结果里的分支名。
        git_ok(repo, &["checkout", "-q", "main"]);
        for (_, br) in &results {
            let out = git_raw(repo, &["merge", "-q", "--no-edit", br]);
            assert!(out.status.success(), "无冲突模块应合并成功: {br}");
        }

        // 2d. 整组验证真门：真跑 cargo check。
        let (passed, diag) = cargo_check(repo);
        assert!(passed, "整组 check 应通过\n{diag}");

        // 编排器把本层模块推进到 reviewing（workflow.md 补的中间步：translating→testing→reviewing），
        // agent_done substatus 在 --to testing/reviewing 时保留（仅 Done 清空）。
        for m in &agent_done {
            for to in ["testing", "reviewing"] {
                let (code, _) = cli(&["state", "transition", "--module", m, "--to", to]);
                assert_eq!(code, 0, "{m} → {to} 应合法");
            }
        }

        // 两层 done 第二层门：整组 check 过 → batch 升 done。
        let (code, json) = cli(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "b",
        ]);
        assert_eq!(code, 0, "batch 应成功: {json}");
        assert_eq!(json["data"]["succeeded"].as_array().unwrap().len(), 2);
        assert!(json["data"]["skipped"].as_array().unwrap().is_empty());

        assert_eq!(module_status(repo, "a"), "done");
        assert_eq!(module_status(repo, "b"), "done");
    });
}

// =========================================================================
// 用例 2：merge 冲突 —— 两模块同改共享文件，第二个 merge 冲突 → abort + 标记重译
// =========================================================================

#[test]
fn orch_merge_conflict_aborts_and_marks_rework() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    // 预置一个共享文件 error.rs，两模块都会改它同一处 → 制造真实 merge 冲突。
    init_repo_with_crate(repo, &["a", "b", "error"]);
    // 给 error.rs 一个可编译初值。
    std::fs::write(repo.join("src/error.rs"), "pub enum E {\n    Base,\n}\n").unwrap();
    git_ok(repo, &["add", "-A"]);
    git_ok(repo, &["commit", "-q", "-m", "seed error.rs"]);

    with_cwd(repo, || {
        let (code, _) = cli(&["init"]);
        assert_eq!(code, 0);
        inject_module(repo, "a", "translating");
        inject_module(repo, "b", "translating");

        // 两模块都往 error.rs 同一处（Base 变体后）加不同变体 → 冲突。
        let (_, br_a) = dispatch_and_translate(
            repo,
            "a",
            "src/error.rs",
            "pub enum E {\n    Base,\n    VariantA,\n}\n",
        );
        let (_, br_b) = dispatch_and_translate(
            repo,
            "b",
            "src/error.rs",
            "pub enum E {\n    Base,\n    VariantB,\n}\n",
        );

        for m in ["a", "b"] {
            let (code, _) = cli(&[
                "state",
                "transition",
                "--module",
                m,
                "--substatus",
                "agent_done",
            ]);
            assert_eq!(code, 0);
        }

        // 逐层合并：a 成功，b 冲突。
        git_ok(repo, &["checkout", "-q", "main"]);
        let out_a = git_raw(repo, &["merge", "-q", "--no-edit", &br_a]);
        assert!(out_a.status.success(), "第一个模块应合并成功");

        let out_b = git_raw(repo, &["merge", "-q", "--no-edit", &br_b]);
        assert!(
            !out_b.status.success(),
            "第二个模块同改共享文件应 merge 冲突"
        );

        // 冲突文件列表（workflow.md:118 的机制）。
        let conflicts = git_ok(repo, &["diff", "--name-only", "--diff-filter=U"]);
        assert!(
            conflicts.contains("src/error.rs"),
            "冲突文件应含 error.rs: {conflicts}"
        );

        // 编排器 abort + 标记冲突模块重译（workflow.md 2c reconcile：git merge --abort →
        // 冲突模块在各自 worktree 内 rebase 后重译，概念上仍 translating；reconcile 轮次
        // 耗尽（max_reconcile_rounds）才降级 → paused）。这里模拟轮次耗尽的降级：
        // translating → paused 不是合法边（矩阵 Translating => CompileFixing|Testing|Blocked），
        // 降级路径经 Blocked 中转或由 run.md 失败恢复标 paused；本测试直接验证「冲突检出 +
        // abort + 冲突模块不进 done」这一 reconcile 核心不变量，降级态取合法的 compile_fixing
        // （workflow.md 2d 归因表「跨模块冲突 → 相关模块整组回 compile_fixing」正是此边）。
        git_ok(repo, &["merge", "--abort"]);
        let (code, _) = cli(&[
            "state",
            "transition",
            "--module",
            "b",
            "--to",
            "compile_fixing",
            "--reason",
            "reconcile 冲突：src/error.rs（跨模块冲突回 compile_fixing）",
        ]);
        assert_eq!(
            code, 0,
            "translating → compile_fixing（跨模块冲突重译）应合法"
        );

        // 冲突模块不得误升 done；已合并模块 a 仍是 translating+agent_done（未走到 done）。
        assert_eq!(
            module_status(repo, "b"),
            "compile_fixing",
            "冲突模块应标 compile_fixing 待重译"
        );
        assert_ne!(
            module_status(repo, "a"),
            "done",
            "a 尚未走完整组门，不应是 done"
        );

        // 真门守护：即便对 b 误调 batch，也因 status≠reviewing 被拒。
        let (code, json) = cli(&["state", "batch-transition-done", "--module", "b"]);
        assert_eq!(code, 0);
        assert!(
            json["data"]["succeeded"].as_array().unwrap().is_empty(),
            "compile_fixing 模块不应被 batch 升 done: {json}"
        );
    });
}

// =========================================================================
// 用例 3：整组 check 真门拦截 —— 一模块产物编译不过 → 无一升 done
// =========================================================================

#[test]
fn orch_whole_group_check_gate_blocks_done() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo_with_crate(repo, &["a", "b"]);

    with_cwd(repo, || {
        let (code, _) = cli(&["init"]);
        assert_eq!(code, 0);
        inject_module(repo, "a", "translating");
        inject_module(repo, "b", "translating");

        // a 正常；b 引用不存在的符号 → 单独 worktree 内 agent 可能自检漏判，
        // 但合并后整组 check 必失败（模拟「跨并发兄弟冲突只有整组编译才暴露」）。
        let (_, br_a) = dispatch_and_translate(repo, "a", "src/a.rs", "pub fn a() -> i32 { 1 }\n");
        let (_, br_b) = dispatch_and_translate(
            repo,
            "b",
            "src/b.rs",
            "pub fn b() -> i32 { nonexistent_symbol() }\n",
        );

        for m in ["a", "b"] {
            let (code, _) = cli(&[
                "state",
                "transition",
                "--module",
                m,
                "--substatus",
                "agent_done",
            ]);
            assert_eq!(code, 0);
        }

        // 合并（写盘不冲突，均成功）。
        git_ok(repo, &["checkout", "-q", "main"]);
        for br in [&br_a, &br_b] {
            let out = git_raw(repo, &["merge", "-q", "--no-edit", br]);
            assert!(out.status.success());
        }

        // 整组 check 真门：必失败，且断言命中**预期的**未定义符号错误（E0425）——
        // 而非任意非零退出（防基础设施故障如 target 权限/磁盘让 check 因无关原因失败而误判真门）。
        let (passed, diag) = cargo_check(repo);
        assert!(!passed, "含未定义符号的整组 check 必须失败（真门）");
        assert!(
            diag.contains("E0425") || diag.contains("nonexistent_symbol"),
            "check 失败应因预期的未定义符号（E0425），实际诊断:\n{diag}"
        );

        // 真门失败 → 编排器不推进到 reviewing、不调 batch done。
        // 模拟：直接对仍处 translating+agent_done 的模块调 batch → 全被矩阵拒绝跳过。
        // 注：此处 batch 被拒的直接原因是 status≠reviewing（编排器据真门失败**没有**执行
        // testing→reviewing 推进）——CLI 强制的是「非 reviewing 不得 done」，整组 check 门本身
        // 坐落编排器判断（LLM/harness），故本用例验证的是「真门失败 → 不推进 → batch 拒」这条链。
        let (code, json) = cli(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "b",
        ]);
        assert_eq!(code, 0);
        assert!(
            json["data"]["succeeded"].as_array().unwrap().is_empty(),
            "整组 check 未过时不应有模块升 done: {json}"
        );
        assert_ne!(module_status(repo, "a"), "done");
        assert_ne!(module_status(repo, "b"), "done");
    });
}
