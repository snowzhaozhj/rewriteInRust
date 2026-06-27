//! 图查询：neighbors / paths / subgraph / stats。

use std::collections::{BTreeMap, BTreeSet};

use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::types::common::NodeId;
use crate::types::graph::{Dependency, EdgeType, NodeType, SourceNode};

use super::SourceGraph;

/// 按节点类型分组的统计。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: BTreeMap<String, usize>,
    pub edges_by_type: BTreeMap<String, usize>,
}

impl SourceGraph {
    /// 获取指定节点的邻居（可按方向过滤）。
    pub fn neighbors(&self, id: &NodeId, direction: Direction) -> Vec<&SourceNode> {
        let idx = match self.node_index(id) {
            Some(i) => i,
            None => return Vec::new(),
        };
        self.graph
            .neighbors_directed(idx, direction)
            .filter_map(|ni| self.graph.node_weight(ni))
            .collect()
    }

    /// 获取指定节点的所有出边邻居。
    pub fn outgoing(&self, id: &NodeId) -> Vec<&SourceNode> {
        self.neighbors(id, Direction::Outgoing)
    }

    /// 获取指定节点的所有入边邻居。
    pub fn incoming(&self, id: &NodeId) -> Vec<&SourceNode> {
        self.neighbors(id, Direction::Incoming)
    }

    /// 获取指定节点的出边（含边类型信息）。
    pub fn outgoing_edges(&self, id: &NodeId) -> Vec<(&SourceNode, EdgeType)> {
        let idx = match self.node_index(id) {
            Some(i) => i,
            None => return Vec::new(),
        };
        self.graph
            .edges_directed(idx, Direction::Outgoing)
            .filter_map(|e| {
                let target = self.graph.node_weight(e.target())?;
                Some((target, e.weight().edge_type))
            })
            .collect()
    }

    /// 获取指定节点的出边（含完整 `Dependency` 边元数据，如 `used_symbols`）。
    ///
    /// 与 `outgoing_edges` 的区别：后者只暴露 `(节点, EdgeType)`、丢弃整条边。
    /// footprint 估算与 `deps-of` 按符号裁剪需读 `used_symbols`，故用此 API（M3-DEC-01）。
    pub fn outgoing_edges_full(&self, id: &NodeId) -> Vec<(&SourceNode, &Dependency)> {
        let idx = match self.node_index(id) {
            Some(i) => i,
            None => return Vec::new(),
        };
        self.graph
            .edges_directed(idx, Direction::Outgoing)
            .filter_map(|e| {
                let target = self.graph.node_weight(e.target())?;
                Some((target, e.weight()))
            })
            .collect()
    }

    /// 获取指定节点的入边（含边类型信息）。
    pub fn incoming_edges(&self, id: &NodeId) -> Vec<(&SourceNode, EdgeType)> {
        let idx = match self.node_index(id) {
            Some(i) => i,
            None => return Vec::new(),
        };
        self.graph
            .edges_directed(idx, Direction::Incoming)
            .filter_map(|e| {
                let source = self.graph.node_weight(e.source())?;
                Some((source, e.weight().edge_type))
            })
            .collect()
    }

    /// 聚合本文件对各依赖文件实际用到的符号（M3-DEC-01）。
    ///
    /// 返回 dep 文件路径 → `Some(被用具名符号集)`（可裁剪）| `None`（use-all，不可裁剪）。
    /// `None` 为吸收态：同一 dep 多条边任一为 use-all 即整体 use-all。供 `deps-of`
    /// 按符号裁剪与 footprint 估算共用。
    pub fn imported_symbols_by_dep(
        &self,
        file: &NodeId,
    ) -> BTreeMap<String, Option<BTreeSet<String>>> {
        let mut map: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
        for (node, dep) in self.outgoing_edges_full(file) {
            if dep.edge_type != EdgeType::Imports || node.node_type != NodeType::File {
                continue;
            }
            let path = node.file_path.clone();
            match &dep.used_symbols {
                None => {
                    map.insert(path, None);
                }
                Some(syms) => {
                    if let Some(set) = map.entry(path).or_insert_with(|| Some(BTreeSet::new())) {
                        set.extend(syms.iter().cloned());
                    }
                }
            }
        }
        map
    }

