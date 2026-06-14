//! CLI 集成测试：Thin E2E 链路 + 各命令 smoke。
//!
//! 复用 `run_with_args(args, writer)` 模式：传 `Vec<u8>` 捕获输出，
//! 断言输出是合法 JSON 且 `status` 字段正确。
//!
//! 注意：CLI 的 `.rust-migration/` 与 `--root .` 等路径相对当前工作目录，
//! 测试通过 `with_cwd` 在临时目录内运行，并用全局锁串行化（cwd 是进程级状态）。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rustmigrate_cli::run_with_args;
use serde_json::Value;

/// 进程级 cwd 锁：任何切换 cwd 的测试都必须先持有它，避免并行竞态。
fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 仓库 fixtures 目录。
fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/cli -> crates -> cli -> repo root
    let repo_root = manifest.ancestors().nth(3).unwrap();
    repo_root.join("fixtures")
}

/// 递归复制目录（仅文件与子目录，足够覆盖 fixture）。
fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

/// 在指定目录内执行闭包（持有 cwd 锁，结束后恢复原 cwd）。
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

/// 运行 CLI，返回 (退出码, 解析后的 JSON)。
fn run(args: &[&str]) -> (i32, Value) {
    let mut full: Vec<&str> = vec!["rustmigrate"];
    full.extend_from_slice(args);
    let mut buf: Vec<u8> = Vec::new();
    let code = run_with_args(full, &mut buf);
    let text = String::from_utf8(buf).expect("输出应为 UTF-8");
    let json: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("输出非合法 JSON: {e}\n原始: {text}"));
    (code, json)
}

/// 准备一个 linear-deps fixture 的临时副本目录。
fn temp_linear_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("linear-deps"), tmp.path());
    tmp
}

/// 在排序结果中查找节点位置（兼容有无 src/ 前缀）。
fn find_position(order: &[Value], name: &str) -> Option<usize> {
    order.iter().position(|v| {
        let s = v.as_str().unwrap_or_default();
        s == format!("file:{name}") || s == format!("file:src/{name}")
    })
}

// === Thin E2E：init -> graph build -> graph topo-sort ===

#[test]
fn e2e_init_build_topo_linear_deps() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        // 1. init
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0, "init 应成功");
        assert_eq!(json["status"], "ok");
        assert!(Path::new(".rust-migration/migration-state.json").exists());

        // 2. graph build --root src
        let (code, json) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0, "graph build 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(json["data"]["node_count"].as_u64().unwrap() > 0);
        assert!(Path::new(".rust-migration/source-graph.db").exists());

        // 3. graph topo-sort
        let (code, json) = run(&["graph", "topo-sort"]);
        assert_eq!(code, 0, "topo-sort 应成功: {json}");
        assert_eq!(json["status"], "ok");

        // 4. 断言拓扑序满足 ground-truth 偏序：utils < service < index
        let order = json["data"]["order"].as_array().expect("order 应为数组");
        let pos_utils = find_position(order, "utils.ts").expect("应含 utils.ts");
        let pos_service = find_position(order, "service.ts").expect("应含 service.ts");
        let pos_index = find_position(order, "index.ts").expect("应含 index.ts");
        assert!(
            pos_utils < pos_service,
            "utils 应排在 service 前: {order:?}"
        );
        assert!(
            pos_service < pos_index,
            "service 应排在 index 前: {order:?}"
        );
    });
}

#[test]
fn e2e_topo_sort_circular_returns_nonzero_and_cycles() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let (code, _) = run(&["init"]);
        assert_eq!(code, 0);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        // 有环：非零退出（退出码 2）+ 列出环路径。
        let (code, json) = run(&["graph", "topo-sort"]);
        assert_eq!(code, 2, "有环应非零退出: {json}");
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["kind"], "cyclic_dependency");
        let cycles = json["data"]["cycle_path"]
            .as_array()
            .expect("应含 cycle_path");
        assert!(!cycles.is_empty(), "应列出至少一个环");
    });
}

// === 各命令 smoke：合法 JSON + status 正确 ===

#[test]
fn smoke_init() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["message"], "initialized");
        // init 同时生成项目根 .rustmigrate.toml（设计 06:89）。
        assert!(json["data"]["config_file"].is_string());
        assert!(
            Path::new(".rustmigrate.toml").exists(),
            "init 应生成 .rustmigrate.toml"
        );
    });
}

#[test]
fn smoke_init_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0);
        assert_eq!(json["data"]["already_initialized"], true);
    });
}

