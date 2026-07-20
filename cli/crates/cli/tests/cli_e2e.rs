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

#[test]
fn e2e_topo_sort_reverse() {
    // --reverse 是纯排序变体（不破环）：默认叶子优先，--reverse 整体反转为依赖优先。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        // 默认：叶子优先（utils < index）。
        let (code, json) = run(&["graph", "topo-sort"]);
        assert_eq!(code, 0, "topo-sort 应成功: {json}");
        let order = json["data"]["order"].as_array().expect("order 应为数组");
        let fwd_utils = find_position(order, "utils.ts").expect("应含 utils.ts");
        let fwd_index = find_position(order, "index.ts").expect("应含 index.ts");
        assert!(fwd_utils < fwd_index, "默认应叶子优先: {order:?}");

        // --reverse：order 整体反转——index 现在排在 utils 前。
        let (code, json) = run(&["graph", "topo-sort", "--reverse"]);
        assert_eq!(code, 0, "reverse 模式应成功: {json}");
        let order = json["data"]["order"].as_array().expect("order 应为数组");
        let pos_utils = find_position(order, "utils.ts").expect("应含 utils.ts");
        let pos_index = find_position(order, "index.ts").expect("应含 index.ts");
        assert!(
            pos_index < pos_utils,
            "reverse 后 index 应排在 utils 前: {order:?}"
        );
    });
}

// === graph parallel-groups：并行层输出（ORCH-01）===

#[test]
fn e2e_parallel_groups_diamond_has_multi_group_layer() {
    // diamond-deps：barrel.ts 与 index.ts 拓扑独立，应落在同一 sprint 层（可并行）。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("diamond-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        let (code, json) = run(&["graph", "parallel-groups"]);
        assert_eq!(code, 0, "parallel-groups 应成功: {json}");
        assert_eq!(json["status"], "ok");

        let layers = json["data"]["layers"].as_array().expect("应含 layers");
        // sprint 1 应为叶节点层，且含叶节点 types.ts。
        let first = &layers[0];
        assert_eq!(first["sprint"], 1, "首层 sprint 应为 1: {json}");
        let sprint1_has_leaf = first["groups"]
            .as_array()
            .map(|gs| {
                gs.iter().any(|g| {
                    g["members"]
                        .as_array()
                        .map(|m| {
                            m.iter()
                                .any(|x| x.as_str().unwrap_or("").ends_with("types.ts"))
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        assert!(sprint1_has_leaf, "sprint 1 应含叶节点 types.ts: {json}");

        // 至少一层含 >1 个组（真并行层）——证明不是纯线性退化。
        let has_parallel_layer = layers
            .iter()
            .any(|l| l["groups"].as_array().map(|g| g.len() > 1).unwrap_or(false));
        assert!(has_parallel_layer, "应存在含多个组的并行层: {json}");
    });
}

#[test]
fn e2e_parallel_groups_circular_folds_and_returns_zero() {
    // 与 topo-sort 相反：有环时 parallel-groups 不报错（环已折叠为 SCC 组）。
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("circular-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        let (code, json) = run(&["graph", "parallel-groups"]);
        assert_eq!(code, 0, "有环时 parallel-groups 仍应零退出: {json}");
        assert_eq!(json["status"], "ok");

        // 环成员折叠成一个 is_cycle=true 的组。
        let layers = json["data"]["layers"].as_array().expect("应含 layers");
        let has_cycle_group = layers.iter().any(|l| {
            l["groups"]
                .as_array()
                .map(|gs| {
                    gs.iter().any(|g| {
                        g["is_cycle"] == true
                            && g["members"]
                                .as_array()
                                .map(|m| m.len() > 1)
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });
        assert!(has_cycle_group, "环应折叠为多成员 is_cycle 组: {json}");
    });
}

#[test]
fn e2e_parallel_groups_empty_graph_zero_layers() {
    // 空图（无源文件）：layer_count/group_count 均为 0，layers 为空，仍零退出。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        // 空目录 build 出空图（status 可能 warning：source_root 回退）。
        let (code, _) = run(&["graph", "build", "--root", "."]);
        assert_eq!(code, 0, "空目录 graph build 应零退出");

        let (code, json) = run(&["graph", "parallel-groups"]);
        assert_eq!(code, 0, "空图 parallel-groups 应零退出: {json}");
        assert_eq!(json["data"]["layer_count"], 0, "空图应无层: {json}");
        assert_eq!(json["data"]["group_count"], 0, "空图应无组: {json}");
        assert_eq!(
            json["data"]["layers"].as_array().map(|a| a.len()),
            Some(0),
            "空图 layers 应为空数组: {json}"
        );
    });
}

// === 各命令 smoke：合法 JSON + status 正确 ===

#[test]
fn smoke_init() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0);
        // 空目录无源文件时 source_root 回退 "." 产生 warning
        assert!(
            json["status"] == "ok" || json["status"] == "warning",
            "init status 应为 ok 或 warning: {json}"
        );
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
fn init_source_root_python_flat_package() {
    let tmp = tempfile::tempdir().unwrap();
    // 模拟 Python flat-package 布局（如 jmespath）
    let pkg = tmp.path().join("mypackage");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(pkg.join("__init__.py"), "").unwrap();
    std::fs::write(pkg.join("core.py"), "x = 1\ny = 2\n").unwrap();
    std::fs::write(
        tmp.path().join("setup.py"),
        "from setuptools import setup\n",
    )
    .unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0, "init 应成功: {json}");
        let config: String = std::fs::read_to_string(tmp.path().join(".rustmigrate.toml")).unwrap();
        assert!(
            config.contains("source_root = \"mypackage\""),
            "flat-package Python 项目的 source_root 应为包名，实际配置:\n{config}"
        );
    });
}

#[test]
fn init_source_root_ts_with_src() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/index.ts"), "export const x = 1;\n").unwrap();
    with_cwd(tmp.path(), || {
        let (code, _) = run(&["init"]);
        assert_eq!(code, 0);
        let config: String = std::fs::read_to_string(tmp.path().join(".rustmigrate.toml")).unwrap();
        assert!(
            config.contains("source_root = \"src\""),
            "有 src/ 的 TS 项目 source_root 应为 src，实际配置:\n{config}"
        );
    });
}

