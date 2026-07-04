//! Go fixture 端到端验收（GO-08 + GO-09）。
//!
//! 用 `build_graph_for_lang(root, SourceLang::Go)` 对 4 个 Go fixture 验证
//! ground-truth.json 的节点/边/拓扑偏序约束，并补充 Go 特有断言：
//! - 首字母大写导出约定（`is_exported` + `exports` 边，含导出方法）
//! - 模块级 const/var 激活 `NodeType::Variable`（M2 预留变体）
//! - 多返回值签名 round-trip（`(int, int)`）
//! - struct 同包嵌入 → `extends` 边；interface 隐式实现**不**连 `Implements`（D-M4-02）
//! - 同包 composite literal 构造 → `sub_kind = Constructor`
//! - 跨包 `pkg.Func` 调用解析到包代表文件（字典序第一非 `_test.go`）
//! - 文件过滤：`_test.go` / 平台后缀完全排除；`//go:build` 门控 → 孤立 File 节点
//! - 包级环检测（SCC）
//! - GO-09：同 package 多文件凝聚到同一 `DecompUnit`
//!
//! ground-truth.json 的 schema 与 `python_ground_truth.rs` / `ground_truth.rs` 对齐。

use rustmigrate_core::graph::build::build_graph_for_lang;
use rustmigrate_core::graph::decompose::plan_decomposition;
use rustmigrate_core::graph::topo::{detect_cycles, migration_sequence, topological_sort};
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::types::common::{NodeId, SourceLang};
use rustmigrate_core::types::graph::{EdgeSubKind, NodeType};
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(3).unwrap().join("fixtures")
}

fn build(fixture: &str) -> SourceGraph {
    let root = fixtures_dir().join(fixture);
    build_graph_for_lang(&root, SourceLang::Go)
        .unwrap_or_else(|e| panic!("{fixture} 构建失败: {e}"))
}

// =============================================================
// ground-truth.json 加载（schema 与 python_ground_truth.rs 对齐）
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
#[allow(dead_code)] // note 仅为完整反序列化 ground-truth.json，未在断言中读取
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

        // node_type 双向校验：ground-truth 的 `type` 字段（PascalCase，如 "Class"）须与实际
        // NodeType 相符——防止 `type` 列写错却因 id 前缀恰巧一致而漏检（codex I-3 / 主审 nit）。
        assert_eq!(
            format!("{:?}", node.node_type),
            spec.node_type,
            "{fixture} 节点 {} node_type 不匹配: 期望 {}, 实际 {:?}",
            spec.id,
            spec.node_type,
            node.node_type
        );

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
/// 纳入 `sub_kind` 后，构造调用（`Constructor`）的子类型标注错误或缺失都会被捕获；
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

fn sig_of(graph: &SourceGraph, id: &str) -> Option<String> {
    let idx = graph.node_index(&NodeId::new(id))?;
    graph.node(idx)?.signature.clone()
}

// =============================================================
// go-linear-deps
// =============================================================

#[test]
fn go_linear_deps_nodes() {
    assert_nodes("go-linear-deps");
}

#[test]
fn go_linear_deps_node_attributes() {
    assert_node_attributes("go-linear-deps");
}

#[test]
fn go_linear_deps_edges() {
    assert_edges("go-linear-deps");
}

#[test]
fn go_linear_deps_topo() {
    assert_topo_constraints("go-linear-deps");
}

#[test]
fn go_linear_deps_multi_return_signature_round_trip() {
    let graph = build("go-linear-deps");
    assert_eq!(
        sig_of(&graph, "function:utils/utils.go:MinMax").as_deref(),
        Some("func MinMax(a, b int) (int, int)"),
        "多返回值签名应完整保留 (int, int)"
    );
    // 方法签名应含 receiver。
    assert_eq!(
        sig_of(&graph, "function:service/service.go:Scaler.Scale").as_deref(),
        Some("func (s Scaler) Scale(v int) int"),
    );
}

#[test]
fn go_linear_deps_variable_nodes_activated() {
    // 模块级 const/var 应激活 NodeType::Variable，并按首字母大小写判定导出。
    let graph = build("go-linear-deps");
    let max = graph
        .node(
            graph
                .node_index(&NodeId::new("variable:utils/utils.go:MaxValue"))
                .expect("MaxValue Variable 节点"),
        )
        .unwrap();
    assert_eq!(max.node_type, NodeType::Variable);
    assert!(max.is_exported, "MaxValue 大写应导出");

    let low = graph
        .node(
            graph
                .node_index(&NodeId::new("variable:utils/utils.go:clampLo"))
                .expect("clampLo Variable 节点"),
        )
        .unwrap();
    assert_eq!(low.node_type, NodeType::Variable);
    assert!(!low.is_exported, "clampLo 小写不导出（无 exports 边）");
}

