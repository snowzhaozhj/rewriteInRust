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

fn build_graph_for_query() {
    let (code, json) = run(&["graph", "build", "--root", "src"]);
    assert_eq!(code, 0, "graph build 应成功，不能继续查询图: {json}");
    assert!(
        Path::new(".rust-migration/source-graph.db").exists(),
        "graph build 成功后应生成 source-graph.db: {json}"
    );
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

/// Python 源（py-linear-deps）未显式 `--full` 时：经 `detect_language` 路由到
/// Python adapter，强制全量并输出降级警告（status 降级为 warning），图非空。
/// 覆盖 `cmd_graph_build` 新增的语言探测 + 降级分支（否则该路径无回归保护）。
#[test]
fn smoke_graph_build_python_detects_and_degrades() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("py-linear-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        // 不 init：走纯 detect_language 探测路径。
        let (code, json) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0, "Python graph build 应成功: {json}");
        assert_eq!(json["status"], "warning", "降级应使 status=warning: {json}");
        assert_eq!(json["data"]["full"], true, "Python 应强制全量: {json}");
        assert!(
            json["data"]["node_count"].as_u64().unwrap_or(0) > 0,
            "Python 图应非空: {json}"
        );
        let warns = json["warnings"].as_array().expect("应有 warnings 数组");
        assert!(
            warns
                .iter()
                .any(|w| w.as_str().unwrap_or_default().contains("降级为全量构建")),
            "warnings 应含增量降级提示: {json}"
        );
    });
}

#[test]
fn smoke_graph_stats() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
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
        build_graph_for_query();
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
fn smoke_graph_rdeps() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        // utils.ts 被 service.ts 直接依赖，也被 index.ts 传递依赖。
        let (code, json) = run(&["graph", "rdeps", "utils.ts"]);
        assert_eq!(code, 0, "graph rdeps 应成功: {json}");
        assert_eq!(json["status"], "ok");
        let rdeps = json["data"]["dependents"].as_array().unwrap();
        assert!(
            rdeps
                .iter()
                .any(|d| d.as_str().unwrap().contains("service.ts")),
            "utils.ts 的反向依赖应含 service.ts: {rdeps:?}"
        );
        assert!(
            rdeps
                .iter()
                .any(|d| d.as_str().unwrap().contains("index.ts")),
            "utils.ts 的传递反向依赖应含 index.ts: {rdeps:?}"
        );
    });
}

#[test]
fn smoke_graph_rdeps_leaf_is_empty() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "rdeps", "index.ts"]);
        assert_eq!(code, 0, "graph rdeps 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(json["data"]["dependents"].as_array().unwrap().is_empty());
    });
}

#[test]
fn smoke_graph_rdeps_true_transitive_closure() {
    let project = tempfile::tempdir().unwrap();
    let src = project.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("a.ts"),
        "import { b } from './b';\nexport const a = b + 1;\n",
    )
    .unwrap();
    std::fs::write(
        src.join("b.ts"),
        "import { c } from './c';\nexport const b = c + 1;\n",
    )
    .unwrap();
    std::fs::write(src.join("c.ts"), "export const c = 1;\n").unwrap();

    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "rdeps", "c.ts"]);
        assert_eq!(code, 0, "graph rdeps 应成功: {json}");
        assert_eq!(json["status"], "ok");
        let rdeps = json["data"]["dependents"].as_array().unwrap();
        assert!(
            rdeps.iter().any(|d| d.as_str().unwrap().contains("b.ts")),
            "c.ts 的直接反向依赖应含 b.ts: {rdeps:?}"
        );
        assert!(
            rdeps.iter().any(|d| d.as_str().unwrap().contains("a.ts")),
            "c.ts 的传递反向依赖应含 a.ts: {rdeps:?}"
        );
    });
}

#[test]
fn smoke_graph_rdeps_cycle_excludes_start() {
    let project = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), project.path());
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "rdeps", "handler.ts"]);
        assert_eq!(code, 0, "graph rdeps 应成功: {json}");
        assert_eq!(json["status"], "ok");
        let rdeps = json["data"]["dependents"].as_array().unwrap();
        assert!(
            rdeps
                .iter()
                .any(|d| d.as_str().unwrap().contains("event-bus.ts")),
            "handler.ts 的反向依赖应含 event-bus.ts: {rdeps:?}"
        );
        assert!(
            rdeps
                .iter()
                .all(|d| !d.as_str().unwrap().contains("handler.ts")),
            "环依赖中 rdeps 不应把起点自身纳入 dependents: {rdeps:?}"
        );
    });
}