#[test]
fn init_source_root_fallback_warning() {
    let tmp = tempfile::tempdir().unwrap();
    // 空目录，无源文件 → 回退 "." 并产生 warning
    std::fs::write(tmp.path().join("README.md"), "# hello").unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&["init"]);
        assert_eq!(code, 0, "init 应成功: {json}");
        let config: String = std::fs::read_to_string(tmp.path().join(".rustmigrate.toml")).unwrap();
        assert!(
            config.contains("source_root = \".\""),
            "无源文件时 source_root 应回退为 \".\"，实际配置:\n{config}"
        );
        // status 应降级为 warning
        assert_eq!(json["status"], "warning", "回退应产生 warning");
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

/// Go 源（go-linear-deps）经 `detect_language` 路由到 Go adapter：强制全量并输出降级
/// 警告（status=warning），图非空。覆盖 CLI 层 Go 语言 dispatch——否则 GoAdapter 路由
/// 若回归（如误落 TS）会静默产出空/错图而无测试报警。
#[test]
fn smoke_graph_build_go_detects_and_degrades() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("go-linear-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        // 不 init：走纯 detect_language 探测路径（go.mod + .go → SourceLang::Go）。
        let (code, json) = run(&["graph", "build", "--root", "."]);
        assert_eq!(code, 0, "Go graph build 应成功: {json}");
        assert_eq!(json["status"], "warning", "降级应使 status=warning: {json}");
        assert_eq!(json["data"]["full"], true, "Go 应强制全量: {json}");
        // 非空且与库层一致（node=11/edge=13），证明确实路由到 GoAdapter 而非空/错图。
        assert_eq!(
            json["data"]["node_count"].as_u64().unwrap_or(0),
            11,
            "Go 图节点数应为 11（GoAdapter 正确路由）: {json}"
        );
        assert_eq!(
            json["data"]["edge_count"].as_u64().unwrap_or(0),
            13,
            "Go 图边数应为 13: {json}"
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

/// 注入一个带 substatus 的模块（两层 done 测试辅助）。
fn inject_module_with_substatus(name: &str, status: &str, substatus: &str) {
    let path = std::path::Path::new(".rust-migration").join("migration-state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    state["modules"][name] = serde_json::json!({ "status": status, "substatus": substatus });
    std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();
}

#[test]
fn e2e_batch_transition_done_all_success() {
    // 两层 done 第二层门：整组 reviewing+agent_done 模块批量升 done。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module_with_substatus("a", "reviewing", "agent_done");
        inject_module_with_substatus("b", "reviewing", "agent_done");

        let (code, json) = run(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "b",
        ]);
        assert_eq!(code, 0, "全部 agent_done 应成功: {json}");
        assert_eq!(json["status"], "ok");
        let succeeded = json["data"]["succeeded"].as_array().unwrap();
        assert_eq!(succeeded.len(), 2, "两个模块都应升 done: {json}");
        assert!(json["data"]["skipped"].as_array().unwrap().is_empty());

        // 落盘校验：均为终态 done。
        let (_, ja) = run(&["state", "get", "a"]);
        assert_eq!(ja["data"]["status"], "done");
        let (_, jb) = run(&["state", "get", "b"]);
        assert_eq!(jb["data"]["status"], "done");
    });
}

#[test]
fn e2e_batch_transition_done_skips_non_agent_done_and_warns() {
    // 非 agent_done 模块被跳过，status 降级 warning，成功的仍升 done。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module_with_substatus("a", "reviewing", "agent_done");
        // b：reviewing 但 substatus 非 agent_done（未过 agent 自检），应被跳过。
        inject_module_with_substatus("b", "reviewing", "other");

        let (code, json) = run(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "b",
        ]);
        assert_eq!(code, 0, "部分成功仍退出 0: {json}");
        assert_eq!(
            json["status"], "warning",
            "skipped 非空应降级 warning: {json}"
        );
        assert_eq!(json["data"]["succeeded"].as_array().unwrap(), &vec!["a"]);
        assert_eq!(json["data"]["skipped"].as_array().unwrap(), &vec!["b"]);
        assert!(!json["warnings"].as_array().unwrap().is_empty());

        let (_, ja) = run(&["state", "get", "a"]);
        assert_eq!(ja["data"]["status"], "done", "a 应升 done");
        let (_, jb) = run(&["state", "get", "b"]);
        assert_eq!(jb["data"]["status"], "reviewing", "b 应保持 reviewing");
    });
}

#[test]
fn e2e_batch_transition_done_skips_wrong_status_and_missing() {
    // 第二条 skip 路径：status 非 reviewing（agent_done 但矩阵拒绝）+ 模块不存在。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module_with_substatus("a", "reviewing", "agent_done");
        // b：translating + agent_done → translating→done 被矩阵拒绝，应跳过。
        inject_module_with_substatus("b", "translating", "agent_done");

        let (code, json) = run(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "b",
            "--module",
            "ghost", // 不存在的模块。
        ]);
        assert_eq!(code, 0);
        assert_eq!(json["status"], "warning");
        // requested 回显（去重后原样）。
        assert_eq!(
            json["data"]["requested"].as_array().unwrap(),
            &vec!["a", "b", "ghost"]
        );
        assert_eq!(json["data"]["succeeded"].as_array().unwrap(), &vec!["a"]);
        let skipped = json["data"]["skipped"].as_array().unwrap();
        assert!(
            skipped.contains(&serde_json::json!("b"))
                && skipped.contains(&serde_json::json!("ghost")),
            "b（矩阵拒绝）和 ghost（不存在）都应跳过: {json}"
        );
        assert_eq!(json["data"]["duplicates"], 0);
    });
}

#[test]
fn e2e_batch_transition_done_dedups_repeated_module() {
    // 重复 --module 去重：不把去重项误算成功，不给已 done 模块追加噪声审计。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        inject_module_with_substatus("a", "reviewing", "agent_done");

        let (code, json) = run(&[
            "state",
            "batch-transition-done",
            "--module",
            "a",
            "--module",
            "a",
        ]);
        assert_eq!(code, 0);
        // 去重后 requested 只剩一个 a，succeeded 一个，无 skipped。
        assert_eq!(json["data"]["requested"].as_array().unwrap(), &vec!["a"]);
        assert_eq!(json["data"]["succeeded"].as_array().unwrap(), &vec!["a"]);
        assert!(json["data"]["skipped"].as_array().unwrap().is_empty());
        assert_eq!(json["data"]["duplicates"], 1);
        assert_eq!(json["status"], "warning", "有重复应降级 warning: {json}");

        // a 只应有一条 done 转换审计，无「跳过」噪声。
        let (_, ja) = run(&["state", "get", "a"]);
        assert_eq!(ja["data"]["status"], "done");
        let attempts = ja["data"]["state"]["attempts"].as_array().unwrap();
        assert!(
            attempts.iter().all(|a| {
                let r = a["result"].as_str().unwrap_or("");
                !r.contains("batch_transition_done")
            }),
            "去重后不应对已成功模块追加跳过审计: {attempts:?}"
        );
    });
}