#[test]
fn go_linear_deps_same_package_constructor_sub_kind() {
    // 同包 composite literal `Scaler{}` 应标记 sub_kind=Constructor。
    let graph = build("go-linear-deps");
    let ctor = graph
        .edges()
        .find(|e| {
            e.source.as_str() == "file:service/service.go"
                && e.target.as_str() == "class:service/service.go:Scaler"
                && e.edge_type.to_string() == "calls"
        })
        .expect("service.go 构造 Scaler 应有 calls 边");
    assert_eq!(
        ctor.sub_kind,
        Some(EdgeSubKind::Constructor),
        "Go `Foo{{}}` 同包构造应标记 sub_kind=Constructor"
    );
}

#[test]
fn go_linear_deps_cross_package_call_to_representative() {
    // 跨包 utils.Clamp 调用应解析到 utils 包代表文件的 Clamp 函数节点。
    let graph = build("go-linear-deps");
    let has = graph.edges().any(|e| {
        e.edge_type.to_string() == "calls"
            && e.source.as_str() == "file:service/service.go"
            && e.target.as_str() == "function:utils/utils.go:Clamp"
    });
    assert!(has, "跨包 utils.Clamp 调用应连到 utils/utils.go:Clamp");
}

// =============================================================
// go-diamond-deps
// =============================================================

#[test]
fn go_diamond_deps_nodes() {
    assert_nodes("go-diamond-deps");
}

#[test]
fn go_diamond_deps_node_attributes() {
    assert_node_attributes("go-diamond-deps");
}

#[test]
fn go_diamond_deps_edges() {
    assert_edges("go-diamond-deps");
}

#[test]
fn go_diamond_deps_topo() {
    assert_topo_constraints("go-diamond-deps");
}

#[test]
fn go_diamond_deps_struct_embed_is_extends_no_implements() {
    let graph = build("go-diamond-deps");
    // struct 嵌入 → 唯一一条 extends 边 Circle → Base。
    let extends: Vec<_> = graph
        .edges()
        .filter(|e| e.edge_type.to_string() == "extends")
        .collect();
    assert_eq!(extends.len(), 1, "应有唯一 struct 嵌入 extends 边");
    assert_eq!(extends[0].source.as_str(), "class:geom/geom.go:Circle");
    assert_eq!(extends[0].target.as_str(), "class:geom/geom.go:Base");

    // Circle 隐式满足 Shape，但**不应**连 Implements 边（D-M4-02）。
    let has_implements = graph
        .edges()
        .any(|e| e.sub_kind == Some(EdgeSubKind::Implements));
    assert!(
        !has_implements,
        "Go interface 隐式实现不应产生 Implements sub_kind"
    );
    // 也不应有指向 interface 的 extends（隐式实现不建结构边）。
    let to_shape = graph.edges().any(|e| {
        e.target.as_str() == "interface:geom/geom.go:Shape" && e.edge_type.to_string() == "extends"
    });
    assert!(!to_shape, "不应有指向 Shape interface 的 extends 边");
}

#[test]
fn go_diamond_deps_interface_and_type_signatures() {
    let graph = build("go-diamond-deps");
    // interface 签名保留方法集。
    let shape = sig_of(&graph, "interface:geom/geom.go:Shape").expect("Shape 签名");
    assert!(
        shape.contains("Area() float64"),
        "interface 签名应含方法集: {shape}"
    );
    // struct 签名保留字段（含嵌入字段）。
    let circle = sig_of(&graph, "class:geom/geom.go:Circle").expect("Circle 签名");
    assert!(
        circle.contains("Base") && circle.contains("R float64"),
        "struct 签名应含嵌入字段与普通字段: {circle}"
    );
}

// =============================================================
// go-circular-deps
// =============================================================

#[test]
fn go_circular_deps_nodes() {
    assert_nodes("go-circular-deps");
}

#[test]
fn go_circular_deps_node_attributes() {
    assert_node_attributes("go-circular-deps");
}

#[test]
fn go_circular_deps_edges() {
    assert_edges("go-circular-deps");
}

