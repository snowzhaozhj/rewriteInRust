//! 源码图引擎：构建、查询、拓扑排序、持久化。
//!
//! 核心数据结构 [`SourceGraph`] 封装 petgraph [`StableGraph`]，
//! 提供从 NodeId 到 NodeIndex 的快速映射。

pub mod build;
pub mod decompose;
pub mod export;
pub mod fingerprint;
pub mod persist;
pub mod query;
pub mod topo;

use std::collections::HashMap;

use petgraph::stable_graph::{NodeIndex, StableGraph};

use crate::types::common::NodeId;
use crate::types::graph::{Dependency, SourceNode};

/// 源码图：petgraph StableGraph + NodeId 索引。
#[derive(Debug, Clone)]
pub struct SourceGraph {
    pub(crate) graph: StableGraph<SourceNode, Dependency>,
    pub(crate) index: HashMap<NodeId, NodeIndex>,
    pub(crate) warnings: Vec<String>,
}

impl SourceGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            index: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// 构建过程中产生的警告（如文件解析失败被跳过）。
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// 添加节点，返回 NodeIndex。重复 NodeId 返回已有索引。
    pub fn add_node(&mut self, node: SourceNode) -> NodeIndex {
        if let Some(&idx) = self.index.get(&node.id) {
            return idx;
        }
        let id = node.id.clone();
        let idx = self.graph.add_node(node);
        self.index.insert(id, idx);
        idx
    }

    /// 添加边。source/target 必须已存在，否则返回 None。
    pub fn add_edge(&mut self, dep: Dependency) -> Option<petgraph::stable_graph::EdgeIndex> {
        let src = self.index.get(&dep.source)?;
        let tgt = self.index.get(&dep.target)?;
        Some(self.graph.add_edge(*src, *tgt, dep))
    }

    /// 按 NodeId 查找 NodeIndex。
    pub fn node_index(&self, id: &NodeId) -> Option<NodeIndex> {
        self.index.get(id).copied()
    }

    /// 按 NodeIndex 取节点引用。
    pub fn node(&self, idx: NodeIndex) -> Option<&SourceNode> {
        self.graph.node_weight(idx)
    }

    /// 节点数。
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// 边数。
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// 遍历所有节点。
    pub fn nodes(&self) -> impl Iterator<Item = &SourceNode> {
        self.graph.node_weights()
    }

    /// 遍历所有边。
    pub fn edges(&self) -> impl Iterator<Item = &Dependency> {
        self.graph.edge_weights()
    }

    /// 删除指定文件路径关联的所有节点及其边。
    ///
    /// 增量更新时用于清理 STRUCTURAL 变更文件的旧数据——
    /// 先删除旧节点+边，再重新解析写入新节点+边。
    pub fn remove_nodes_by_file(&mut self, file_path: &str) {
        let to_remove: Vec<(NodeId, NodeIndex)> = self
            .index
            .iter()
            .filter_map(|(id, &idx)| {
                let node = self.graph.node_weight(idx)?;
                if node.file_path == file_path {
                    Some((id.clone(), idx))
                } else {
                    None
                }
            })
            .collect();

        for (id, idx) in to_remove {
            self.graph.remove_node(idx);
            self.index.remove(&id);
        }
    }
}

impl Default for SourceGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::graph::{Dependency, EdgeType, NodeType};

    fn test_node(id: &str, name: &str) -> SourceNode {
        SourceNode::new(
            NodeId::new(id),
            NodeType::Function,
            name.to_string(),
            "test.ts".to_string(),
        )
    }

    #[test]
    fn add_node_dedup_returns_existing() {
        let mut g = SourceGraph::new();
        let idx1 = g.add_node(test_node("function:test.ts:foo", "first"));
        let idx2 = g.add_node(test_node("function:test.ts:foo", "second"));
        assert_eq!(idx1, idx2);
        assert_eq!(g.node(idx1).unwrap().name, "first");
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn add_edge_nonexistent_returns_none() {
        let mut g = SourceGraph::new();
        g.add_node(test_node("function:test.ts:foo", "foo"));
        let result = g.add_edge(Dependency::new(
            NodeId::new("function:test.ts:foo"),
            NodeId::new("function:test.ts:missing"),
            EdgeType::Calls,
        ));
        assert!(result.is_none());
        assert_eq!(g.edge_count(), 0);
    }
}