#[test]
fn smoke_state_reset() {
    // M4-ROB-01a：state reset 幂等回退失败/中途模块 + 输出 cleanup 作用域。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 注入一个「编译修复中」的失败现场模块。
        inject_module("compile_fixing");

        // 首次 reset：回退到 translating，was_noop=false，附 cleanup 作用域。
        let (code, json) = run(&["state", "reset", "--module", "a"]);
        assert_eq!(code, 0, "非终态 reset 应成功: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["reset_from"], "compile_fixing");
        assert_eq!(json["data"]["reset_to"], "translating");
        assert_eq!(json["data"]["was_noop"], false);
        // cleanup 作用域：单文件模块 member_files = [module key "a"]。
        assert_eq!(json["data"]["cleanup"]["member_files"][0], "a");
        assert!(json["data"]["cleanup"]["next"]
            .as_str()
            .unwrap()
            .contains("--retry"));

        // 二次 reset：幂等空操作（已在干净入口 translating/null），cleanup 给 skip 信号。
        let (code, json) = run(&["state", "reset", "--module", "a"]);
        assert_eq!(code, 0);
        assert_eq!(json["data"]["was_noop"], true, "reset;reset 应幂等: {json}");
        assert_eq!(json["data"]["reset_to"], "translating");
        assert_eq!(
            json["data"]["cleanup"]["skip"], true,
            "noop 的 cleanup 应给 skip 信号（编排层幂等）: {json}"
        );

        // done 终态：不带 --force 应报错。
        inject_module("done");
        let (code, json) = run(&["state", "reset", "--module", "a"]);
        assert_eq!(code, 1, "done 无 --force reset 应报错: {json}");
        assert_eq!(json["status"], "error");

        // done + --force：回退到 translating。
        let (code, json) = run(&["state", "reset", "--module", "a", "--force"]);
        assert_eq!(code, 0, "done + --force reset 应成功: {json}");
        assert_eq!(json["data"]["reset_from"], "done");
        assert_eq!(json["data"]["reset_to"], "translating");
    });
}

#[test]
fn smoke_state_recover() {
    // M4-ROB-01b：state recover 从 watchdog stall 幂等恢复（retry / skip / noop）。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);

        // retry：中途失败模块回退到 translating + cleanup 作用域（同 reset）。
        inject_module("compile_fixing");
        let (code, json) = run(&[
            "state",
            "recover",
            "--module",
            "a",
            "--policy",
            "retry",
            "--reason",
            "stall: stdout 静默 620s",
        ]);
        assert_eq!(code, 0, "retry 恢复应成功: {json}");
        assert_eq!(json["data"]["policy"], "retry");
        assert_eq!(json["data"]["recover_from"], "compile_fixing");
        assert_eq!(json["data"]["recover_to"], "translating");
        assert_eq!(json["data"]["was_noop"], false);
        assert_eq!(json["data"]["recovery"]["member_files"][0], "a");
        assert!(json["data"]["recovery"]["next"]
            .as_str()
            .unwrap()
            .contains("--retry"));

        // retry 二次：已在干净入口 → 幂等 noop，recovery 给 skip 信号。
        let (code, json) = run(&["state", "recover", "--module", "a", "--policy", "retry"]);
        assert_eq!(code, 0);
        assert_eq!(
            json["data"]["was_noop"], true,
            "recover;recover 应幂等: {json}"
        );
        assert_eq!(json["data"]["recovery"]["skip"], true);

        // skip：中途模块 → paused（决策点）+ advice。
        inject_module("testing");
        let (code, json) = run(&[
            "state", "recover", "--module", "a", "--policy", "skip", "--reason", "stall",
        ]);
        assert_eq!(code, 0, "skip 恢复应成功: {json}");
        assert_eq!(json["data"]["policy"], "skip");
        assert_eq!(json["data"]["recover_to"], "paused");
        assert!(json["data"]["recovery"]["advice"]
            .as_str()
            .unwrap()
            .contains("paused"));

        // skip 二次：已 paused → noop。
        let (code, json) = run(&["state", "recover", "--module", "a", "--policy", "skip"]);
        assert_eq!(code, 0);
        assert_eq!(json["data"]["was_noop"], true);

        // done：非 stall 态，两策略均拒绝。
        inject_module("done");
        let (code, json) = run(&["state", "recover", "--module", "a", "--policy", "retry"]);
        assert_eq!(code, 1, "done recover 应报错: {json}");
        assert_eq!(json["status"], "error");
    });
}

