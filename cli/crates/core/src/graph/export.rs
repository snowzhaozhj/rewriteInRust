//! 图导出：JSON / DOT / Mermaid 格式。
//!
//! 对应 `docs/design/06-plugin-structure.md` 的 `graph export` 命令。

use serde_json::{json, Value};

use super::SourceGraph;

/// 导出完整图为 JSON 对象（nodes + edges）。
///
/// 返回一个 JSON 对象，包含 `nodes` 和 `edges` 数组。
/// 每个 node 包含 id、node_type、name、file_path 等属性；
/// 每个 edge 包含 source、target、edge_type 等属性。
pub fn export_json(graph: &SourceGraph) -> Value {
    let nodes: Vec<Value> = graph
        .nodes()
        .map(|n| {
            json!({
                "id": n.id.as_str(),
                "node_type": n.node_type.to_string(),
                "name": n.name,
                "file_path": n.file_path,
                "is_exported": n.is_exported,
            })
        })
        .collect();

    let edges: Vec<Value> = graph
        .edges()
        .map(|e| {
            json!({
                "source": e.source.as_str(),
                "target": e.target.as_str(),
                "edge_type": e.edge_type.to_string(),
                "provenance": e.provenance.to_string(),
                "weight": e.weight,
            })
        })
        .collect();

    json!({
        "nodes": nodes,
        "edges": edges,
    })
}

/// 转义 DOT 格式双引号字符串中的 `\` 和 `"`。
fn escape_dot_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 导出图为 Graphviz DOT 格式。
///
/// 格式：`digraph { "file:a.ts" -> "file:b.ts" [label="imports"]; }`
pub fn export_dot(graph: &SourceGraph) -> String {
    let mut lines = Vec::new();
    lines.push("digraph {".to_string());

    // 输出所有节点（带标签）
    for node in graph.nodes() {
        let id = escape_dot_string(node.id.as_str());
        let label = escape_dot_string(&node.name);
        lines.push(format!("  \"{id}\" [label=\"{label}\"];"));
    }

    // 输出所有边（带 label）
    for edge in graph.edges() {
        let src = escape_dot_string(edge.source.as_str());
        let tgt = escape_dot_string(edge.target.as_str());
        let label = escape_dot_string(&edge.edge_type.to_string());
        lines.push(format!("  \"{src}\" -> \"{tgt}\" [label=\"{label}\"];"));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// 将字符串转换为 Mermaid 安全的节点 ID。
///
/// 仅放行 `[a-zA-Z0-9_]`，其余全部替换为 `_`。
fn mermaid_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 导出图为 Mermaid flowchart 格式。
///
/// 格式：`flowchart TD\n  file_a_ts["file:a.ts"] -->|imports| file_b_ts["file:b.ts"]`
pub fn export_mermaid(graph: &SourceGraph) -> String {
    let mut lines = Vec::new();
    lines.push("flowchart TD".to_string());

    // 输出所有节点声明
    for node in graph.nodes() {
        let id = mermaid_id(node.id.as_str());
        let label = node.id.as_str();
        lines.push(format!("  {id}[\"{label}\"]"));
    }

    // 输出所有边
    for edge in graph.edges() {
        let src = mermaid_id(edge.source.as_str());
        let tgt = mermaid_id(edge.target.as_str());
        let label = edge.edge_type.to_string();
        lines.push(format!("  {src} -->|{label}| {tgt}"));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build::build_graph_ts;
    use crate::graph::SourceGraph;
    use crate::types::common::NodeId;
    use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    /// 构造一个简单的两节点图用于测试。
    fn simple_graph() -> SourceGraph {
        let mut g = SourceGraph::new();
        g.add_node(SourceNode::new(
            NodeId::file("a.ts"),
            NodeType::File,
            "a.ts".to_string(),
            "a.ts".to_string(),
        ));
        g.add_node(SourceNode::new(
            NodeId::file("b.ts"),
            NodeType::File,
            "b.ts".to_string(),
            "b.ts".to_string(),
        ));
        g.add_edge(Dependency::new(
            NodeId::file("a.ts"),
            NodeId::file("b.ts"),
            EdgeType::Imports,
        ));
        g
    }

    #[test]
    fn export_json_contains_nodes_and_edges() {
        let g = simple_graph();
        let json = export_json(&g);

        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2);

        let edges = json["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["edge_type"], "imports");
    }

    #[test]
    fn export_dot_format() {
        let g = simple_graph();
        let dot = export_dot(&g);

        assert!(dot.starts_with("digraph {"), "应以 digraph {{ 开头");
        assert!(dot.ends_with('}'), "应以 }} 结尾");
        assert!(
            dot.contains("\"file:a.ts\" -> \"file:b.ts\" [label=\"imports\"]"),
            "应含边声明: {dot}"
        );
    }

    #[test]
    fn export_mermaid_format() {
        let g = simple_graph();
        let mermaid = export_mermaid(&g);

        assert!(
            mermaid.starts_with("flowchart TD"),
            "应以 flowchart TD 开头"
        );
        // 节点 ID 应转义特殊字符
        assert!(
            mermaid.contains("file_a_ts[\"file:a.ts\"]"),
            "应含节点声明: {mermaid}"
        );
        assert!(
            mermaid.contains("file_a_ts -->|imports| file_b_ts"),
            "应含边声明: {mermaid}"
        );
    }

    #[test]
    fn export_mermaid_id_escaping() {
        assert_eq!(mermaid_id("file:src/utils.ts"), "file_src_utils_ts");
        assert_eq!(mermaid_id("function:a.ts:foo"), "function_a_ts_foo");
    }

    #[test]
    fn export_empty_graph() {
        let g = SourceGraph::new();

        let json = export_json(&g);
        assert_eq!(json["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(json["edges"].as_array().unwrap().len(), 0);

        let dot = export_dot(&g);
        assert!(dot.contains("digraph {"));
        assert!(dot.ends_with('}'));

        let mermaid = export_mermaid(&g);
        assert!(mermaid.starts_with("flowchart TD"));
    }

    #[test]
    fn export_json_fixture_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let json = export_json(&graph);

        let nodes = json["nodes"].as_array().unwrap();
        let edges = json["edges"].as_array().unwrap();
        assert!(nodes.len() >= 3, "linear-deps 至少 3 个节点");
        assert!(!edges.is_empty(), "linear-deps 应有边");
    }

    #[test]
    fn export_dot_fixture_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let dot = export_dot(&graph);

        assert!(dot.starts_with("digraph {"));
        assert!(dot.contains("[label="));
        assert!(dot.contains("->"));
    }

    #[test]
    fn export_mermaid_fixture_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let mermaid = export_mermaid(&graph);

        assert!(mermaid.starts_with("flowchart TD"));
        assert!(mermaid.contains("-->|"));
    }
}