#[test]
fn smoke_graph_interfaces() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
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
        build_graph_for_query();
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
fn smoke_state_record_subagent_call() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);

        // 第一条：完整字段（含 started_at/ended_at/error_message）。
        let (code, json) = run(&[
            "state",
            "record-subagent-call",
            "--step-index",
            "1",
            "--subagent-name",
            "translator",
            "--status",
            "success",
            "--started-at",
            "2026-06-14T09:05:00Z",
            "--ended-at",
            "2026-06-14T09:08:30Z",
        ]);
        assert_eq!(code, 0, "记录 subagent 调用应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["recorded"], true);
        assert_eq!(json["data"]["subagent_calls_count"], 1);

        // 落盘字段正确（直接读文件断言 append-only 数组内容）。
        let path = std::path::Path::new(".rust-migration").join("migration-state.json");
        let state: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let calls = state["subagent_calls"].as_array().expect("应为数组");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["step_index"], 1);
        assert_eq!(calls[0]["subagent_name"], "translator");
        assert_eq!(calls[0]["status"], "success");
        assert_eq!(calls[0]["started_at"], "2026-06-14T09:05:00Z");
        assert_eq!(calls[0]["ended_at"], "2026-06-14T09:08:30Z");

        // 第二条：仅必填字段，started_at 省略由 CLI 取当前时间，error_message 记录失败原因。
        let (code, json) = run(&[
            "state",
            "record-subagent-call",
            "--step-index",
            "2",
            "--subagent-name",
            "verifier",
            "--status",
            "timeout",
            "--error-message",
            "exceeded 600s",
        ]);
        assert_eq!(code, 0, "第二次记录应成功: {json}");
        // append 到长度 2。
        assert_eq!(json["data"]["subagent_calls_count"], 2);

        let state: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let calls = state["subagent_calls"].as_array().expect("应为数组");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1]["subagent_name"], "verifier");
        assert_eq!(calls[1]["status"], "timeout");
        assert_eq!(calls[1]["error_message"], "exceeded 600s");
        // started_at 缺省也应落非空时间戳（schema 必填）。
        assert!(calls[1]["started_at"]
            .as_str()
            .is_some_and(|s| !s.is_empty()));
        // ended_at 省略时不序列化（skip_serializing_if）。
        assert!(calls[1].get("ended_at").is_none());
    });
}

#[test]
fn e2e_record_subagent_call_without_init_errors() {
    // 无 init（无 migration-state.json）时调用应返回明确错误，而非 panic 或静默成功。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&[
            "state",
            "record-subagent-call",
            "--step-index",
            "1",
            "--subagent-name",
            "translator",
            "--status",
            "success",
        ]);
        assert_eq!(code, 1, "无 init 应报错: {json}");
        assert_eq!(json["status"], "error");
        let msg = json["data"]["message"].as_str().unwrap_or_default();
        assert!(
            msg.contains("文件不存在"),
            "错误信息应提示状态文件不存在: {json}"
        );
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
fn smoke_state_transition_requires_to_or_substatus() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module("pending");
        // --to 与 --substatus 都缺省应报错。
        let (code, json) = run(&["state", "transition", "--module", "a"]);
        assert_eq!(code, 1, "缺 --to/--substatus 应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_state_transition_degrade_force() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module("degrade_manual");
        // 不带 --force：degrade_* → translating 应被拒。
        let (code, json) = run(&[
            "state",
            "transition",
            "--module",
            "a",
            "--to",
            "translating",
        ]);
        assert_eq!(code, 1, "degrade 恢复不带 --force 应报错: {json}");
        assert_eq!(json["status"], "error");
        // 带 --force：成功。
        let (code, json) = run(&[
            "state",
            "transition",
            "--module",
            "a",
            "--to",
            "translating",
            "--force",
        ]);
        assert_eq!(code, 0, "degrade 恢复带 --force 应成功: {json}");
        assert_eq!(json["data"]["status"], "translating");
    });
}

#[test]
fn smoke_state_transition_project_level() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]); // 项目 state=init

        // 无 --module：项目级 init → profile → plan → scaffold → sprint_loop 合法。
        // graduate 须走 `rustmigrate graduate` 命令（含前置检查），不允许 state transition。
        for (from, to) in [
            ("init", "profile"),
            ("profile", "plan"),
            ("plan", "scaffold"),
            ("scaffold", "sprint_loop"),
        ] {
            let (code, json) = run(&["state", "transition", "--to", to]);
            assert_eq!(code, 0, "项目级 {from}→{to} 应成功: {json}");
            assert_eq!(json["status"], "ok");
            assert_eq!(json["data"]["from"], from);
            assert_eq!(json["data"]["state"], to);
        }
        // graduate 通过 state transition 应被拒绝。
        let (code, json) = run(&["state", "transition", "--to", "graduate"]);
        assert_eq!(code, 1, "graduate 不应通过 state transition 推进: {json}");
    });
}

#[test]
fn smoke_state_transition_project_illegal_jump() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]); // state=init
                                // 跳过中间态 init → sprint_loop 非法。
        let (code, json) = run(&["state", "transition", "--to", "sprint_loop"]);
        assert_eq!(code, 1, "非法项目级跳转应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_state_transition_project_rejects_module_args() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 项目级不支持 --substatus。
        let (code, json) = run(&["state", "transition", "--to", "profile", "--substatus", "x"]);
        assert_eq!(code, 1, "项目级带 --substatus 应报错: {json}");
        assert_eq!(json["status"], "error");
        // 项目级不支持 --force。
        let (code, json) = run(&["state", "transition", "--to", "profile", "--force"]);
        assert_eq!(code, 1, "项目级带 --force 应报错: {json}");
        assert_eq!(json["status"], "error");
        // 项目级不支持 --reason（与 substatus/force 一致，不静默吞参）。
        let (code, json) = run(&["state", "transition", "--to", "profile", "--reason", "x"]);
        assert_eq!(code, 1, "项目级带 --reason 应报错: {json}");
        assert_eq!(json["status"], "error");
        // 缺 --to 报错。
        let (code, json) = run(&["state", "transition"]);
        assert_eq!(code, 1, "项目级缺 --to 应报错: {json}");
        assert_eq!(json["status"], "error");
        // 非法 ProjectState 报错。
        let (code, json) = run(&["state", "transition", "--to", "bogus"]);
        assert_eq!(code, 1, "非法 ProjectState 应报错: {json}");
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
fn smoke_stats_loc_overlapping_roots_warns() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        std::fs::create_dir_all("src/rust").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        std::fs::write("src/rust/a.rs", "pub fn x() -> i32 {\n    1\n}\n").unwrap();
        // rust_root 嵌在 source_root 下 → 应告警 LOC 可能重复计数。
        let (code, json) = run(&["stats", "loc", "--source", "src", "--rust", "src/rust"]);
        assert_eq!(code, 0, "stats loc 应成功: {json}");
        assert_eq!(json["status"], "warning");
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("包含关系")),
            "应含包含关系 warning: {json}"
        );
    });
}