#[test]
fn smoke_state_resume() {
    // M4-ROB-01c：state resume 输出额度耗尽/中断后的续跑断点计划（纯查询）。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        // 注入混合态：done 终态 / 运行态 compile_fixing / paused 决策点 / pending 候选 / blocked。
        let path = std::path::Path::new(".rust-migration").join("migration-state.json");
        let mut state: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        state["modules"] = serde_json::json!({
            "d":  { "status": "done" },
            "run":{ "status": "compile_fixing" },
            "pau":{ "status": "paused" },
            "p":  { "status": "pending" },
            "b":  { "status": "blocked" },
        });
        std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

        let (code, json) = run(&["state", "resume"]);
        assert_eq!(code, 0, "resume 应成功: {json}");
        assert_eq!(json["status"], "ok");

        // interrupted：仅运行态模块，且带 recover_command（幂等重入）。
        let interrupted = json["data"]["interrupted"].as_array().unwrap();
        assert_eq!(
            interrupted.len(),
            1,
            "仅 compile_fixing 归 interrupted: {json}"
        );
        assert_eq!(interrupted[0]["module"], "run");
        assert_eq!(interrupted[0]["status"], "compile_fixing");
        assert_eq!(
            interrupted[0]["recover_command"],
            "state recover --module run --policy retry"
        );

        // resume_point：断点位置精简视图（sprint + interrupted 模块名列表）。
        assert_eq!(
            json["data"]["resume_point"]["interrupted"],
            serde_json::json!(["run"]),
            "resume_point.interrupted 应为精简模块名视图: {json}"
        );
        // init 未建 sprint 状态 → resume_point.sprint 为 null。
        assert!(
            json["data"]["resume_point"]["sprint"].is_null(),
            "无 sprint 状态时 resume_point.sprint 应为 null: {json}"
        );

        // paused 单列 awaiting_decision，不进 interrupted（续跑不复活）。
        assert_eq!(json["data"]["awaiting_decision"][0], "pau");
        // pending → next；blocked → blocked。
        assert_eq!(json["data"]["next"][0], "p");
        assert_eq!(json["data"]["blocked"][0], "b");

        // done 终态不出现在任何可操作桶（不重跑）。
        assert!(!interrupted.iter().any(|m| m["module"] == "d"));
        assert!(!json["data"]["next"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "d"));

        // progress 计数对账。
        let p = &json["data"]["progress"];
        assert_eq!(p["done"], 1);
        assert_eq!(p["in_progress"], 1);
        assert_eq!(p["awaiting_decision"], 1);
        assert_eq!(p["pending"], 1);
        assert_eq!(p["blocked"], 1);
        assert_eq!(p["total"], 5);
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
fn e2e_record_metrics_requires_at_least_one_metric() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, json) = run(&["state", "record-metrics", "--module", "file:a.ts"]);
        assert_eq!(code, 1, "无度量参数应报错: {json}");
        assert_eq!(json["status"], "error");
        assert!(
            json["data"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("至少需要"),
            "错误应明确说明至少提供一个度量: {json}"
        );
    });
}

#[test]
fn smoke_stats_loc() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        // 先建目录再 init，确保 source_root 探测到 src/
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        std::fs::write("rust-src/a.rs", "pub fn x() -> i32 {\n    1\n}\n").unwrap();
        let _ = run(&["init"]);

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
        std::fs::create_dir_all("src").unwrap();
        std::fs::write("src/a.ts", "export const x = 1;\n").unwrap();
        let _ = run(&["init"]);
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
fn e2e_stats_compare_supports_python_source() {
    // M3-VAL-02 验收：stats compare 结构门此前硬编码 TS、非 TS 源直接报错
    // （旧 M2-ADV-06 行为）。M3 Python 支持落地后，Python 源应产出真实结构报告
    // （tree-sitter-python 取 Function 数与控制流嵌套），而非报错。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let cfg = std::fs::read_to_string(".rustmigrate.toml").unwrap();
        let cfg = cfg.replace(
            "source_language = \"typescript\"",
            "source_language = \"python\"",
        );
        std::fs::write(".rustmigrate.toml", &cfg).unwrap();
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        // Python 源：2 个函数，最大嵌套 2（for 内 if）。
        std::fs::write(
            "src/a.py",
            "def f(x):\n    for i in range(x):\n        if i > 0:\n            print(i)\n\ndef g():\n    pass\n",
        )
        .unwrap();
        std::fs::write(
            "rust-src/a.rs",
            "pub fn f(x: i64) {\n    for i in 0..x {\n        let _ = i;\n    }\n}\n",
        )
        .unwrap();

        let (code, json) = run(&["stats", "compare", "--source", "src", "--rust", "rust-src"]);
        assert_eq!(code, 0, "Python 源应被支持、正常产出报告: {json}");
        assert_eq!(json["status"], "ok", "应为 ok: {json}");
        assert_eq!(
            json["data"]["source"]["functions"], 2,
            "Python 源应识别 f + g 两个函数: {json}"
        );
        assert_eq!(
            json["data"]["source"]["max_nesting"], 2,
            "for>if 嵌套应为 2 层: {json}"
        );
        assert_eq!(
            json["data"]["source"]["method"], "tree-sitter",
            "Python 源走 tree-sitter: {json}"
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
        // 默认不输出 human 字段（向后兼容）。
        assert!(
            json["data"].get("human").is_none(),
            "默认不应附带 human 字段: {json}"
        );
        // --human：附加人类友好显示名（去 file: 前缀 / src/ 根 / .ts 扩展），内部 key 不变。
        let (code, json) = run(&["state", "get", &key, "--human"]);
        assert_eq!(code, 0, "--human 应成功: {json}");
        assert_eq!(json["data"]["module"], key, "内部 key 应保持不变");
        assert!(
            json["data"]["human"]
                .as_str()
                .is_some_and(|h| !h.is_empty() && !h.contains("file:") && !h.ends_with(".ts")),
            "human 应为去前缀/扩展的友好名: {json}"
        );

        // 衔接验证（codex #5）：run 阶段依赖门禁用组感知 state deps（对 coupled_batch 与 cycle/batch 一致）。
        // 注：linear 三文件合成单组且无组外依赖，dependencies 必为空——本段是「coupled_batch 也能跑通
        // state deps」的衔接 smoke，非去重逻辑验证（非空场景的组内剔除+blocking 由 smoke_state_deps_group_aware
        // 在 circular 簇上强覆盖）。
        let (code, deps_json) = run(&["state", "deps", &key]);
        assert_eq!(code, 0, "state deps 应成功: {deps_json}");
        let deps: Vec<String> = deps_json["data"]["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["module"].as_str().unwrap().to_string())
            .collect();
        assert!(
            deps.is_empty(),
            "linear 单组无组外依赖，deps 应为空: {deps:?}"
        );
        assert_eq!(
            deps_json["data"]["all_ready"], true,
            "组无组外未就绪依赖 → all_ready: {deps_json}"
        );

        // 非代表成员 key 归一：传非代表成员应归一到组代表，返回与组代表查询等价的结果（非仅不报错）。
        let non_rep = members.iter().find(|m| *m != &key).unwrap();
        let (code, j) = run(&["state", "deps", non_rep]);
        assert_eq!(code, 0, "非代表成员 key 应归一而非报错: {j}");
        assert_eq!(
            j["data"]["all_ready"], deps_json["data"]["all_ready"],
            "非代表成员查询应归一到组代表、结果等价: {j}"
        );
    });
}