#[test]
fn go_circular_deps_topo_error() {
    let gt = load_ground_truth("go-circular-deps");
    let spec = gt.topo_sort.expect("go-circular-deps 应有 topo_sort 字段");
    assert!(spec.expect_error, "应期望拓扑排序失败");

    let graph = build("go-circular-deps");
    assert!(
        topological_sort(&graph).is_err(),
        "包级环拓扑排序应返回错误"
    );

    let cycles = detect_cycles(&graph);
    assert!(!cycles.is_empty(), "应检测到至少一个环");

    // 精确断言：cycle_contains 的全部成员必须落在「同一个」环（SCC）内。
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
fn go_circular_deps_shared_not_in_cycle() {
    let graph = build("go-circular-deps");
    let cycles = detect_cycles(&graph);
    let shared_in_cycle = cycles
        .iter()
        .any(|c| c.iter().any(|id| id.as_str().contains("shared")));
    assert!(
        !shared_in_cycle,
        "shared 包不应出现在任何环中，实际环: {cycles:?}"
    );
}

#[test]
fn go_circular_deps_migration_sequence_has_cycles() {
    let graph = build("go-circular-deps");
    let seq = migration_sequence(&graph);
    assert!(seq.has_cycles(), "circular 应标记有环");
    assert!(!seq.order.is_empty(), "有环时仍应生成尽力排序");
}

// =============================================================
// go-pkg-deps（多文件同包凝聚 + 跨包调用 + 文件过滤）
// =============================================================

#[test]
fn go_pkg_deps_nodes() {
    assert_nodes("go-pkg-deps");
}

#[test]
fn go_pkg_deps_node_attributes() {
    assert_node_attributes("go-pkg-deps");
}

#[test]
fn go_pkg_deps_edges() {
    assert_edges("go-pkg-deps");
}

#[test]
fn go_pkg_deps_topo() {
    assert_topo_constraints("go-pkg-deps");
}

#[test]
fn go_pkg_deps_file_filtering() {
    // 过滤回归：_test.go 与平台后缀文件完全排除（无任何节点）；//go:build ignore → 孤立 File 节点。
    let graph = build("go-pkg-deps");
    let ids: HashSet<&str> = graph.nodes().map(|n| n.id.as_str()).collect();

    assert!(
        !ids.iter().any(|id| id.contains("store_test.go")),
        "_test.go 应被 can_handle 完全排除（含 File 节点），实际: {ids:?}"
    );
    assert!(
        !ids.iter().any(|id| id.contains("helper_windows.go")),
        "平台后缀 _windows.go 应被完全排除，实际: {ids:?}"
    );
    // tagged.go 被 //go:build ignore 门控 → 仅保留 File 节点，无符号节点。
    assert!(
        ids.contains("file:store/tagged.go"),
        "//go:build ignore 文件应保留孤立 File 节点"
    );
    assert!(
        !ids.iter().any(|id| id.contains("tagged.go:")),
        "//go:build ignore 文件应跳过符号提取（无 tagged.go 内符号节点）"
    );
}

#[test]
fn go_pkg_deps_cross_package_call_resolves_to_representative() {
    // 代表文件（query.go 字典序第一）内的导出符号可被跨包 store.Query 精确解析。
    let graph = build("go-pkg-deps");
    let has = graph.edges().any(|e| {
        e.edge_type.to_string() == "calls"
            && e.source.as_str() == "file:main.go"
            && e.target.as_str() == "function:store/query.go:Query"
    });
    assert!(has, "跨包 store.Query 应连到代表文件 store/query.go:Query");
    // import 边也应指向代表文件。
    let has_import = graph.edges().any(|e| {
        e.edge_type.to_string() == "imports"
            && e.source.as_str() == "file:main.go"
            && e.target.as_str() == "file:store/query.go"
    });
    assert!(has_import, "跨包 import 应解析到 store 包代表文件 query.go");
}

/// GO-09：同 package 多文件应凝聚到同一 `DecompUnit`（预算充裕时）。
#[test]
fn go_pkg_deps_same_package_files_in_one_decomp_unit() {
    let graph = build("go-pkg-deps");
    let seq = migration_sequence(&graph);

    // self_sizes/footprints 按文件 NodeId 赋一致值（PER=10），绕开真实 token≈bytes/4 换算。
    let per = 10usize;
    let sizes: HashMap<NodeId, usize> = graph
        .nodes()
        .filter(|n| n.node_type == NodeType::File)
        .map(|n| (n.id.clone(), per))
        .collect();

    // 预算从 store 目录实际 File 节点数**自适应**推导（不写死 35、不依赖 tagged.go 具体计数）：
    // 取 = store 目录全部 File 节点 footprint 之和 + PER/2，恰好容 store 全并（sum ≤ budget）、
    // 但容不下再并入任一外部目录文件（sum + PER > budget）。这样即使将来 store 增删文件、或
    // 改变 //go:build ignore 文件是否计为 File 节点，"同包凝聚 + 跨目录边界"两个不变量仍成立。
    let store_footprint: usize = sizes
        .keys()
        .filter(|id| id.as_str().starts_with("file:store/"))
        .count()
        * per;
    let budget = store_footprint + per / 2;
    let plan = plan_decomposition(&graph, &seq, &sizes, &sizes, budget);

    // 找到包含 store/query.go 的单元，断言 store 包全部实体文件同在其中。
    let unit = plan
        .units
        .iter()
        .find(|u| u.members.iter().any(|m| m == "file:store/query.go"))
        .expect("应有包含 store/query.go 的 DecompUnit");
    for f in [
        "file:store/query.go",
        "file:store/store.go",
        "file:store/tagged.go",
    ] {
        assert!(
            unit.members.iter().any(|m| m == f),
            "store 包文件 {f} 应与同包文件凝聚到同一 DecompUnit，实际 members: {:?}",
            unit.members
        );
    }
    // main.go 属于不同目录（root），不应混入 store 单元——预算按上式恰好挡住跨目录并入，
    // 使断言非"预算无穷大 → 全并一单元"的平凡通过。
    assert!(
        !unit.members.iter().any(|m| m == "file:main.go"),
        "根目录 main.go 不应并入 store 包单元: {:?}",
        unit.members
    );
}