#[test]
fn e2e_stats_compare_rejects_non_typescript_source() {
    // 问题1（M2-ADV-06 审查）：源侧解析强绑 TS。非 TS 项目须显式报错，
    // 而非源侧静默收集 0 文件、给出半残比值（functions/nesting 全 0）。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 将生成的 config 源语言改为非 TS（Python）。
        let cfg = std::fs::read_to_string(".rustmigrate.toml").unwrap();
        let cfg = cfg.replace(
            "source_language = \"typescript\"",
            "source_language = \"python\"",
        );
        std::fs::write(".rustmigrate.toml", &cfg).unwrap();
        assert!(
            cfg.contains("source_language = \"python\""),
            "前置：config 应已改为 python: {cfg}"
        );
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();

        let (code, json) = run(&["stats", "compare", "--source", "src", "--rust", "rust-src"]);
        assert_eq!(code, 1, "非 TS 源应报错: {json}");
        assert_eq!(json["status"], "error", "应为 error: {json}");
        let msg = json["data"]["message"].as_str().unwrap_or_default();
        assert!(
            msg.contains("仅支持 TypeScript"),
            "错误信息应说明仅支持 TS: {json}"
        );
    });
}

#[test]
fn smoke_stats_compare_structure() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        // 源码：2 个函数（f + 箭头常量 g）。
        std::fs::write(
            "src/a.ts",
            "export function f(x: number) {\n  if (x > 0) { return x; }\n}\nexport const g = () => {};\n",
        )
        .unwrap();
        // Rust：1 个函数。
        std::fs::write(
            "rust-src/a.rs",
            "pub fn f(x: i64) -> i64 {\n    if x > 0 {\n        return x;\n    }\n    0\n}\n",
        )
        .unwrap();

        let (code, json) = run(&["stats", "compare", "--source", "src", "--rust", "rust-src"]);
        assert_eq!(code, 0, "stats compare 应成功: {json}");
        // 两侧目录均存在、无 warning → status ok。
        assert_eq!(json["status"], "ok", "无 warning 应为 ok: {json}");
        assert_eq!(
            json["data"]["source"]["functions"], 2,
            "源码 2 个函数: {json}"
        );
        assert_eq!(
            json["data"]["rust"]["functions"], 1,
            "Rust 1 个函数: {json}"
        );
        assert_eq!(json["data"]["source"]["method"], "tree-sitter");
        assert_eq!(json["data"]["rust"]["method"], "lexical-scan");
        // 函数数比 rust/source = 0.5。
        assert_eq!(json["data"]["function_ratio"]["ratio"], 0.5);
        assert!(
            json["data"]["loc_ratio"]["source"].as_f64().unwrap() > 0.0,
            "源码 LOC 应 > 0: {json}"
        );
    });
}

#[test]
fn smoke_stats_compare_missing_rust_root() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        std::fs::create_dir_all("src").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        // rust-src 不存在 → data 为 null + warning，命令不失败。
        let (code, json) = run(&["stats", "compare"]);
        assert_eq!(code, 0, "缺 rust 目录不应失败: {json}");
        assert_eq!(json["status"], "warning");
        assert!(json["data"].is_null(), "data 应为 null: {json}");
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("跳过结构对比")),
            "应含跳过对比 warning: {json}"
        );
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

// === state populate-modules（PLAN.md §9.5 M1-PLAN-01：analyze→run 衔接）===

#[test]
fn e2e_populate_modules_linear_unblocks_run() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        // populate：拓扑序落成 modules + sprint。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "populate 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert!(
            json["data"]["total_sprints"].as_u64().unwrap() >= 1,
            "应至少有 1 个 sprint: {json}"
        );
        // decompose 默认启用：linear 3 文件同目录且均含逻辑 → 合成 1 个 coupled_batch 组（M3-DEC）。
        let count = json["data"]["module_count"].as_u64().unwrap();
        assert_eq!(
            count, 1,
            "linear-deps 3 文件应合成 1 个 coupled_batch 组: {json}"
        );

        let modules = json["data"]["modules"].as_array().unwrap();
        let group = &modules[0];
        assert_eq!(
            group["composite_kind"], "coupled_batch",
            "含逻辑成员的耦合簇应标 coupled_batch（不展开、不走机械轻量路径）: {group}"
        );
        let members: Vec<String> = group["member_files"]
            .as_array()
            .expect("coupled_batch 应有 member_files")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            members.len(),
            3,
            "组应含 index/service/utils 3 个成员: {members:?}"
        );
        // module key = 组内字典序最小成员的 NodeId 原值。
        let key = group["id"].as_str().unwrap().to_owned();
        assert!(
            key.starts_with("file:"),
            "组代表 key 应为 NodeId 原值: {key}"
        );

        // 落盘校验：state 合法 + 组模块 status=pending、sprint=1。
        let (code, json) = run(&["validate", "state"]);
        assert_eq!(code, 0, "填充后 state 应合法: {json}");
        let (code, json) = run(&["state", "get", &key]);
        assert_eq!(code, 0, "应能读取组模块: {json}");
        assert_eq!(json["data"]["status"], "pending");
        assert_eq!(json["data"]["state"]["sprint"], 1);

        // 衔接验证（codex #5）：run 阶段依赖门禁用组感知 state deps（对 coupled_batch 与 cycle/batch 一致）。
        // 组成员互引（index→service→utils）为组内自依赖应全部剔除；组无组外依赖 → all_ready。
        let (code, deps_json) = run(&["state", "deps", &key]);
        assert_eq!(code, 0, "state deps 应成功: {deps_json}");
        let deps: Vec<String> = deps_json["data"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["module"].as_str().unwrap().to_string())
            .collect();
        assert!(
            !deps.iter().any(|m| members.contains(m)),
            "组内自依赖应剔除: {deps:?}"
        );
        assert_eq!(
            deps_json["data"]["all_ready"], true,
            "组无组外未就绪依赖 → all_ready: {deps_json}"
        );

        // 非代表成员 key 归一：传非代表成员（file:utils.ts）应归一到组代表而非报「模块不存在」。
        let non_rep = members.iter().find(|m| *m != &key).unwrap();
        let (code, j) = run(&["state", "deps", non_rep]);
        assert_eq!(code, 0, "非代表成员 key 应归一而非报错: {j}");
    });
}