/// M3-DEC 回归：全机械合批组（成员全为 barrel/pure-type/pure-constant）应标 `composite_kind=batch`
/// （轻量路径）。守护 `all_mechanical` 谓词——它写反或机械分类失效时本测试会变红，而 coupled_batch
/// 测试无法发现（只要有逻辑成员就落 coupled_batch，机械分类失灵也照样通过）。
#[test]
fn e2e_populate_all_mechanical_cluster_is_batch() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("ts-mechanical-batch"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "populate 应成功: {json}");
        assert_eq!(
            json["data"]["module_count"], 1,
            "3 个同目录机械文件应合成 1 个组: {json}"
        );
        let group = &json["data"]["modules"].as_array().unwrap()[0];
        assert_eq!(
            group["composite_kind"], "batch",
            "全机械成员（barrel+pure_type+pure_constant）应标 batch 走轻量路径（非 coupled_batch）: {group}"
        );
        assert_eq!(
            group["member_files"].as_array().unwrap().len(),
            3,
            "组应含全部 3 个机械成员: {group}"
        );
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

/// MDR-013：`classify_file` 产出的 danger 类别须透传进 `migration-state.json`。
/// 构造两个耦合文件——leaf.py 触发 `numeric_precision`（`import math`），app.py 触发
/// `concurrency`（async/await）——decompose 凝聚为 1 个 coupled_batch 组，组 danger = 成员
/// 危险信号并集（去重 + 字典序）。同时校验落盘 `ModuleState.danger` 经 serde 往返一致。
#[test]
fn e2e_populate_danger_signals_into_state() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("leaf.py"),
        "import math\n\n\ndef dist(a, b):\n    return math.sqrt(a * a + b * b)\n",
    )
    .unwrap();
    std::fs::write(
        src.join("app.py"),
        "from .leaf import dist\n\n\nasync def run():\n    return await load()\n\n\nasync def load():\n    return dist(3, 4)\n",
    )
    .unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        let (code, json) = run(&["state", "populate-modules", "--root", "src"]);
        assert_eq!(code, 0, "populate 应成功: {json}");
        let modules = json["data"]["modules"].as_array().unwrap();
        assert_eq!(modules.len(), 1, "两个耦合文件应凝聚为 1 组: {json}");
        let group = &modules[0];
        assert_eq!(group["composite_kind"], "coupled_batch", "{group}");
        // 组 danger = 成员并集：concurrency（app.py）+ numeric_precision（leaf.py），去重 + 字典序。
        assert_eq!(
            group["danger"],
            serde_json::json!(["concurrency", "numeric_precision"]),
            "组 danger 应为成员危险信号并集（去重 + 字典序）: {group}"
        );

        // 落盘校验：state get 反序列化的 ModuleState.danger 透传一致（serde 往返）。
        let (code, json) = run(&["state", "get", "file:app.py"]);
        assert_eq!(code, 0, "state get 应成功: {json}");
        assert_eq!(
            json["data"]["state"]["danger"],
            serde_json::json!(["concurrency", "numeric_precision"]),
            "落盘 ModuleState.danger 应含并集: {json}"
        );
        assert_eq!(run(&["validate", "state"]).0, 0, "落盘后 state 应合法");
    });
}

/// PLG-05：Go `classify_file` 的 danger 类别须端到端透传进 `migration-state.json`，与
/// TS/Python 同链路。两个同包耦合文件——worker.go 触发 `concurrency`（goroutine + channel）、
/// meta.go 触发 `dynamic_reflection`（`import "reflect"`）——decompose 凝聚为 1 coupled_batch，
/// 组 danger = 成员并集（去重 + 字典序）。守护 classify_file → populate → state 对 Go 生效，
/// 支撑 headless run 的 degrade_skip 边界（并发/反射类降级）。
#[test]
fn e2e_populate_go_danger_signals_into_state() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("go.mod"),
        "module example.com/e2e\n\ngo 1.21\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("worker.go"),
        "package app\n\nfunc FanOut(xs []int) int {\n\tch := make(chan int)\n\tfor _, x := range xs {\n\t\tgo func(v int) { ch <- v }(x)\n\t}\n\tsum := 0\n\tfor range xs {\n\t\tsum += <-ch\n\t}\n\treturn sum\n}\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("meta.go"),
        "package app\n\nimport \"reflect\"\n\nfunc TypeName(x any) string {\n\treturn reflect.TypeOf(x).String()\n}\n",
    )
    .unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        assert_eq!(run(&["graph", "build", "--root", "."]).0, 0);

        let (code, json) = run(&["state", "populate-modules", "--root", "."]);
        assert_eq!(code, 0, "populate 应成功: {json}");
        let modules = json["data"]["modules"].as_array().unwrap();
        assert_eq!(modules.len(), 1, "同包两文件应凝聚为 1 组: {json}");
        let group = &modules[0];
        assert_eq!(group["composite_kind"], "coupled_batch", "{group}");
        // 组 danger = 成员并集：concurrency（worker.go）+ dynamic_reflection（meta.go），字典序。
        assert_eq!(
            group["danger"],
            serde_json::json!(["concurrency", "dynamic_reflection"]),
            "组 danger 应为 Go 成员危险信号并集（去重 + 字典序）: {group}"
        );
        assert_eq!(run(&["validate", "state"]).0, 0, "落盘后 state 应合法");
    });
}