#[test]
fn smoke_profile() {
    let project = temp_linear_project();
    let root = project.path().join("src");
    // 未传 --adapter-tools：跳过适配器工具检测但仍检测 cargo-nextest，产出 warning。
    let (code, json) = run(&["profile", "--root", root.to_str().unwrap()]);
    assert_eq!(code, 0, "profile 应成功: {json}");
    assert_eq!(json["status"], "warning");
    assert_eq!(json["data"]["primary_language"], "typescript");
    // tool_checks 至少含 cargo-nextest 一项。
    let checks = json["data"]["tool_checks"].as_array().unwrap();
    assert!(
        checks.iter().any(|c| c["tool_id"] == "cargo-nextest"),
        "tool_checks 应含 cargo-nextest: {json}"
    );
    // 未提供 adapter-tools 的跳过提示。
    assert!(
        json["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("adapter-tools")),
        "应含跳过适配器工具检测的 warning: {json}"
    );
}

#[test]
fn smoke_profile_with_adapter_tools() {
    let project = temp_linear_project();
    let root = project.path().join("src");
    // 含必不存在工具的 analysis-tools.json，验证 ADAPTER_TOOL_MISSING 路径。
    let tools = project.path().join("analysis-tools.json");
    std::fs::write(
        &tools,
        r#"[{"tool_id":"definitely-not-real-bin-xyz","display_name":"Ghost","min_version":"1.0.0","install_hint":"install ghost","required":true}]"#,
    )
    .unwrap();
    let (code, json) = run(&[
        "profile",
        "--root",
        root.to_str().unwrap(),
        "--adapter-tools",
        tools.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "profile 应成功: {json}");
    assert_eq!(json["status"], "warning");
    assert!(
        json["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("ADAPTER_TOOL_MISSING")),
        "应含 ADAPTER_TOOL_MISSING warning: {json}"
    );
}

#[test]
fn smoke_graph_build() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        assert_eq!(json["status"], "ok");
        assert!(json["data"]["edge_count"].as_u64().is_some());
        // M1 不传 --full 也是全量构建，full 字段恒 true（不再误报 false）。
        assert_eq!(json["data"]["full"], true);
    });
}

#[test]
fn smoke_graph_build_full_flag() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["graph", "build", "--root", "src", "--full"]);
        assert_eq!(code, 0);
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["full"], true);
    });
}

#[test]
fn smoke_graph_stats() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["graph", "build", "--root", "src"]);
        let (code, json) = run(&["graph", "stats"]);
        assert_eq!(code, 0, "graph stats 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(json["data"]["total_nodes"].as_u64().unwrap() > 0);
    });
}

#[test]
fn smoke_graph_deps() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["graph", "build", "--root", "src"]);
        // index.ts 依赖 service.ts，service.ts 依赖 utils.ts（传递闭包应含两者）。
        let (code, json) = run(&["graph", "deps", "index.ts"]);
        assert_eq!(code, 0, "graph deps 应成功: {json}");
        assert_eq!(json["status"], "ok");
        let deps = json["data"]["dependencies"].as_array().unwrap();
        assert!(
            deps.iter()
                .any(|d| d.as_str().unwrap().contains("utils.ts")),
            "index.ts 的传递依赖应含 utils.ts: {deps:?}"
        );
    });
}

#[test]
fn smoke_graph_interfaces() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["graph", "build", "--root", "src"]);
        let (code, json) = run(&["graph", "interfaces", "utils.ts"]);
        assert_eq!(code, 0, "graph interfaces 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(json["data"]["interfaces"].is_array());
    });
}

#[test]
fn smoke_graph_deps_missing_module_errors() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["graph", "build", "--root", "src"]);
        let (code, json) = run(&["graph", "deps", "does-not-exist.ts"]);
        assert_eq!(code, 1, "不存在的模块应报错");
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["kind"], "graph");
    });
}

#[test]
fn smoke_validate_state() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["validate", "state"]);
        assert_eq!(code, 0, "validate state 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["valid"], true);
    });
}