/// M3-DEC 回归：含逻辑成员的耦合簇必须保留为单个 coupled_batch 组，不被展开成独立单文件模块。
/// py-pkg-deps 是混合簇（barrel `__init__.py` + 纯类型 `types.py` + 逻辑 base/impl/main）——
/// 修复前 populate 因「成员非全机械」把整组展开为 5 个单文件模块（推翻 decompose 分组），
/// 修复后整组保留为 1 个 coupled_batch（走完整组路径）。这是 jmespath 场景的最小复现。
#[test]
fn e2e_populate_mixed_cluster_kept_as_coupled_batch() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("py-pkg-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "."]);
        assert_eq!(code, 0);

        let (code, json) = run(&["state", "populate-modules", "--root", "."]);
        assert_eq!(code, 0, "populate 应成功: {json}");
        assert_eq!(
            json["data"]["module_count"], 1,
            "混合耦合簇应保留为 1 个组（而非展开为 5 个单文件模块）: {json}"
        );
        let group = &json["data"]["modules"].as_array().unwrap()[0];
        assert_eq!(
            group["composite_kind"], "coupled_batch",
            "含逻辑成员的混合簇应标 coupled_batch（不是 batch、不展开）: {group}"
        );
        let members = group["member_files"].as_array().unwrap();
        assert_eq!(
            members.len(),
            5,
            "组应含全部 5 个成员（main + pkg/__init__/base/impl/types）: {group}"
        );
        let (code, json) = run(&["validate", "state"]);
        assert_eq!(code, 0, "落盘后 state 应合法: {json}");
    });
}

#[test]
fn e2e_populate_cleans_orphan_pending() {
    // 源码图删文件后重填：上一轮登记、本轮序列已不含的 pending 模块应被清理为孤儿，
    // 并经 warning 告知用户，避免不存在的模块被计入进度。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        // 孤儿清理测试前提是「多个独立单文件模块」——用 --no-decompose 走 SCC-only 旧路径
        // 保持 1 文件 1 模块（decompose 默认会把同目录文件合成 1 组，删单成员只是组缩小、不产孤儿）。
        // 同时为 --no-decompose 旧路径提供回归覆盖（计划 6d）。
        // 首轮填充：linear-deps 3 个模块（index/service/utils），全部 pending。
        let (code, json) = run(&["state", "populate-modules", "--no-decompose"]);
        assert_eq!(code, 0, "首轮填充应成功: {json}");
        assert_eq!(json["data"]["module_count"], 3);

        // 删 index.ts（根模块，无其他文件 import 它，不破坏剩余依赖）后重建源码图。
        std::fs::remove_file("src/index.ts").unwrap();
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        // 重填：序列缩为 2，原 index 模块成孤儿应被清理 + warning 告知。
        let (code, json) = run(&["state", "populate-modules", "--no-decompose"]);
        assert_eq!(code, 0, "重填应成功: {json}");
        assert_eq!(
            json["data"]["module_count"], 2,
            "重填后应只剩 2 个模块: {json}"
        );
        assert_eq!(json["status"], "warning", "清理孤儿应降级 warning: {json}");
        let warnings = json["warnings"].as_array().expect("应有 warnings 数组");
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap_or_default().contains("孤儿")),
            "应有孤儿清理 warning: {json}"
        );

        // 落盘校验：modules 中不再含 index，且 state 合法。
        let modules = json["data"]["modules"].as_array().unwrap();
        assert!(
            !modules
                .iter()
                .any(|m| m["id"].as_str().unwrap_or_default().contains("index")),
            "重填后不应再含 index 模块: {json}"
        );
        let (code, json) = run(&["validate", "state"]);
        assert_eq!(code, 0, "清理后 state 应合法: {json}");
    });
}

#[test]
fn smoke_populate_folds_cycles() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        // 破环（M2-SCALE-SCC）：循环依赖不再拒绝，整组折叠为 composite 模块组。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "有环应折叠而非拒绝: {json}");
        assert_eq!(json["status"], "warning");
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap().contains("折叠")),
            "应有折叠 warning: {json}"
        );
        // 应存在一个 composite 模块组，member_files 含 event-bus/handler/emitter。
        let modules = json["data"]["modules"].as_array().unwrap();
        let composite = modules
            .iter()
            .find(|m| m.get("member_files").is_some())
            .expect("应有一个 composite 模块组");
        let members: Vec<&str> = composite["member_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            members.iter().any(|s| s.contains("event-bus"))
                && members.iter().any(|s| s.contains("handler"))
                && members.iter().any(|s| s.contains("emitter")),
            "composite 组应含环成员: {members:?}"
        );
        // M3-DEC-01：循环 composite 应显式标记 composite_kind=cycle（区别于机械合批组）。
        assert_eq!(
            composite["composite_kind"], "cycle",
            "循环 composite 应标记 composite_kind=cycle: {composite}"
        );
        // 折叠后 state 应合法。
        let (code, json) = run(&["validate", "state"]);
        assert_eq!(code, 0, "折叠后 state 应合法: {json}");
    });
}