/// 回归（PLG-06 发现的 pre-existing bug）：populate 的 tier 分档须按源语言选 adapter。
/// 修复前 `detect_tier` 硬编码 TS adapter → 非 TS 文件 `can_handle=false` 一律保守判 `Full`
/// （Go/Python tier 全失真）；修复后按 `detect_tier_for_lang` 选对 adapter，Go 纯计算单文件
/// （有函数体、无危险信号）应判 `standard`。防 tier 检测退回 TS-only。
#[test]
fn e2e_populate_go_tier_detected_by_language() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("go.mod"),
        "module example.com/t\n\ngo 1.21\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("calc.go"),
        "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n",
    )
    .unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        assert_eq!(run(&["graph", "build", "--root", "."]).0, 0);
        let (code, json) = run(&["state", "populate-modules", "--root", "."]);
        assert_eq!(code, 0, "{json}");
        let m = &json["data"]["modules"][0];
        assert_eq!(
            m["tier"], "standard",
            "Go 纯计算单文件应判 standard（非 TS 硬编码保守 full）: {m}"
        );
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

        // 本测试覆盖 --no-decompose 旧路径（每文件单模块，删文件即整模块消失成孤儿）。
        // decompose 默认路径下删非代表成员只是组缩小、不产孤儿，但删代表成员会触发代表
        // 漂移孤儿——该场景由 e2e_populate_cleans_orphan_decompose_default 钉住。
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
fn e2e_populate_cleans_orphan_decompose_default() {
    // 默认 decompose 路径特有的孤儿：组「缩小 + 代表漂移」——删掉组内字典序最小成员
    // （= 组代表 key），组未消失但代表 key 改变，旧 key 必须被清理为孤儿。--no-decompose
    // 旧路径（每文件单模块）无此场景（见上 e2e_populate_cleans_orphan_pending），故由本
    // 测试钉住默认 decompose 路径的孤儿清理回归。
    // 注：「整组消失」与本场景共享 retain_modules 的 live_keys 集合差路径，存活组场景已由
    // e2e_populate_cleans_orphan_pending 覆盖，无需单独用例。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        // 首轮（默认 decompose）：linear-deps 3 文件凝聚为 1 个 coupled_batch 组，
        // 代表 = 字典序最小成员 file:index.ts。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "首轮填充应成功: {json}");
        assert_eq!(json["data"]["module_count"], 1, "应凝聚为 1 组: {json}");
        assert_eq!(
            json["data"]["modules"][0]["id"], "file:index.ts",
            "首轮组代表应为 file:index.ts: {json}"
        );

        // 删组代表 index.ts（顶层文件，无人 import）后重建源码图。
        std::fs::remove_file("src/index.ts").unwrap();
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        // 重填：组缩小为 {service,utils}（仍是同一个凝聚组，不展开为 2 个单文件模块），
        // 代表漂移到 file:service.ts；旧 key file:index.ts 成孤儿被清理 + warning 告知。
        let (code, json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0, "重填应成功: {json}");
        assert_eq!(
            json["status"], "warning",
            "代表漂移清理孤儿应降级 warning: {json}"
        );

        // 仍是 1 个凝聚组（钉住「组缩小 ≠ 误展开为 2 个单文件模块」）。
        assert_eq!(
            json["data"]["module_count"], 1,
            "组缩小后应仍是 1 个凝聚组，而非展开成单文件模块: {json}"
        );
        let m = &json["data"]["modules"][0];
        assert_eq!(
            m["id"], "file:service.ts",
            "新组代表应漂移为 service: {json}"
        );
        // 组仍含 service+utils（区分「缩小」与「重组」；member_files 按字典序）。
        assert_eq!(
            m["member_files"],
            serde_json::json!(["file:service.ts", "file:utils.ts"]),
            "缩小后组成员应恰为 service+utils: {json}"
        );

        // 孤儿恰为旧代表 file:index.ts，且未误删/误报存活组成员。
        let warnings = json["warnings"].as_array().expect("应有 warnings 数组");
        assert!(
            warnings.iter().any(|w| {
                let s = w.as_str().unwrap_or_default();
                s.contains("孤儿") && s.contains("file:index.ts")
            }),
            "应有含 file:index.ts 的孤儿清理 warning: {json}"
        );
        assert!(
            !warnings.iter().any(|w| {
                let s = w.as_str().unwrap_or_default();
                s.contains("孤儿") && (s.contains("service") || s.contains("utils"))
            }),
            "存活组成员不应被误报为孤儿: {json}"
        );

        // 清理后 state 仍合法（孤儿移除不破坏 schema/依赖约束）。
        assert_eq!(run(&["validate", "state"]).0, 0, "清理后 state 应合法");
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
fn e2e_decompose_aborts_on_high_read_failure() {
    // graph build 用 src 根记录文件相对路径；decompose --root 指向不含源文件的目录
    // （.rust-migration，init 后必存在）→ 全部读失败 → 硬阻断，避免产出全 0-size 退化
    // plan 污染后续 Sprint 规划（旧行为：仅 warn 后照常产出）。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        let (code, json) = run(&["graph", "decompose", "--root", ".rust-migration"]);
        assert_ne!(code, 0, "高比例读失败应非零退出: {json}");
        assert_eq!(json["status"], "error");
        assert!(
            json["data"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("读取失败"),
            "错误消息应说明读取失败原因: {json}"
        );
    });
}

#[test]
fn e2e_decompose_warns_on_low_read_failure() {
    // 阈值低占比侧：少数文件读失败（<50%）仍 warn 放行（status warning），不阻断。
    // linear-deps 有 3 文件，删 1 个 → 1/3≈33% < 50%。
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        assert_eq!(run(&["init"]).0, 0);
        assert_eq!(run(&["graph", "build", "--root", "src"]).0, 0);

        std::fs::remove_file("src/utils.ts").expect("删 utils.ts");
        let (code, json) = run(&["graph", "decompose", "--root", "src"]);
        assert_eq!(code, 0, "低占比读失败应放行不阻断: {json}");
        assert_eq!(json["status"], "warning", "应降级 warning: {json}");
        assert!(
            json["warnings"]
                .as_array()
                .map(|w| w
                    .iter()
                    .any(|x| x.as_str().unwrap_or_default().contains("读取失败")))
                .unwrap_or(false),
            "warnings 应含读取失败告警: {json}"
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

// === stats quality（M4-QUAL-01：迁移质量度量框架）===

#[test]
fn e2e_stats_quality_on_populated_project() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        let (code, _) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0);

        let (code, json) = run(&["stats", "quality"]);
        assert_eq!(code, 0, "stats quality 应成功: {json}");
        assert_eq!(
            json["status"], "warning",
            "缺 rust 目录应降级 warning: {json}"
        );
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("loc_ratio 留空")),
            "应明确提示 loc_ratio 无法计算: {json}"
        );

        let data = &json["data"];
        assert!(
            data["degrade_rate"].is_number(),
            "应有 degrade_rate 数值: {json}"
        );
        assert!(
            data["total_modules"].is_number(),
            "应有 total_modules: {json}"
        );
        assert!(data["modules"].is_array(), "应有 modules 数组: {json}");
        assert!(
            data["data_completeness"].is_number(),
            "应有 data_completeness: {json}"
        );

        // populate 后全是 pending，无编译/测试数据 → degrade_rate=0，final_score 全 None
        assert_eq!(data["degrade_rate"], 0.0, "全 pending → 无降级: {json}");
        let modules = data["modules"].as_array().unwrap();
        assert!(!modules.is_empty(), "至少 1 个模块: {json}");
        for m in modules {
            assert_eq!(m["status"], "pending", "populate 后应为 pending: {m}");
            assert!(
                m["final_score"].is_null(),
                "pending 模块无足够指标 → final_score=null: {m}"
            );
        }
    });
}

