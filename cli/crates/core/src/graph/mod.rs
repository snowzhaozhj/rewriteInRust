//! 源码图引擎：构建、查询、拓扑排序、持久化。
//!
//! 核心数据结构 [`SourceGraph`] 封装 petgraph [`StableGraph`]，
//! 提供从 NodeId 到 NodeIndex 的快速映射。

pub mod build;
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
}

impl SourceGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            index: HashMap::new(),
        }
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
}

impl Default for SourceGraph {
    fn default() -> Self {
        Self::new()
    }
}