#[test]
fn smoke_state_deps_group_aware() {
    // 破环门禁衔接（M2-SCALE-SCC）：composite 组代表的组感知依赖应聚合组所有成员的
    // 对外依赖、映射回组代表、剔除组内自依赖。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        let (code, _) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0);

        // 组 {emitter,event-bus,handler} 折叠，代表 emitter；三者都 import shared。
        let (code, json) = run(&["state", "deps", "file:emitter.ts"]);
        assert_eq!(code, 0, "state deps 应成功: {json}");
        let deps: Vec<String> = json["data"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["module"].as_str().unwrap().to_string())
            .collect();
        // 聚合组外依赖 shared
        assert!(
            deps.iter().any(|m| m.contains("shared")),
            "应聚合组外依赖 shared: {deps:?}"
        );
        // 组内自依赖剔除（emitter↔event-bus↔handler 互引不算依赖）
        assert!(
            !deps
                .iter()
                .any(|m| m.contains("event-bus") || m.contains("handler")),
            "组内自依赖应剔除: {deps:?}"
        );
        // shared 仍 pending → 未就绪，列入 blocking
        assert_eq!(json["data"]["all_ready"], false);
        assert!(
            json["data"]["blocking"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b.as_str().unwrap().contains("shared")),
            "shared(pending) 应在 blocking: {json}"
        );

        // 非代表成员 key 归一（主审 #1）：传 file:handler.ts（非代表成员）应归一到组代表
        // emitter 并返回等价结果，而非报「模块不存在」。
        let (code, json2) = run(&["state", "deps", "file:handler.ts"]);
        assert_eq!(code, 0, "非代表成员 key 应归一而非报错: {json2}");
        let deps2: Vec<String> = json2["data"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["module"].as_str().unwrap().to_string())
            .collect();
        assert!(
            deps2.iter().any(|m| m.contains("shared")),
            "归一后应聚合组外依赖 shared: {json2}"
        );
        assert!(
            !deps2
                .iter()
                .any(|m| m.contains("handler") || m.contains("event-bus")),
            "归一后组内自依赖应剔除: {deps2:?}"
        );
    });
}

#[test]
fn smoke_state_deps_unresolved_not_blocking() {
    // absent 死锁修复（主审）：依赖未登记为模块（state 与 graph 不同步）时进 unresolved + warning，
    // **不进 blocking**——否则会被填入 blocked_by，而 check-blocked 对缺失 key 永判非终态导致死锁。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        let (code, _) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0);

        // 制造 state/graph 不同步：从 state 删除 shared 模块（它仍在 graph、被 emitter 组依赖）。
        let sp = tmp.path().join(".rust-migration/migration-state.json");
        let mut sf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sp).unwrap()).unwrap();
        sf["modules"]
            .as_object_mut()
            .unwrap()
            .remove("file:shared.ts");
        std::fs::write(&sp, serde_json::to_string_pretty(&sf).unwrap()).unwrap();

        // emitter 组依赖 shared（现已未登记）→ 应进 unresolved + warning，不进 blocking。
        let (code, json) = run(&["state", "deps", "file:emitter.ts"]);
        assert_eq!(code, 0, "absent 依赖应降级 warning 而非命令失败: {json}");
        assert_eq!(json["status"], "warning");
        let unresolved: Vec<String> = json["data"]["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        assert!(
            unresolved.iter().any(|m| m.contains("shared")),
            "shared 应在 unresolved: {json}"
        );
        // 关键：absent 依赖不进 blocking（避免 blocked_by 死锁）。
        assert!(
            !json["data"]["blocking"]
                .as_array()
                .unwrap()
                .iter()
                .any(|b| b.as_str().unwrap().contains("shared")),
            "absent 依赖不应进 blocking: {json}"
        );
    });
}

#[test]
fn smoke_populate_rejects_active_progress() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        // 用 --no-decompose 保 1 文件 1 模块：topo-sort order[0] 是文件 NodeId，需与 modules key 一致；
        // decompose 默认合组后非代表成员不在 modules 表，transition 会失配。本测试验证「活跃模块拒绝重填」，
        // 与分组无关，旧路径即可。
        let (code, _) = run(&["state", "populate-modules", "--no-decompose"]);
        assert_eq!(code, 0);

        // 把某模块推进到 translating（模拟迁移进行中）。
        let utils = {
            let (_, json) = run(&["graph", "topo-sort"]);
            json["data"]["order"][0].as_str().unwrap().to_owned()
        };
        let (code, _) = run(&[
            "state",
            "transition",
            "--module",
            &utils,
            "--to",
            "translating",
        ]);
        assert_eq!(code, 0);

        // 再次 populate：拒绝重填以免重置进度（断点续传安全）。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 1, "存在活跃模块应拒绝重填: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_populate_idempotent_when_all_pending() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        let (code, json1) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0);
        let count1 = json1["data"]["module_count"].as_u64().unwrap();

        // 全部仍为 pending → 再次 populate 应成功且结果稳定。
        let (code, json2) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "全 pending 重填应成功: {json2}");
        assert_eq!(
            json2["data"]["module_count"].as_u64().unwrap(),
            count1,
            "重填后 module_count 应不变"
        );
    });
}