/// 向 init 生成的 migration-state.json 注入一个模块（测试辅助）。
fn inject_module(status: &str) {
    let path = std::path::Path::new(".rust-migration").join("migration-state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    state["modules"]["a"] = serde_json::json!({ "status": status });
    std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

#[test]
fn smoke_state_transition() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module("pending");

        // 合法转换 pending → translating。
        let (code, json) = run(&[
            "state",
            "transition",
            "--module",
            "a",
            "--to",
            "translating",
            "--reason",
            "kick off",
        ]);
        assert_eq!(code, 0, "合法转换应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["status"], "translating");
        // reason 落盘到 attempts。
        assert_eq!(
            json["data"]["state"]["attempts"]
                .as_array()
                .map(|a| a.len()),
            Some(1)
        );

        // 仅更新 substatus（无 --to），status 不变。
        let (code, json) = run(&[
            "state",
            "transition",
            "--module",
            "a",
            "--substatus",
            "phase_a_complete_awaiting_review",
        ]);
        assert_eq!(code, 0);
        assert_eq!(json["data"]["status"], "translating");
        assert_eq!(
            json["data"]["substatus"],
            "phase_a_complete_awaiting_review"
        );

        // 非法转换 translating → done（缺中间态）应报错。
        let (code, json) = run(&["state", "transition", "--module", "a", "--to", "done"]);
        assert_eq!(code, 1, "非法转换应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_state_transition_invalid_status() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module("pending");
        let (code, json) = run(&["state", "transition", "--module", "a", "--to", "bogus"]);
        assert_eq!(code, 1, "非法 ModuleStatus 应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_state_get_missing_errors() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["state", "get", "nonexistent"]);
        assert_eq!(code, 1, "不存在模块应报错");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_stats_loc() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 造源码与 Rust 两侧文件。
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        std::fs::write("rust-src/a.rs", "pub fn x() -> i32 {\n    1\n}\n").unwrap();

        let (code, json) = run(&["stats", "loc"]);
        assert_eq!(code, 0, "stats loc 应成功: {json}");
        assert_eq!(json["status"], "ok");
        // 源码侧统计到 TypeScript，Rust 侧统计到 Rust。
        assert!(json["data"]["source"]["code"].as_u64().unwrap() >= 1);
        assert!(
            json["data"]["source"]["by_language"]["TypeScript"].is_object(),
            "源码侧应含 TypeScript: {json}"
        );
        assert!(json["data"]["rust"]["code"].as_u64().unwrap() >= 1);
        assert!(
            json["data"]["rust"]["by_language"]["Rust"].is_object(),
            "Rust 侧应含 Rust: {json}"
        );
    });
}

#[test]
fn smoke_stats_loc_missing_rust_root() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        std::fs::create_dir_all("src").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        // rust-src 不存在 → rust 侧为 null + warning，命令不失败。
        let (code, json) = run(&["stats", "loc"]);
        assert_eq!(code, 0, "缺 rust 目录不应失败: {json}");
        assert_eq!(json["status"], "warning");
        assert!(json["data"]["rust"].is_null(), "rust 侧应为 null: {json}");
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("rust 目录不存在")),
            "应含 rust 目录缺失 warning: {json}"
        );
    });
}

#[test]
fn smoke_stats_compare_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        // stats compare 推迟 M2，返回带 warning 的占位响应。
        let (code, json) = run(&["stats", "compare"]);
        assert_eq!(code, 0, "占位响应应成功: {json}");
        // 带 warnings 时 status 为 warning。
        assert_eq!(json["status"], "warning");
        assert_eq!(json["data"]["implemented"], false);
    });
}

#[test]
fn smoke_scaffold_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("rust");
    // scaffold 委托 `cargo init` 子进程，子进程继承当前 cwd；
    // 在稳定的 tmp 目录内运行并持有 cwd 锁，避免与改 cwd 的测试竞态。
    with_cwd(tmp.path(), || {
        let (code, json) = run(&[
            "scaffold",
            "workspace",
            "--name",
            "demo_crate",
            "--target",
            target.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "scaffold workspace 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(target.join("Cargo.toml").exists(), "应生成 Cargo.toml");
    });
}

#[test]
fn cli_parse_error_emits_unified_json() {
    // clap 解析失败（非法参数）应走统一 JSON 错误，不输出 clap 裸文本，退出码 1。
    let (code, json) = run(&["graph", "build", "--bogus-flag"]);
    assert_eq!(code, 1, "解析失败应退出 1: {json}");
    assert_eq!(json["status"], "error");
    assert_eq!(json["data"]["kind"], "cli_parse");
}

#[test]
fn cli_help_is_plain_text_exit_zero() {
    // --help 是正常输出：原样文本 + 退出码 0（不包成 JSON 错误）。
    let mut buf: Vec<u8> = Vec::new();
    let code = run_with_args(["rustmigrate", "--help"], &mut buf);
    assert_eq!(code, 0, "--help 应退出 0");
    let text = String::from_utf8(buf).unwrap();
    assert!(
        serde_json::from_str::<Value>(text.trim()).is_err(),
        "--help 应为纯文本而非 JSON: {text}"
    );
}

// === 错误路径：未构建图时读命令应报错（非 panic） ===

#[test]
fn topo_sort_without_db_errors() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["graph", "topo-sort"]);
        assert_eq!(code, 1, "无 db 时应返回一般错误");
        assert_eq!(json["status"], "error");
    });
}