#[test]
fn e2e_stats_quality_with_transitions() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);
        let (code, populate_json) = run(&["state", "populate-modules"]);
        assert_eq!(code, 0);

        let modules = populate_json["data"]["modules"].as_array().unwrap();
        let key = modules[0]["id"].as_str().unwrap();

        // 推进到 translating → compile_fixing → testing
        let (code, _) = run(&[
            "state",
            "transition",
            "--module",
            key,
            "--to",
            "translating",
            "--reason",
            "test",
        ]);
        assert_eq!(code, 0);
        let (code, _) = run(&[
            "state",
            "transition",
            "--module",
            key,
            "--to",
            "compile_fixing",
            "--reason",
            "test",
        ]);
        assert_eq!(code, 0);
        let (code, _) = run(&[
            "state",
            "transition",
            "--module",
            key,
            "--to",
            "testing",
            "--reason",
            "test",
        ]);
        assert_eq!(code, 0);

        let (code, metrics_json) = run(&[
            "state",
            "record-metrics",
            "--module",
            key,
            "--test-pass-rate",
            "276/276",
            "--known-differences",
            "0",
        ]);
        assert_eq!(code, 0, "record-metrics 应成功: {metrics_json}");
        assert_eq!(metrics_json["data"]["test_pass_rate"], "276/276");

        let (code, json) = run(&["stats", "quality"]);
        assert_eq!(code, 0, "stats quality 应成功: {json}");

        let mq = json["data"]["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["module"].as_str() == Some(key))
            .expect("应找到目标模块");

        // testing 状态 + 差异测试通过率 → 两项确定性指标，final_score 可计算。
        assert_eq!(
            mq["deterministic"]["compile_pass"], true,
            "testing 意味着编译通过: {mq}"
        );
        assert_eq!(mq["deterministic"]["test_pass_rate"], 1.0);
        assert_eq!(mq["behavior_coverage"], 1.0);
        assert!(mq["final_score"].is_number(), "应产出 final_score: {mq}");
    });
}

#[test]
fn e2e_stats_quality_go_loc_ratio_scores_mechanical_module() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        std::fs::create_dir_all("src").unwrap();
        std::fs::create_dir_all("rust-src").unwrap();
        std::fs::write(
            "src/version.go",
            "package semver\n\nfunc Major() uint64 { return 1 }\n",
        )
        .unwrap();
        std::fs::write("src/go.mod", "module example.com/semver\n\ngo 1.21\n").unwrap();
        std::fs::write("rust-src/version.rs", "pub fn major() -> u64 { 1 }\n").unwrap();

        let _ = run(&["init"]);
        let (code, build_json) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0, "Go graph build 应成功: {build_json}");
        let (code, populate_json) = run(&["state", "populate-modules", "--root", "src"]);
        assert_eq!(code, 0, "Go populate 应成功: {populate_json}");
        let key = populate_json["data"]["modules"][0]["id"].as_str().unwrap();

        for status in [
            "translating",
            "compile_fixing",
            "testing",
            "reviewing",
            "done",
        ] {
            let (code, json) = run(&["state", "transition", "--module", key, "--to", status]);
            assert_eq!(code, 0, "推进 {status} 应成功: {json}");
        }

        let (code, json) = run(&["stats", "quality", "--source", "src", "--rust", "rust-src"]);
        assert_eq!(code, 0, "Go quality 应成功: {json}");
        assert_eq!(
            json["status"], "warning",
            "项目级 loc_ratio 近似应明确 warning: {json}"
        );
        let module = &json["data"]["modules"][0];
        assert!(
            module["deterministic"]["loc_ratio"].is_number(),
            "Go 应绕开 compare_structure 的 nesting NotImplemented，产出 loc_ratio: {json}"
        );
        assert!(
            module["final_score"].is_number(),
            "mechanical done 应靠 compile_pass + loc_ratio 产出 final_score: {json}"
        );
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("项目级近似值")),
            "应披露 loc_ratio 粒度近似: {json}"
        );
    });
}

#[test]
fn e2e_stats_community_on_linear_deps() {
    let project = temp_linear_project();
    with_cwd(project.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        let (code, json) = run(&["stats", "community"]);
        assert_eq!(code, 0, "stats community 应成功: {json}");

        let data = &json["data"];
        assert!(data["file_count"].is_number(), "应有 file_count: {json}");
        assert!(data["nmi"].is_number(), "应有 nmi: {json}");
        assert!(data["ari"].is_number(), "应有 ari: {json}");
        assert!(
            data["deviation_score"].is_number(),
            "应有 deviation_score: {json}"
        );
        assert!(
            data["communities"].is_array(),
            "应有 communities 数组: {json}"
        );

        let file_count = data["file_count"].as_u64().unwrap();
        assert!(file_count >= 3, "linear-deps 应至少 3 个文件节点: {json}");

        let dev_score = data["deviation_score"].as_f64().unwrap();
        assert!(
            (0.0..=1.0).contains(&dev_score),
            "deviation_score 应在 [0,1]: {dev_score}"
        );
    });
}

#[test]
fn e2e_stats_community_diamond_deps() {
    let tmp = tempfile::tempdir().unwrap();
    copy_dir(&fixtures_dir().join("diamond-deps"), tmp.path());
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);
        let (code, _) = run(&["graph", "build", "--root", "src"]);
        assert_eq!(code, 0);

        let (code, json) = run(&["stats", "community"]);
        assert_eq!(code, 0, "stats community 应成功: {json}");
        let data = &json["data"];
        assert!(data["file_count"].as_u64().unwrap() >= 4);
        assert!(data["community_count"].as_u64().unwrap() >= 1);
    });
}