#[test]
fn smoke_populate_empty_graph_warns() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 构建一个无源文件的空图。
        std::fs::create_dir_all("empty-src").unwrap();
        let (code, _) = run(&["graph", "build", "--root", "empty-src"]);
        assert_eq!(code, 0);

        // populate 空图 → warning(status 降级) + module_count=0。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "空图 populate 不应 hard error: {json}");
        assert_eq!(json["status"], "warning", "空图应降级为 warning: {json}");
        assert_eq!(json["data"]["module_count"], 0);
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("无文件模块")),
            "应含'无文件模块'提示: {json}"
        );
    });
}

#[test]
fn smoke_populate_without_db_errors() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 1, "无 db 时应报错");
        assert_eq!(json["status"], "error");
    });
}

// === graph cycles（M2-CLI-02：完整 SCC 环检测）===

#[test]
fn smoke_graph_cycles_no_cycles() {
    // linear-deps 无环：has_cycles=false, cycles=[]。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        build_graph_for_query();
        let (code, json) = run(&["graph", "cycles"]);
        assert_eq!(code, 0, "graph cycles 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["has_cycles"], false);
        assert_eq!(json["data"]["cycle_count"], 0);
        assert!(
            json["data"]["cycles"].as_array().unwrap().is_empty(),
            "无环时 cycles 应为空数组: {json}"
        );
    });
}

#[test]
fn smoke_graph_cycles_with_cycles() {
    // circular-deps 有环：has_cycles=true, cycles 非空，含 event-bus/handler/emitter。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        build_graph_for_query();
        let (code, json) = run(&["graph", "cycles"]);
        assert_eq!(code, 0, "graph cycles 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["has_cycles"], true);
        let cycle_count = json["data"]["cycle_count"].as_u64().unwrap();
        assert!(cycle_count >= 1, "应至少检测到 1 个环: {json}");
        let cycles = json["data"]["cycles"].as_array().unwrap();
        assert!(!cycles.is_empty(), "cycles 应非空: {json}");

        // 至少有一个环包含 event-bus、handler、emitter。
        let has_expected = cycles.iter().any(|cycle| {
            let members: Vec<&str> = cycle
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap_or_default())
                .collect();
            let has_eb = members.iter().any(|s| s.contains("event-bus"));
            let has_h = members.iter().any(|s| s.contains("handler"));
            let has_e = members.iter().any(|s| s.contains("emitter"));
            has_eb && has_h && has_e
        });
        assert!(
            has_expected,
            "应包含 event-bus/handler/emitter 的环: {cycles:?}"
        );
    });
}

#[test]
fn smoke_graph_cycles_without_db_errors() {
    // 无 db 时应报错（非 panic）。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["graph", "cycles"]);
        assert_eq!(code, 1, "无 db 时应返回错误");
        assert_eq!(json["status"], "error");
    });
}

// === graph interfaces --deps-of 批量模式 ===

#[test]
fn smoke_graph_interfaces_deps_of() {
    // linear-deps: index.ts -> service.ts -> utils.ts
    // index.ts 的 imports 1-hop 邻居应包含 service.ts 和 utils.ts。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "interfaces", "dummy", "--deps-of", "index.ts"]);
        assert_eq!(code, 0, "graph interfaces --deps-of 应成功: {json}");
        assert_eq!(json["status"], "ok");

        // data.target 应为 index.ts 的完整路径。
        let target = json["data"]["target"].as_str().unwrap();
        assert!(
            target.contains("index.ts"),
            "target 应包含 index.ts: {target}"
        );

        // data.dependencies 应为数组且非空。
        let deps = json["data"]["dependencies"].as_array().unwrap();
        assert!(!deps.is_empty(), "dependencies 应非空: {json}");

        // 每个依赖应有 module 和 exports 字段。
        for dep in deps {
            assert!(dep["module"].is_string(), "每个依赖应有 module 字段");
            assert!(dep["exports"].is_array(), "每个依赖应有 exports 数组");
        }

        // 依赖模块名应包含 service.ts 和 utils.ts。
        let dep_modules: Vec<&str> = deps.iter().map(|d| d["module"].as_str().unwrap()).collect();
        assert!(
            dep_modules.iter().any(|m| m.contains("service.ts")),
            "依赖应包含 service.ts: {dep_modules:?}"
        );
        assert!(
            dep_modules.iter().any(|m| m.contains("utils.ts")),
            "依赖应包含 utils.ts: {dep_modules:?}"
        );
    });
}

#[test]
fn smoke_graph_interfaces_deps_of_exports_content() {
    // service.ts 的 1-hop 依赖只有 utils.ts，其导出应含 clamp / fetchData / Range / Predicate。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "interfaces", "dummy", "--deps-of", "service.ts"]);
        assert_eq!(
            code, 0,
            "graph interfaces --deps-of service.ts 应成功: {json}"
        );
        assert_eq!(json["status"], "ok");

        let deps = json["data"]["dependencies"].as_array().unwrap();
        // service.ts 只依赖 utils.ts。
        assert_eq!(deps.len(), 1, "service.ts 应只有 1 个依赖: {deps:?}");

        let utils_dep = &deps[0];
        assert!(
            utils_dep["module"].as_str().unwrap().contains("utils.ts"),
            "依赖应为 utils.ts"
        );

        // utils.ts 的导出接口应含 clamp 函数。
        let exports = utils_dep["exports"].as_array().unwrap();
        assert!(!exports.is_empty(), "utils.ts 应有导出接口");
        let export_names: Vec<&str> = exports
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(
            export_names.contains(&"clamp"),
            "utils.ts 导出应包含 clamp: {export_names:?}"
        );

        // 每条导出接口应含 token_estimate。
        for exp in exports {
            assert!(
                exp["token_estimate"].is_number(),
                "导出接口应含 token_estimate"
            );
        }

        // 按符号裁剪（M3-DEC-01）：service.ts 仅 import { clamp, Range, fetchData }，
        // 未被使用的 Predicate 应从 deps-of 输出裁掉。
        assert!(
            !export_names.contains(&"Predicate"),
            "未被 import 的 Predicate 应被裁剪: {export_names:?}"
        );
        let used: Vec<&str> = utils_dep["used_symbols"]
            .as_array()
            .expect("纯具名依赖应有 used_symbols 数组")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            used,
            vec!["Range", "clamp", "fetchData"],
            "used_symbols 应为被用具名符号（排序）: {used:?}"
        );
        // footprint 的依赖签名规模应为正数。
        assert!(
            json["data"]["dependency_signature_tokens"]
                .as_u64()
                .is_some_and(|t| t > 0),
            "应输出正的 dependency_signature_tokens: {json}"
        );
    });
}

