//! Python fixture 端到端验收（PY-08）。
//!
//! 用 `build_graph_for_lang(root, SourceLang::Python)` 对 4 个 Python fixture
//! 验证 ground-truth.json 的节点/边/拓扑偏序约束，并补充 Python 特有断言：
//! - `__all__` 导出（`is_exported` + `exports` 边）
//! - 继承统一为 `extends` 边（Python 无 `Implements` sub_kind）
//! - signature round-trip（含类型注解、类骨架）
//! - `if TYPE_CHECKING` 块识别为 `ImportKind::StaticType`
//! - `__init__.py` 包结构 + re-export 透传偏序

use rustmigrate_core::graph::build::build_graph_for_lang;
use rustmigrate_core::graph::topo::{detect_cycles, migration_sequence, topological_sort};
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::lang::registry::create_adapter;
use rustmigrate_core::lang::ImportKind;
use rustmigrate_core::types::common::{NodeId, SourceLang};
use rustmigrate_core::types::graph::EdgeSubKind;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(3).unwrap().join("fixtures")
}

fn build(fixture: &str) -> SourceGraph {
    let root = fixtures_dir().join(fixture);
    build_graph_for_lang(&root, SourceLang::Python)
        .unwrap_or_else(|e| panic!("{fixture} 构建失败: {e}"))
}

