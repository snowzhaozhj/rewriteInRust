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
    let (code, json) = run(&["profile", "--root", root.to_str().unwrap()]);
    assert_eq!(code, 0, "profile 应成功: {json}");
    // 工具可用性检测未实现 → status 为 warning（设计 06:90 要求该检测，缺失须显式告知下游）。
    assert_eq!(json["status"], "warning");
    assert!(
        json["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("工具可用性检测")),
        "profile 应含工具检测未实现的 warning: {json}"
    );
    assert_eq!(json["data"]["primary_language"], "typescript");
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

#[test]
fn smoke_state_transition_placeholder() {
    // 模块级状态转换为占位实现（见 lib.rs cmd_state_transition）：设计要求 --to 取
    // ModuleStatus 并做合法性校验，属独立的模块级状态机功能，本 PR 未实现，诚实返回
    // implemented=false + 警告，而非用项目级 ProjectState 语义冒充。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&[
            "state",
            "transition",
            "--module",
            "all",
            "--to",
            "translating",
        ]);
        assert_eq!(code, 0, "占位响应应成功返回: {json}");
        // 占位携带「未实现」警告，Response 据此将 status 置为 warning（ok|warning|error 三态）。
        assert_eq!(json["status"], "warning");
        assert_eq!(json["data"]["implemented"], false);
        assert!(
            !json["warnings"].as_array().unwrap().is_empty(),
            "应含未实现警告: {json}"
        );
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
        let (code, json) = run(&["stats", "loc"]);
        assert_eq!(code, 0, "stats loc 应成功: {json}");
        // 带 tokei 语义偏差告警，Response 据此将 status 置为 warning（见 cmd_stats_loc）。
        assert_eq!(json["status"], "warning");
        assert_eq!(json["data"]["total_modules"], 0);
        assert!(
            json["data"]["note"].is_string(),
            "应含 note 偏差说明: {json}"
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