// === graph decompose 拆解 dry-run（M3-DEC-01 §8 验收报告）===

#[test]
fn smoke_graph_decompose_dry_run() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        // 用默认 budget（12000，MDR-011）跑，覆盖新默认路径。
        let (code, json) = run(&["graph", "decompose", "--root", "src"]);
        assert_eq!(code, 0, "decompose 应成功: {json}");

        let data = &json["data"];
        // 四维度齐全。
        assert!(data["target"].is_object(), "应有 target 维度");
        assert!(data["invariants"].is_object(), "应有 invariants 维度");
        assert!(data["cohesion"].is_object(), "应有 cohesion 维度");
        assert!(
            data["classification"].is_object(),
            "应有 classification 维度"
        );
        // MDR-011 字段：residual_single_file（替代旧 residual_mechanical_single）+ cohesion.coupling_edges/pass。
        assert!(
            data["target"]["residual_single_file"].is_u64(),
            "应有 residual_single_file 字段"
        );
        assert!(
            data["cohesion"]["coupling_edges"].is_u64(),
            "应有 cohesion.coupling_edges 字段"
        );
        assert!(
            data["cohesion"]["pass"].is_boolean(),
            "应有 cohesion.pass 判定"
        );

        // 硬不变量：每文件恰好一个单元 + 单元图无环。
        assert_eq!(data["invariants"]["partition_ok"], true);
        assert_eq!(data["invariants"]["dag_acyclic"], true);

        // 单元覆盖全部文件（modules_before == 文件数；linear-deps 有 3 文件）。
        let before = data["target"]["modules_before"].as_u64().unwrap();
        assert_eq!(before, 3, "linear-deps 应有 3 个源文件");
        let units = data["units"].as_array().unwrap();
        let total_members: usize = units
            .iter()
            .map(|u| u["members"].as_array().unwrap().len())
            .sum();
        assert_eq!(total_members as u64, before, "单元成员应恰好覆盖全部文件");

        // 确定性：plan_hash 两次一致（§8「跑两次字节级一致」）。
        let hash1 = data["invariants"]["plan_hash"]
            .as_str()
            .unwrap()
            .to_string();
        let (_, json2) = run(&["graph", "decompose", "--root", "src"]);
        assert_eq!(
            json2["data"]["invariants"]["plan_hash"].as_str().unwrap(),
            hash1,
            "拆解计划应确定可复现"
        );
    });
}

#[test]
fn smoke_graph_interfaces_deps_of_nonexistent_target_errors() {
    // --deps-of 的 target 不存在时应报错。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&[
            "graph",
            "interfaces",
            "dummy",
            "--deps-of",
            "does-not-exist.ts",
        ]);
        assert_eq!(code, 1, "不存在的 target 应报错");
        assert_eq!(json["status"], "error");
    });
}

// === graph interfaces --members 整组模式（Phase 2 契约 agent 输入）===

#[test]
fn smoke_graph_interfaces_members_whole_scc_group() {
    // circular-deps: {emitter, event-bus, handler} 三向引用环折叠为一个 SCC 组。
    // --members 以任一成员定位整组，一次输出全组导出签名（含 signature_text + token 合计）。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "interfaces", "emitter.ts", "--members"]);
        assert_eq!(code, 0, "graph interfaces --members 应成功: {json}");
        assert_eq!(json["status"], "ok");

        let data = &json["data"];
        assert_eq!(data["is_cycle"], true, "三向环应为 cycle: {data}");
        assert_eq!(data["member_count"], 3, "环应含 3 成员: {data}");

        // 全组成员模块名应覆盖三文件。
        let members = data["members"].as_array().unwrap();
        let modules: Vec<&str> = members
            .iter()
            .map(|m| m["module"].as_str().unwrap())
            .collect();
        for f in ["emitter.ts", "event-bus.ts", "handler.ts"] {
            assert!(
                modules.iter().any(|m| m.contains(f)),
                "成员应含 {f}: {modules:?}"
            );
        }

        // signature 来自图节点（build 时 AST 提取、持久化），query 不回读源文件。
        // Emitter 类签名剥离函数体为 `class Emitter`（node 为 class_declaration，不含 export 关键字）。
        let emitter = members
            .iter()
            .find(|m| m["module"].as_str().unwrap().contains("emitter.ts"))
            .unwrap();
        let exports = emitter["exports"].as_array().unwrap();
        let cls = exports.iter().find(|e| e["name"] == "Emitter").unwrap();
        assert_eq!(
            cls["signature"].as_str().unwrap(),
            "class Emitter",
            "class 签名应剥离函数体: {cls}"
        );

        // 整组签名 token 合计为正。
        let sig = data["total_signature_tokens"].as_u64().unwrap();
        assert!(sig > 0, "签名 token 合计应为正: {data}");
        // 签名来自图、与查询 cwd 无关，无 missing_source 之说，status 不降级。
        assert!(json["warnings"].is_null(), "不应有告警: {json}");
    });
}