#[test]
fn e2e_stats_quality_empty_project() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let _ = run(&["init"]);

        let (code, json) = run(&["stats", "quality"]);
        assert_eq!(code, 0, "空项目 stats quality 应成功: {json}");
        assert_eq!(json["data"]["total_modules"], 0);
        assert_eq!(json["data"]["degrade_rate"], 0.0);
        assert!(json["data"]["avg_final_score"].is_null());
        let modules = json["data"]["modules"].as_array().unwrap();
        assert!(modules.is_empty());
    });
}

// === M4-GOV-01：validate rules（规则版本陈旧检测）===

/// 仓库根目录（`fixtures/` 的父目录）。
fn repo_root() -> PathBuf {
    fixtures_dir().parent().unwrap().to_path_buf()
}

/// 权威规则清单 + 适配器根目录（随插件发布）。
fn shipped_registry() -> PathBuf {
    repo_root().join("plugin/skills/migrate/references/rule-registry.json")
}
fn shipped_adapters() -> PathBuf {
    repo_root().join("plugin/skills/migrate/adapters")
}

/// 回归守卫：随发布的各适配器 `porting-template.md` 的 `rule_version` 与权威清单一致。
/// 任一模板 bump 规则版本却漏改清单（或反之）会红于此。
#[test]
fn e2e_validate_rules_shipped_templates_consistent() {
    // 在无 .rustmigrate.toml 的临时 cwd 运行 → enforce 取默认 true；一致则不受影响。
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&[
            "validate",
            "rules",
            "--registry",
            shipped_registry().to_str().unwrap(),
            "--adapters-dir",
            shipped_adapters().to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "随发布模板应与权威清单一致: {json}");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["data"]["consistent"], true);
        assert_eq!(json["data"]["enforce"], true);
        // 三个适配器（go/python/typescript）均无 issue。
        let checks = json["data"]["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 3, "应扫描到 3 个适配器模板: {json}");
        for c in checks {
            assert!(
                c["issues"].as_array().unwrap().is_empty(),
                "适配器 {} 不应有不一致项: {json}",
                c["adapter"]
            );
        }
    });
}

/// 写一个「漂移」权威清单到临时目录（RULE-3 版本不符 + 新增未在模板声明的 RULE-99）。
fn write_drifted_registry(dir: &Path) -> PathBuf {
    let path = dir.join("bad-registry.json");
    std::fs::write(
        &path,
        r#"{"schema_version":"1.0","rules":{
            "RULE-2":"v1.0.0","RULE-3":"v2.0.0","RULE-6":"v1.0.0","RULE-7":"v1.0.0",
            "RULE-8":"v1.0.0","RULE-10":"v1.0.0","RULE-12":"v1.0.0","RULE-15":"v1.0.0",
            "RULE-20":"v1.0.0","RULE-99":"v1.0.0"}}"#,
    )
    .unwrap();
    path
}

/// enforce 默认 true：模板与清单漂移时返回错误（退出码 1，非静默）。
#[test]
fn e2e_validate_rules_enforce_true_drift_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let registry = write_drifted_registry(tmp.path());
    with_cwd(tmp.path(), || {
        let (code, json) = run(&[
            "validate",
            "rules",
            "--registry",
            registry.to_str().unwrap(),
            "--adapters-dir",
            shipped_adapters().to_str().unwrap(),
        ]);
        assert_eq!(code, 1, "enforce=true 漂移应退出码 1: {json}");
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["error_code"], "E008");
        assert!(
            json["data"]["message"]
                .as_str()
                .unwrap()
                .contains("rule_version"),
            "错误信息应指明 rule_version 不一致: {json}"
        );
        // important-B：报错时结构化 checks 仍经 details flatten 提升到 data 顶层，
        // 供 CI（默认 enforce=true）机读逐条不一致清单，而非仅拿到一句 message。
        let checks = json["data"]["checks"]
            .as_array()
            .unwrap_or_else(|| panic!("报错响应应保留 data.checks: {json}"));
        assert_eq!(checks.len(), 3, "应保留全部 3 个适配器的校验结果: {json}");
        let all_kinds: Vec<&str> = checks
            .iter()
            .flat_map(|c| c["issues"].as_array().unwrap())
            .map(|i| i["kind"].as_str().unwrap())
            .collect();
        assert!(
            all_kinds.contains(&"version_mismatch"),
            "应含版本不符: {json}"
        );
        assert!(
            all_kinds.contains(&"missing_in_template"),
            "应含模板缺失: {json}"
        );
    });
}

/// enforce=false：漂移降级为 warning（退出码 0），逐条不一致项落 `data.checks[].issues`。
#[test]
fn e2e_validate_rules_enforce_false_drift_warns() {
    let tmp = tempfile::tempdir().unwrap();
    let registry = write_drifted_registry(tmp.path());
    with_cwd(tmp.path(), || {
        std::fs::write(
            ".rustmigrate.toml",
            "[rules]\nenforce_rule_version_consistency = false\n",
        )
        .unwrap();
        let (code, json) = run(&[
            "validate",
            "rules",
            "--registry",
            registry.to_str().unwrap(),
            "--adapters-dir",
            shipped_adapters().to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "enforce=false 漂移应退出码 0: {json}");
        assert_eq!(json["status"], "warning");
        assert_eq!(json["data"]["consistent"], false);
        assert_eq!(json["data"]["enforce"], false);
        // 机读逐条 issue：跨全部模板聚合，含 version_mismatch(RULE-3) 与 missing_in_template(RULE-99)。
        let checks = json["data"]["checks"].as_array().unwrap();
        let kinds: Vec<&str> = checks
            .iter()
            .flat_map(|c| c["issues"].as_array().unwrap())
            .map(|i| i["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"version_mismatch"), "应含版本不符: {json}");
        assert!(
            kinds.contains(&"missing_in_template"),
            "应含模板缺失: {json}"
        );
    });
}

/// 清单文件不存在：返回错误（不静默通过）。
#[test]
fn e2e_validate_rules_missing_registry_errors() {
    let tmp = tempfile::tempdir().unwrap();
    with_cwd(tmp.path(), || {
        let (code, json) = run(&[
            "validate",
            "rules",
            "--registry",
            "does-not-exist.json",
            "--adapters-dir",
            shipped_adapters().to_str().unwrap(),
        ]);
        assert_eq!(code, 1, "清单不存在应退出码 1: {json}");
        assert_eq!(json["status"], "error");
    });
}