    /// footprint 的"依赖签名"部分（token≈bytes/4，M3-DEC-01）：1-hop 依赖中
    /// target 实际用到的导出符号签名规模之和。
    ///
    /// 自身源码规模由调用方（持有源文件内容）相加得完整 footprint；本函数只算图能确定的部分。
    pub fn dependency_signature_tokens(&self, file: &NodeId) -> usize {
        let used = self.imported_symbols_by_dep(file);
        let mut total = 0usize;
        for (path, usage) in &used {
            for n in self.symbols_in_file(path) {
                if !n.is_exported {
                    continue;
                }
                if let Some(set) = usage {
                    if !set.contains(&n.name) {
                        continue;
                    }
                }
                let sig = n.signature.as_deref().unwrap_or(&n.name);
                total += sig.len().div_ceil(4);
            }
        }
        total
    }

    /// 按节点类型过滤所有节点。
    pub fn nodes_by_type(&self, node_type: NodeType) -> Vec<&SourceNode> {
        self.nodes().filter(|n| n.node_type == node_type).collect()
    }

    /// 按文件路径获取该文件下的所有符号节点。
    pub fn symbols_in_file(&self, file_path: &str) -> Vec<&SourceNode> {
        self.nodes()
            .filter(|n| n.file_path == file_path && n.node_type != NodeType::File)
            .collect()
    }

    /// 计算图统计信息。
    pub fn stats(&self) -> GraphStats {
        let mut nodes_by_type: BTreeMap<String, usize> = BTreeMap::new();
        let mut edges_by_type: BTreeMap<String, usize> = BTreeMap::new();

        for node in self.nodes() {
            *nodes_by_type.entry(node.node_type.to_string()).or_default() += 1;
        }
        for edge in self.edges() {
            *edges_by_type.entry(edge.edge_type.to_string()).or_default() += 1;
        }

        GraphStats {
            total_nodes: self.node_count(),
            total_edges: self.edge_count(),
            nodes_by_type,
            edges_by_type,
        }
    }

    /// 提取仅包含指定节点及其关联边的子图。
    pub fn subgraph(&self, node_ids: &[NodeId]) -> SourceGraph {
        let mut sub = SourceGraph::new();
        let indices: Vec<NodeIndex> = node_ids
            .iter()
            .filter_map(|id| self.node_index(id))
            .collect();

        for &idx in &indices {
            if let Some(node) = self.graph.node_weight(idx) {
                sub.add_node(node.clone());
            }
        }

        for &idx in &indices {
            for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                if indices.contains(&edge.target()) {
                    sub.add_edge(edge.weight().clone());
                }
            }
        }

        sub
    }

    /// 获取所有文件节点。
    pub fn file_nodes(&self) -> Vec<&SourceNode> {
        self.nodes_by_type(NodeType::File)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build::build_graph_ts;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    #[test]
    fn query_neighbors() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let files = graph.file_nodes();
        assert!(files.len() >= 3, "should have at least 3 file nodes");
    }

    #[test]
    fn query_stats() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let stats = graph.stats();
        assert!(stats.total_nodes > 0);
        assert!(stats.total_edges > 0);
        assert!(stats.nodes_by_type.contains_key("file"));
    }

    #[test]
    fn query_nodes_by_type() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let functions = graph.nodes_by_type(NodeType::Function);
        assert!(!functions.is_empty(), "should have function nodes");
    }

    #[test]
    fn query_subgraph() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let file_ids: Vec<NodeId> = graph.file_nodes().iter().map(|n| n.id.clone()).collect();
        let sub = graph.subgraph(&file_ids);
        assert_eq!(sub.node_count(), graph.file_nodes().len());
    }

    #[test]
    fn query_nonexistent_node() {
        let graph = SourceGraph::new();
        let result = graph.outgoing(&NodeId::new("nonexistent"));
        assert!(result.is_empty());
    }

    #[test]
    fn imported_symbols_and_footprint() {
        let dir = std::env::temp_dir().join("rustmigrate_query_used_symbols");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("dep.ts"),
            "export function greet(name: string): string { return name; }\n\
             export function unused(): void {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("main.ts"),
            "import { greet } from './dep';\ngreet('x');\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let main = NodeId::file("main.ts");
        let by_dep = graph.imported_symbols_by_dep(&main);
        let toks = graph.dependency_signature_tokens(&main);
        let _ = std::fs::remove_dir_all(&dir);

        let mut expected = std::collections::BTreeSet::new();
        expected.insert("greet".to_string());
        assert_eq!(
            by_dep.get("dep.ts"),
            Some(&Some(expected)),
            "应只聚合被用具名符号 greet（不含 unused）"
        );
        assert!(toks > 0, "footprint 依赖签名应统计到 greet 的签名 token");
    }
}