// === graph export 测试 ===

#[test]
fn smoke_graph_export_json() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "export", "--format", "json"]);
        assert_eq!(code, 0, "graph export --format json 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["format"], "json");

        // json 格式时 content 是对象（含 nodes 和 edges），不是字符串
        let content = &json["data"]["content"];
        assert!(
            content.is_object(),
            "json 格式的 content 应为对象: {content}"
        );
        assert!(content["nodes"].is_array(), "content 应含 nodes 数组");
        assert!(content["edges"].is_array(), "content 应含 edges 数组");
        assert!(
            content["nodes"].as_array().unwrap().len() >= 3,
            "linear-deps 至少 3 个节点"
        );
    });
}

#[test]
fn smoke_graph_export_json_default_format() {
    // 不指定 --format 时默认 json
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "export"]);
        assert_eq!(code, 0, "graph export（默认格式）应成功: {json}");
        assert_eq!(json["data"]["format"], "json");
        assert!(json["data"]["content"].is_object());
    });
}

#[test]
fn smoke_graph_export_dot() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "export", "--format", "dot"]);
        assert_eq!(code, 0, "graph export --format dot 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["format"], "dot");

        let content = json["data"]["content"].as_str().unwrap();
        assert!(
            content.starts_with("digraph {"),
            "DOT 应以 digraph {{ 开头: {content}"
        );
        assert!(content.contains("->"), "DOT 应含 -> 边声明");
        assert!(content.contains("[label="), "DOT 应含 label 属性");
    });
}

#[test]
fn smoke_graph_export_mermaid() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "export", "--format", "mermaid"]);
        assert_eq!(code, 0, "graph export --format mermaid 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["format"], "mermaid");

        let content = json["data"]["content"].as_str().unwrap();
        assert!(
            content.starts_with("flowchart TD"),
            "Mermaid 应以 flowchart TD 开头: {content}"
        );
        assert!(content.contains("-->|"), "Mermaid 应含 -->| 边声明");
        // Mermaid ID 不应含冒号
        let lines: Vec<&str> = content.lines().collect();
        for line in &lines[1..] {
            // 节点声明行和边行的 ID 部分不应含冒号（标签引号内可以）
            if let Some(id_part) = line.trim().split('[').next() {
                if !id_part.contains("-->") {
                    assert!(!id_part.contains(':'), "Mermaid 节点 ID 不应含冒号: {line}");
                }
            }
        }
    });
}

#[test]
fn smoke_graph_export_invalid_format() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        build_graph_for_query();
        let (code, json) = run(&["graph", "export", "--format", "xml"]);
        assert_eq!(code, 1, "不支持的格式应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

// === validate config 测试 ===

#[test]
fn smoke_validate_config_no_file() {
    // 无配置文件时：status=warning（有 warning 降级），valid=true。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["validate", "config"]);
        assert_eq!(code, 0, "无配置文件应成功: {json}");
        // 有 warning 时 Response::ok_with_warnings 会降级 status 为 warning。
        assert_eq!(json["status"], "warning");
        assert_eq!(json["data"]["valid"], true);
        assert_eq!(json["data"]["fields_checked"], 0);
        // 应有 warning 提示未找到配置文件。
        let warnings = json["warnings"].as_array().expect("应有 warnings 字段");
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap().contains("未找到配置文件")),
            "应有'未找到配置文件'警告: {json}"
        );
    });
}

#[test]
fn smoke_validate_config_valid() {
    // 合法配置文件、所有路径存在：status=ok，valid=true。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        // 先 init 生成合法配置文件，再创建 source_root 和 rust_root 目录。
        let _ = run(&["init"]);
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        let (code, json) = run(&["validate", "config"]);
        assert_eq!(code, 0, "合法配置应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["valid"], true);
        assert_eq!(json["data"]["fields_checked"], 5);
        assert_eq!(json["data"]["config_path"], ".rustmigrate.toml");
    });
}

#[test]
fn smoke_validate_config_invalid_toml() {
    // 非法 TOML 语法：应报错。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        std::fs::write(".rustmigrate.toml", "这不是合法的 [[[toml").unwrap();
        let (code, json) = run(&["validate", "config"]);
        assert_eq!(code, 1, "非法 TOML 应报错: {json}");
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["kind"], "config");
    });
}

#[test]
fn smoke_validate_config_warnings() {
    // 字段值不合理时产出 warnings。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let config = r#"
[project]
name = "test"
source_root = "nonexistent_src"
rust_root = "nonexistent_rust"

[strategy]
max_retry_rounds = 0

[testing]
coverage_threshold = 200

[orchestration]
subagent_timeout_secs = 0
"#;
        std::fs::write(".rustmigrate.toml", config).unwrap();
        let (code, json) = run(&["validate", "config"]);
        assert_eq!(
            code, 0,
            "配置校验应成功（问题走 warnings 不走 error）: {json}"
        );
        // 有不合理字段时 valid=false。
        assert_eq!(json["data"]["valid"], false);
        // status 应降级为 warning。
        assert_eq!(
            json["status"], "warning",
            "有 warnings 时 status 应降级: {json}"
        );
        let warnings = json["warnings"].as_array().expect("应有 warnings 字段");
        // 至少应有：source_root 不存在、rust_root 不存在、max_retry_rounds=0、
        // coverage_threshold>100、subagent_timeout_secs=0
        assert!(warnings.len() >= 4, "应有至少 4 条 warnings: {json}");
    });
}
