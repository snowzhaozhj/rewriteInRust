use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::graph::topo::{detect_cycles, migration_sequence, topological_sort};
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::EdgeSubKind;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(3).unwrap().join("fixtures")
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // 部分字段仅为完整反序列化 ground-truth.json，未在断言中读取
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
    #[serde(default)]
    expectations: Option<Expectations>,
}

#[derive(Debug, Deserialize)]
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
#[allow(dead_code)] // sub_kind 仅为完整反序列化，未在断言中读取
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

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // 字段仅为完整反序列化 expectations 区块，未在断言中读取
struct Expectations {
    #[serde(default)]
    empty_file: Option<String>,
    #[serde(default)]
    syntax_error: Option<String>,
    #[serde(default)]
    pure_types: Option<String>,
}

fn load_ground_truth(fixture: &str) -> GroundTruth {
    let path = fixtures_dir().join(fixture).join("ground-truth.json");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 {fixture}/ground-truth.json 失败: {e}"));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("解析 {fixture}/ground-truth.json 失败: {e}"))
}

// =============================================================
// linear-deps
// =============================================================

#[test]
fn ground_truth_linear_deps_nodes() {
    let gt = load_ground_truth("linear-deps");
    let root = fixtures_dir().join("linear-deps");
    let graph = build_graph_ts(&root).unwrap();

    let all_ids: HashSet<String> = graph.nodes().map(|n| n.id.as_str().to_owned()).collect();

    let mut missing = Vec::new();
    for node_spec in &gt.nodes {
        if !all_ids.contains(&node_spec.id) {
            missing.push(format!("{} ({})", node_spec.id, node_spec.node_type));
        }
    }
    assert!(
        missing.is_empty(),
        "linear-deps 缺失节点:\n  {}\n实际节点:\n  {}",
        missing.join("\n  "),
        all_ids.iter().cloned().collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn ground_truth_linear_deps_node_attributes() {
    let gt = load_ground_truth("linear-deps");
    let root = fixtures_dir().join("linear-deps");
    let graph = build_graph_ts(&root).unwrap();

    for node_spec in &gt.nodes {
        if node_spec.note.is_some() {
            continue;
        }
        let idx = graph
            .node_index(&NodeId::new(&node_spec.id))
            .unwrap_or_else(|| panic!("节点 {} 不存在", node_spec.id));
        let node = graph.node(idx).unwrap();

        if let Some(expected_exported) = node_spec.is_exported {
            assert_eq!(
                node.is_exported, expected_exported,
                "节点 {} is_exported 不匹配: 期望 {}, 实际 {}",
                node_spec.id, expected_exported, node.is_exported
            );
        }
        if let Some(expected_async) = node_spec.is_async {
            assert_eq!(
                node.is_async, expected_async,
                "节点 {} is_async 不匹配: 期望 {}, 实际 {}",
                node_spec.id, expected_async, node.is_async
            );
        }
    }
}

#[test]
fn ground_truth_linear_deps_edges() {
    let gt = load_ground_truth("linear-deps");
    let root = fixtures_dir().join("linear-deps");
    let graph = build_graph_ts(&root).unwrap();

    let actual_edges: HashSet<(String, String, String)> = graph
        .edges()
        .map(|e| {
            (
                e.source.as_str().to_owned(),
                e.target.as_str().to_owned(),
                e.edge_type.to_string(),
            )
        })
        .collect();

    let mut missing = Vec::new();
    for edge_spec in &gt.edges {
        // 跳过已知限制（标注了 note 的边）
        if edge_spec.note.is_some() {
            continue;
        }
        let key = (
            edge_spec.source.clone(),
            edge_spec.target.clone(),
            edge_spec.edge_type.clone(),
        );
        if !actual_edges.contains(&key) {
            missing.push(format!(
                "{} --[{}]--> {}",
                edge_spec.source, edge_spec.edge_type, edge_spec.target
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "linear-deps 缺失边:\n  {}\n实际边:\n  {}",
        missing.join("\n  "),
        actual_edges
            .iter()
            .map(|(s, t, e)| format!("{s} --[{e}]--> {t}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn ground_truth_linear_deps_topo() {
    let gt = load_ground_truth("linear-deps");
    let root = fixtures_dir().join("linear-deps");
    let graph = build_graph_ts(&root).unwrap();
    let order = topological_sort(&graph).unwrap();

    for (before, after) in &gt.topo_order_constraints {
        let pos_before = order
            .iter()
            .position(|id| id.as_str() == before)
            .unwrap_or_else(|| panic!("拓扑排序中找不到 {before}"));
        let pos_after = order
            .iter()
            .position(|id| id.as_str() == after)
            .unwrap_or_else(|| panic!("拓扑排序中找不到 {after}"));
        assert!(
            pos_before < pos_after,
            "{before} 应排在 {after} 前，实际: {order:?}"
        );
    }
}

// =============================================================
// diamond-deps
// =============================================================

#[test]
fn ground_truth_diamond_deps_nodes() {
    let gt = load_ground_truth("diamond-deps");
    let root = fixtures_dir().join("diamond-deps");
    let graph = build_graph_ts(&root).unwrap();

    let all_ids: HashSet<String> = graph.nodes().map(|n| n.id.as_str().to_owned()).collect();

    let mut missing = Vec::new();
    for node_spec in &gt.nodes {
        if !all_ids.contains(&node_spec.id) {
            missing.push(format!("{} ({})", node_spec.id, node_spec.node_type));
        }
    }
    assert!(
        missing.is_empty(),
        "diamond-deps 缺失节点:\n  {}\n实际节点:\n  {}",
        missing.join("\n  "),
        all_ids.iter().cloned().collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn ground_truth_diamond_deps_edges() {
    let gt = load_ground_truth("diamond-deps");
    let root = fixtures_dir().join("diamond-deps");
    let graph = build_graph_ts(&root).unwrap();

    let actual_edges: HashSet<(String, String, String)> = graph
        .edges()
        .map(|e| {
            (
                e.source.as_str().to_owned(),
                e.target.as_str().to_owned(),
                e.edge_type.to_string(),
            )
        })
        .collect();

    let mut missing = Vec::new();
    for edge_spec in &gt.edges {
        if edge_spec.note.is_some() {
            continue;
        }
        let key = (
            edge_spec.source.clone(),
            edge_spec.target.clone(),
            edge_spec.edge_type.clone(),
        );
        if !actual_edges.contains(&key) {
            missing.push(format!(
                "{} --[{}]--> {}",
                edge_spec.source, edge_spec.edge_type, edge_spec.target
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "diamond-deps 缺失边:\n  {}\n实际边:\n  {}",
        missing.join("\n  "),
        actual_edges
            .iter()
            .map(|(s, t, e)| format!("{s} --[{e}]--> {t}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn ground_truth_diamond_deps_topo() {
    let gt = load_ground_truth("diamond-deps");
    let root = fixtures_dir().join("diamond-deps");
    let graph = build_graph_ts(&root).unwrap();
    let order = topological_sort(&graph).unwrap();

    for (before, after) in &gt.topo_order_constraints {
        let pos_before = order
            .iter()
            .position(|id| id.as_str() == before)
            .unwrap_or_else(|| panic!("拓扑排序中找不到 {before}"));
        let pos_after = order
            .iter()
            .position(|id| id.as_str() == after)
            .unwrap_or_else(|| panic!("拓扑排序中找不到 {after}"));
        assert!(
            pos_before < pos_after,
            "{before} 应排在 {after} 前，实际: {order:?}"
        );
    }
}

#[test]
fn ground_truth_diamond_deps_extends() {
    let root = fixtures_dir().join("diamond-deps");
    let graph = build_graph_ts(&root).unwrap();

    let extends_edges: Vec<_> = graph
        .edges()
        .filter(|e| e.edge_type.to_string() == "extends")
        .collect();

    assert!(
        !extends_edges.is_empty(),
        "diamond-deps 应有 extends 边（AuthService implements Serializable）"
    );

    let has_implements = extends_edges.iter().any(|e| {
        e.source.as_str().contains("AuthService")
            && e.target.as_str().contains("Serializable")
            && e.sub_kind == Some(EdgeSubKind::Implements)
    });
    assert!(
        has_implements,
        "AuthService -> Serializable extends 边应有 sub_kind=implements，实际: {:?}",
        extends_edges
            .iter()
            .map(|e| format!(
                "{} -> {} (sub_kind={:?})",
                e.source.as_str(),
                e.target.as_str(),
                e.sub_kind
            ))
            .collect::<Vec<_>>()
    );
}

// =============================================================
// circular-deps
// =============================================================

#[test]
fn ground_truth_circular_deps_nodes() {
    let gt = load_ground_truth("circular-deps");
    let root = fixtures_dir().join("circular-deps");
    let graph = build_graph_ts(&root).unwrap();

    let all_ids: HashSet<String> = graph.nodes().map(|n| n.id.as_str().to_owned()).collect();

    let mut missing = Vec::new();
    for node_spec in &gt.nodes {
        if !all_ids.contains(&node_spec.id) {
            missing.push(format!("{} ({})", node_spec.id, node_spec.node_type));
        }
    }
    assert!(
        missing.is_empty(),
        "circular-deps 缺失节点:\n  {}\n实际节点:\n  {}",
        missing.join("\n  "),
        all_ids.iter().cloned().collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn ground_truth_circular_deps_edges() {
    let gt = load_ground_truth("circular-deps");
    let root = fixtures_dir().join("circular-deps");
    let graph = build_graph_ts(&root).unwrap();

    let actual_edges: HashSet<(String, String, String)> = graph
        .edges()
        .map(|e| {
            (
                e.source.as_str().to_owned(),
                e.target.as_str().to_owned(),
                e.edge_type.to_string(),
            )
        })
        .collect();

    let mut missing = Vec::new();
    for edge_spec in &gt.edges {
        if edge_spec.note.is_some() {
            continue;
        }
        let key = (
            edge_spec.source.clone(),
            edge_spec.target.clone(),
            edge_spec.edge_type.clone(),
        );
        if !actual_edges.contains(&key) {
            missing.push(format!(
                "{} --[{}]--> {}",
                edge_spec.source, edge_spec.edge_type, edge_spec.target
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "circular-deps 缺失边:\n  {}\n实际边:\n  {}",
        missing.join("\n  "),
        actual_edges
            .iter()
            .map(|(s, t, e)| format!("{s} --[{e}]--> {t}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn ground_truth_circular_deps_topo_error() {
    let gt = load_ground_truth("circular-deps");
    let topo_spec = gt.topo_sort.expect("circular-deps 应有 topo_sort 字段");
    assert!(topo_spec.expect_error, "circular-deps 应期望拓扑排序失败");

    let root = fixtures_dir().join("circular-deps");
    let graph = build_graph_ts(&root).unwrap();

    let result = topological_sort(&graph);
    assert!(result.is_err(), "circular-deps 拓扑排序应返回错误");

    let cycles = detect_cycles(&graph);
    assert!(!cycles.is_empty(), "应检测到至少一个环");

    for expected_member in &topo_spec.cycle_contains {
        let found = cycles
            .iter()
            .any(|cycle| cycle.iter().any(|id| id.as_str().contains(expected_member)));
        assert!(found, "环中应包含 {expected_member}，实际环: {cycles:?}");
    }
}

#[test]
fn ground_truth_circular_deps_shared_not_in_cycle() {
    let root = fixtures_dir().join("circular-deps");
    let graph = build_graph_ts(&root).unwrap();

    let cycles = detect_cycles(&graph);
    let shared_in_cycle = cycles
        .iter()
        .any(|cycle| cycle.iter().any(|id| id.as_str().contains("shared")));
    assert!(
        !shared_in_cycle,
        "shared.ts 不应出现在任何环中，实际环: {cycles:?}"
    );
}

// =============================================================
// edge-cases
// =============================================================

#[test]
fn ground_truth_edge_cases_build_succeeds() {
    let root = fixtures_dir().join("edge-cases");
    let graph = build_graph_ts(&root).unwrap();
    assert!(graph.node_count() > 0, "edge-cases 图应至少有一个节点");
}

#[test]
fn ground_truth_edge_cases_empty_file() {
    let root = fixtures_dir().join("edge-cases");
    let graph = build_graph_ts(&root).unwrap();

    let empty_file = graph.node_index(&NodeId::new("file:src/empty.ts"));
    assert!(empty_file.is_some(), "应有 file:src/empty.ts 节点");

    let children: Vec<_> = graph
        .nodes()
        .filter(|n| n.file_path == "src/empty.ts" && n.id.as_str() != "file:src/empty.ts")
        .collect();
    assert!(
        children.is_empty(),
        "empty.ts 不应有子节点，实际: {:?}",
        children.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn ground_truth_edge_cases_syntax_error() {
    let root = fixtures_dir().join("edge-cases");
    let graph = build_graph_ts(&root).unwrap();

    let syntax_file = graph.node_index(&NodeId::new("file:src/syntax-error.ts"));
    assert!(
        syntax_file.is_some(),
        "应有 file:src/syntax-error.ts 节点（tree-sitter 容错解析）"
    );
}

#[test]
fn ground_truth_edge_cases_pure_types() {
    let root = fixtures_dir().join("edge-cases");
    let graph = build_graph_ts(&root).unwrap();

    let has_config = graph
        .node_index(&NodeId::new("interface:src/pure-types.ts:Config"))
        .is_some();
    let has_log_level = graph
        .node_index(&NodeId::new("enum:src/pure-types.ts:LogLevel"))
        .is_some();

    assert!(has_config, "应有 interface:src/pure-types.ts:Config 节点");
    assert!(has_log_level, "应有 enum:src/pure-types.ts:LogLevel 节点");

    let calls_from_pure: Vec<_> = graph
        .edges()
        .filter(|e| e.source.as_str().contains("pure-types") && e.edge_type.to_string() == "calls")
        .collect();
    assert!(
        calls_from_pure.is_empty(),
        "pure-types.ts 不应有 calls 边: {:?}",
        calls_from_pure
            .iter()
            .map(|e| format!("{} -> {}", e.source.as_str(), e.target.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ground_truth_edge_cases_no_topo_constraints() {
    let root = fixtures_dir().join("edge-cases");
    let graph = build_graph_ts(&root).unwrap();

    let result = topological_sort(&graph);
    assert!(
        result.is_ok(),
        "edge-cases 无依赖，拓扑排序应成功: {:?}",
        result.err()
    );
}

// =============================================================
// migration sequence 综合验证
// =============================================================

#[test]
fn ground_truth_linear_deps_migration_sequence() {
    let root = fixtures_dir().join("linear-deps");
    let graph = build_graph_ts(&root).unwrap();
    let seq = migration_sequence(&graph);

    assert!(!seq.has_cycles(), "linear-deps 不应有环");
    assert!(!seq.order.is_empty(), "迁移序列不应为空");
    assert!(!seq.scc_groups.is_empty(), "应有至少一个 SCC 迁移单位");

    // sprint 1（首并行层）应包含叶节点 utils.ts。
    let sprint1_members: Vec<&str> = seq
        .scc_groups
        .iter()
        .filter(|g| g.sprint == 1)
        .flat_map(|g| g.members.iter().map(|id| id.as_str()))
        .collect();
    let has_leaf = sprint1_members.iter().any(|s| s.contains("utils.ts"));
    assert!(
        has_leaf,
        "sprint 1（首并行层）应包含叶节点 utils.ts: {sprint1_members:?}"
    );
}

#[test]
fn ground_truth_circular_deps_migration_sequence() {
    let root = fixtures_dir().join("circular-deps");
    let graph = build_graph_ts(&root).unwrap();
    let seq = migration_sequence(&graph);

    assert!(seq.has_cycles(), "circular-deps 应标记有环");
    assert!(!seq.order.is_empty(), "有环时仍应生成尽力排序");
}