// =============================================================
// ground-truth.json 加载（schema 与 ground_truth.rs 对齐）
// =============================================================

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GroundTruth {
    #[serde(default)]
    description: String,
    #[serde(default)]
    nodes: Vec<NodeSpec>,
    #[serde(default)]
    edges: Vec<EdgeSpec>,
    #[serde(default)]
    topo_order_constraints: Vec<(String, String)>,
    #[serde(default)]
    topo_sort: Option<TopoSortSpec>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // node_type/note 仅为完整反序列化 ground-truth.json，未在断言中读取
struct NodeSpec {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    is_exported: Option<bool>,
    #[serde(default)]
    is_async: Option<bool>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EdgeSpec {
    source: String,
    target: String,
    #[serde(rename = "type")]
    edge_type: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    sub_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TopoSortSpec {
    #[serde(default)]
    expect_error: bool,
    #[serde(default)]
    cycle_contains: Vec<String>,
}

fn load_ground_truth(fixture: &str) -> GroundTruth {
    let path = fixtures_dir().join(fixture).join("ground-truth.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 {fixture}/ground-truth.json 失败: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("解析 {fixture}/ground-truth.json 失败: {e}"))
}

// =============================================================
// 通用断言（节点存在 / 节点属性 / 边存在 / 拓扑偏序）
// =============================================================

/// 节点双向严格校验：ground-truth 节点集合必须与实际图节点集合完全相等
/// （既不能缺失，也不能有未声明的多余节点——后者防止 adapter 多产出节点被掩盖）。
fn assert_nodes(fixture: &str) {
    let gt = load_ground_truth(fixture);
    let graph = build(fixture);
    let actual: HashSet<String> = graph.nodes().map(|n| n.id.as_str().to_owned()).collect();
    let expected: HashSet<String> = gt.nodes.iter().map(|n| n.id.clone()).collect();

    let missing: Vec<&String> = expected.difference(&actual).collect();
    let unexpected: Vec<&String> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "{fixture} 节点集合不匹配:\n  缺失（ground-truth 有但图中无）: {missing:?}\n  多余（图中有但 ground-truth 未声明）: {unexpected:?}"
    );
}

fn assert_node_attributes(fixture: &str) {
    let gt = load_ground_truth(fixture);
    let graph = build(fixture);

    for spec in &gt.nodes {
        let idx = graph
            .node_index(&NodeId::new(&spec.id))
            .unwrap_or_else(|| panic!("{fixture} 节点 {} 不存在", spec.id));
        let node = graph.node(idx).unwrap();

        if let Some(expected) = spec.is_exported {
            assert_eq!(
                node.is_exported, expected,
                "{fixture} 节点 {} is_exported 不匹配: 期望 {}, 实际 {}",
                spec.id, expected, node.is_exported
            );
        }
        if let Some(expected) = spec.is_async {
            assert_eq!(
                node.is_async, expected,
                "{fixture} 节点 {} is_async 不匹配: 期望 {}, 实际 {}",
                spec.id, expected, node.is_async
            );
        }
    }
}

/// 边四元组 key：`(source, target, edge_type, sub_kind)`。
/// 纳入 `sub_kind` 后，构造调用（`Constructor`）/继承的子类型标注错误或缺失都会被捕获；
/// 无 sub_kind 统一表示为 `"None"`，避免「该为 None 却标了值」漏检。
fn edge_key(source: &str, target: &str, edge_type: &str, sub_kind: &str) -> String {
    format!("{source}\t{target}\t{edge_type}\t{sub_kind}")
}

/// 边双向严格校验：ground-truth 边集合（含 sub_kind）必须与实际图边集合完全相等。
fn assert_edges(fixture: &str) {
    let gt = load_ground_truth(fixture);
    let graph = build(fixture);

    let actual: HashSet<String> = graph
        .edges()
        .map(|e| {
            let sub = e
                .sub_kind
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| "None".to_owned());
            edge_key(
                e.source.as_str(),
                e.target.as_str(),
                &e.edge_type.to_string(),
                &sub,
            )
        })
        .collect();
    let expected: HashSet<String> = gt
        .edges
        .iter()
        .map(|spec| {
            let sub = spec.sub_kind.clone().unwrap_or_else(|| "None".to_owned());
            edge_key(&spec.source, &spec.target, &spec.edge_type, &sub)
        })
        .collect();

    let missing: Vec<&String> = expected.difference(&actual).collect();
    let unexpected: Vec<&String> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "{fixture} 边集合不匹配（key=source\\ttarget\\ttype\\tsub_kind）:\n  缺失: {missing:?}\n  多余: {unexpected:?}"
    );
}

fn assert_topo_constraints(fixture: &str) {
    let gt = load_ground_truth(fixture);
    let graph = build(fixture);
    let order = topological_sort(&graph).unwrap_or_else(|e| panic!("{fixture} 拓扑排序失败: {e}"));

    for (before, after) in &gt.topo_order_constraints {
        let pb = order
            .iter()
            .position(|id| id.as_str() == before)
            .unwrap_or_else(|| panic!("{fixture} 拓扑序中找不到 {before}"));
        let pa = order
            .iter()
            .position(|id| id.as_str() == after)
            .unwrap_or_else(|| panic!("{fixture} 拓扑序中找不到 {after}"));
        assert!(
            pb < pa,
            "{fixture}: {before} 应排在 {after} 前，实际: {order:?}"
        );
    }
}

// =============================================================
// py-linear-deps
// =============================================================

#[test]
fn py_linear_deps_nodes() {
    assert_nodes("py-linear-deps");
}

#[test]
fn py_linear_deps_node_attributes() {
    assert_node_attributes("py-linear-deps");
}

#[test]
fn py_linear_deps_edges() {
    assert_edges("py-linear-deps");
}

#[test]
fn py_linear_deps_topo() {
    assert_topo_constraints("py-linear-deps");
}

#[test]
fn py_linear_deps_constructor_call_sub_kind() {
    let graph = build("py-linear-deps");
    let ctor = graph.edges().find(|e| {
        e.source.as_str() == "file:src/index.py"
            && e.target.as_str() == "class:src/service.py:NumberService"
            && e.edge_type.to_string() == "calls"
    });
    let ctor = ctor.expect("index.py 构造 NumberService 应有 calls 边");
    assert_eq!(
        ctor.sub_kind,
        Some(EdgeSubKind::Constructor),
        "Python `Foo()` 构造调用应标记 sub_kind=Constructor"
    );
}

#[test]
fn py_linear_deps_signature_round_trip() {
    let graph = build("py-linear-deps");
    // async 函数签名应保留 `async def` 前缀和返回类型注解
    let fetch = graph
        .node(
            graph
                .node_index(&NodeId::new("function:src/utils.py:fetch_data"))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        fetch.signature.as_deref(),
        Some("async def fetch_data(url: str) -> List[int]"),
        "fetch_data 签名 round-trip 应含 async 与类型注解"
    );
    // 方法签名应保留 self 与参数注解
    let normalize = graph
        .node(
            graph
                .node_index(&NodeId::new(
                    "function:src/service.py:NumberService.normalize",
                ))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        normalize.signature.as_deref(),
        Some("def normalize(self, value: int) -> int")
    );
}

// =============================================================
// py-diamond-deps
// =============================================================

#[test]
fn py_diamond_deps_nodes() {
    assert_nodes("py-diamond-deps");
}

#[test]
fn py_diamond_deps_node_attributes() {
    assert_node_attributes("py-diamond-deps");
}

#[test]
fn py_diamond_deps_edges() {
    assert_edges("py-diamond-deps");
}

#[test]
fn py_diamond_deps_topo() {
    assert_topo_constraints("py-diamond-deps");
}

#[test]
fn py_diamond_deps_extends_no_implements() {
    // Python 的 ABC 继承统一表达为 extends 边，不区分 implements（TS 专属语义）。
    let graph = build("py-diamond-deps");
    let extends: Vec<_> = graph
        .edges()
        .filter(|e| e.edge_type.to_string() == "extends")
        .collect();

    assert_eq!(
        extends.len(),
        2,
        "应有 AuthService/UserModel 两条 extends 边"
    );
    for e in &extends {
        assert!(
            e.target.as_str() == "class:src/base.py:Serializable",
            "extends 应指向 Serializable，实际: {}",
            e.target.as_str()
        );
        assert_ne!(
            e.sub_kind,
            Some(EdgeSubKind::Implements),
            "Python 继承不应携带 Implements sub_kind（{} -> {}）",
            e.source.as_str(),
            e.target.as_str()
        );
    }
}

#[test]
fn py_diamond_deps_class_signature_with_bases() {
    let graph = build("py-diamond-deps");
    let auth = graph
        .node(
            graph
                .node_index(&NodeId::new("class:src/auth.py:AuthService"))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        auth.signature.as_deref(),
        Some("class AuthService(Serializable) [__init__, serialize]"),
        "class 签名应含基类与方法骨架"
    );
}

// =============================================================
// py-circular-deps
// =============================================================

#[test]
fn py_circular_deps_nodes() {
    assert_nodes("py-circular-deps");
}

#[test]
fn py_circular_deps_node_attributes() {
    assert_node_attributes("py-circular-deps");
}

#[test]
fn py_circular_deps_edges() {
    assert_edges("py-circular-deps");
}

#[test]
fn py_circular_deps_topo_error() {
    let gt = load_ground_truth("py-circular-deps");
    let spec = gt.topo_sort.expect("py-circular-deps 应有 topo_sort 字段");
    assert!(spec.expect_error, "应期望拓扑排序失败");

    let graph = build("py-circular-deps");
    assert!(
        topological_sort(&graph).is_err(),
        "循环依赖拓扑排序应返回错误"
    );

    let cycles = detect_cycles(&graph);
    assert!(!cycles.is_empty(), "应检测到至少一个环");

    // 精确断言：cycle_contains 的全部成员必须落在「同一个」环（SCC）内，
    // 而非分散在多个互不相关的环里——后者会让「a/b 实为两个独立环」的错误蒙混过关。
    let members = &spec.cycle_contains;
    let joint = cycles.iter().any(|c| {
        members
            .iter()
            .all(|m| c.iter().any(|id| id.as_str().contains(m)))
    });
    assert!(
        joint,
        "应存在同时包含 {members:?} 全部成员的单个环，实际环: {cycles:?}"
    );
}

#[test]
fn py_circular_deps_shared_not_in_cycle() {
    let graph = build("py-circular-deps");
    let cycles = detect_cycles(&graph);
    let shared_in_cycle = cycles
        .iter()
        .any(|c| c.iter().any(|id| id.as_str().contains("shared")));
    assert!(
        !shared_in_cycle,
        "shared.py 不应出现在任何环中，实际环: {cycles:?}"
    );
}

#[test]
fn py_circular_deps_migration_sequence_has_cycles() {
    let graph = build("py-circular-deps");
    let seq = migration_sequence(&graph);
    assert!(seq.has_cycles(), "circular 应标记有环");
    assert!(!seq.order.is_empty(), "有环时仍应生成尽力排序");
}

// =============================================================
// py-pkg-deps（__init__.py 包 + re-export + TYPE_CHECKING）
// =============================================================

#[test]
fn py_pkg_deps_nodes() {
    assert_nodes("py-pkg-deps");
}

#[test]
fn py_pkg_deps_node_attributes() {
    assert_node_attributes("py-pkg-deps");
}

#[test]
fn py_pkg_deps_edges() {
    assert_edges("py-pkg-deps");
}

#[test]
fn py_pkg_deps_reexport_topo() {
    // re-export 透传偏序：main -> pkg/__init__ -> {base, impl}，无环可拓扑排序。
    assert_topo_constraints("py-pkg-deps");
}

#[test]
fn py_pkg_deps_init_is_package_node() {
    let graph = build("py-pkg-deps");
    assert!(
        graph
            .node_index(&NodeId::new("file:pkg/__init__.py"))
            .is_some(),
        "__init__.py 应作为包入口 File 节点存在"
    );
    // main 经包入口导入，应有 main -> __init__ 的 imports 边
    let has_import = graph.edges().any(|e| {
        e.source.as_str() == "file:main.py"
            && e.target.as_str() == "file:pkg/__init__.py"
            && e.edge_type.to_string() == "imports"
    });
    assert!(has_import, "main 应通过包入口 __init__.py 导入");
}

#[test]
fn py_pkg_deps_type_checking_import_is_static_type() {
    // `if TYPE_CHECKING:` 块内的导入应标记为 ImportKind::StaticType。
    let mut adapter = create_adapter(SourceLang::Python).unwrap();
    let source = std::fs::read_to_string(fixtures_dir().join("py-pkg-deps/pkg/base.py")).unwrap();
    let analysis = adapter.analyze_file(&source, "pkg/base.py").unwrap();

    let type_imports: Vec<_> = analysis
        .imports
        .iter()
        .filter(|i| i.kind == ImportKind::StaticType)
        .collect();
    assert_eq!(
        type_imports.len(),
        1,
        "base.py 的 TYPE_CHECKING 块应产出 1 个 StaticType import，实际: {:?}",
        analysis
            .imports
            .iter()
            .map(|i| (&i.module_path, i.kind))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        type_imports[0].module_path, ".types",
        "StaticType import 模块路径应为 .types"
    );
}
