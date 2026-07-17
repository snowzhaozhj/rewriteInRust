//! proptest 属性测试：图操作不变量。
//!
//! 用随机生成的 DAG / 有环图验证拓扑排序、环检测、迁移序列、并行分组的正确性。
//! 每项属性跑 1000 次 fuzz，任何一次 panic 即失败。

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;

use rustmigrate_core::graph::topo::{detect_cycles, migration_sequence, topological_sort};
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::{Dependency, EdgeType, NodeType, SourceNode};

// =========================================================================
// 辅助：随机图生成策略
// =========================================================================

/// 生成随机 DAG（无环有向图）。
///
/// 策略：节点编号 0..n，只允许从小编号指向大编号的边（天然 DAG）。
fn arb_dag() -> impl Strategy<Value = SourceGraph> {
    // 节点数 2~50，边数 0~200
    (2..=50usize, 0..=200usize)
        .prop_flat_map(|(n, max_edges)| {
            // 生成至多 max_edges 条边，每条边 (src, tgt) 满足 src < tgt
            let edge_strategy = prop::collection::vec((0..n, 0..n), 0..=max_edges);
            (Just(n), edge_strategy)
        })
        .prop_map(|(n, raw_edges)| {
            let mut graph = SourceGraph::new();

            // 添加 File 节点
            for i in 0..n {
                let id = NodeId::new(format!("file:{i}.ts"));
                let node =
                    SourceNode::new(id, NodeType::File, format!("{i}.ts"), format!("{i}.ts"));
                graph.add_node(node);
            }

            // 添加边（仅保留 src < tgt 的边，保证无环）
            for (src, tgt) in raw_edges {
                if src < tgt {
                    let dep = Dependency::new(
                        NodeId::new(format!("file:{src}.ts")),
                        NodeId::new(format!("file:{tgt}.ts")),
                        EdgeType::Imports,
                    );
                    graph.add_edge(dep);
                }
            }

            graph
        })
}

/// 生成随机有环图（至少包含一个环）。
///
/// 策略：先生成随机图，再强制插入一条从最大编号到最小编号的边（形成环）。
fn arb_cyclic_graph() -> impl Strategy<Value = SourceGraph> {
    // 节点数 2~50，额外随机边数 0~200
    (2..=50usize, 0..=200usize)
        .prop_flat_map(|(n, max_edges)| {
            let edge_strategy = prop::collection::vec((0..n, 0..n), 0..=max_edges);
            (Just(n), edge_strategy)
        })
        .prop_map(|(n, raw_edges)| {
            let mut graph = SourceGraph::new();

            // 添加 File 节点
            for i in 0..n {
                let id = NodeId::new(format!("file:{i}.ts"));
                let node =
                    SourceNode::new(id, NodeType::File, format!("{i}.ts"), format!("{i}.ts"));
                graph.add_node(node);
            }

            // 添加随机边（不限方向，可能已有环）
            for (src, tgt) in &raw_edges {
                if src != tgt {
                    let dep = Dependency::new(
                        NodeId::new(format!("file:{src}.ts")),
                        NodeId::new(format!("file:{tgt}.ts")),
                        EdgeType::Imports,
                    );
                    graph.add_edge(dep);
                }
            }

            // 强制插入一条环边：0 -> 1 -> ... -> n-1 -> 0
            // 先确保正向链路存在
            for i in 0..n - 1 {
                let dep = Dependency::new(
                    NodeId::new(format!("file:{i}.ts")),
                    NodeId::new(format!("file:{}.ts", i + 1)),
                    EdgeType::Imports,
                );
                graph.add_edge(dep);
            }
            // 再插入回边形成环
            let dep = Dependency::new(
                NodeId::new(format!("file:{}.ts", n - 1)),
                NodeId::new("file:0.ts"),
                EdgeType::Imports,
            );
            graph.add_edge(dep);

            graph
        })
}

/// 生成含**多个独立 SCC**的有环图（区别于 `arb_cyclic_graph` 全图一个大环）。
///
/// 策略：把节点切成 k 个大小为 2~3 的小簇，每簇内部成环（形成独立 SCC），
/// 再在**簇之间**只加「低编号簇 → 高编号簇」方向的边（保证缩点 DAG 无环、可分层）。
/// 用于验证「同 sprint 的不同 SCC 之间无依赖边」这一编排器原子调度依赖的不变量。
fn arb_multi_scc_graph() -> impl Strategy<Value = SourceGraph> {
    // 簇数 2~8，簇间边数 0~40
    (2..=8usize, 0..=40usize)
        .prop_flat_map(|(k, max_edges)| {
            // 每簇大小 2 或 3
            let sizes = prop::collection::vec(2..=3usize, k);
            let edges = prop::collection::vec((0..k, 0..k), 0..=max_edges);
            (Just(k), sizes, edges)
        })
        .prop_map(|(_k, sizes, cluster_edges)| {
            let mut graph = SourceGraph::new();

            // 为每簇分配连续节点编号；cluster_start[c] = 簇 c 首节点全局编号。
            let mut cluster_start = Vec::with_capacity(sizes.len());
            let mut next = 0usize;
            for &sz in &sizes {
                cluster_start.push(next);
                next += sz;
            }
            let total = next;

            for i in 0..total {
                let id = NodeId::new(format!("file:{i}.ts"));
                let node =
                    SourceNode::new(id, NodeType::File, format!("{i}.ts"), format!("{i}.ts"));
                graph.add_node(node);
            }

            let edge = |g: &mut SourceGraph, a: usize, b: usize| {
                g.add_edge(Dependency::new(
                    NodeId::new(format!("file:{a}.ts")),
                    NodeId::new(format!("file:{b}.ts")),
                    EdgeType::Imports,
                ));
            };

            // 簇内成环（大小 2 或 3 均形成真 SCC）。
            for (c, &sz) in sizes.iter().enumerate() {
                let start = cluster_start[c];
                for j in 0..sz {
                    edge(&mut graph, start + j, start + (j + 1) % sz);
                }
            }

            // 簇间边：仅低编号簇 → 高编号簇（缩点 DAG 无环），连各自首节点。
            for (src_c, tgt_c) in cluster_edges {
                if src_c < tgt_c {
                    edge(&mut graph, cluster_start[src_c], cluster_start[tgt_c]);
                }
            }

            graph
        })
}

// =========================================================================
// 属性测试
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// 拓扑排序一致性：对随机 DAG，topo_sort 结果中任意边 (u,v)，u 的位置在 v 之前。
    ///
    /// "u 在 v 之前"含义：u 是被依赖方（被 import 的），应先迁移，在结果中位置更靠前。
    /// 图中边 A->B 表示 A imports B（A 依赖 B），拓扑排序应让 B 排在 A 前面。
    #[test]
    fn proptest_topo_sort_consistency(graph in arb_dag()) {
        let order = topological_sort(&graph).expect("DAG 不应有环");

        // 构建 NodeId -> 位置映射
        let pos: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();

        // 对每条 Imports 边 (source imports target)，target 应排在 source 之前
        for edge in graph.edges() {
            if edge.edge_type == EdgeType::Imports {
                let src_pos = pos.get(edge.source.as_str());
                let tgt_pos = pos.get(edge.target.as_str());
                if let (Some(&sp), Some(&tp)) = (src_pos, tgt_pos) {
                    prop_assert!(
                        tp < sp,
                        "边 {} -> {}：被依赖方 {} (pos={}) 应排在依赖方 {} (pos={}) 前",
                        edge.source, edge.target,
                        edge.target, tp,
                        edge.source, sp,
                    );
                }
            }
        }
    }

    /// 环检测正确性（DAG）：对随机 DAG，detect_cycles 必返回空。
    #[test]
    fn proptest_detect_cycles_dag_returns_empty(graph in arb_dag()) {
        let cycles = detect_cycles(&graph);
        prop_assert!(
            cycles.is_empty(),
            "DAG 不应检测到环，实际: {:?}", cycles
        );
    }

    /// 环检测正确性（有环图）：对随机有环图，detect_cycles 必返回非空环路径。
    #[test]
    fn proptest_detect_cycles_cyclic_returns_nonempty(graph in arb_cyclic_graph()) {
        let cycles = detect_cycles(&graph);
        prop_assert!(
            !cycles.is_empty(),
            "有环图应检测到至少一个环"
        );
    }

    /// 迁移序列完整性：migration_sequence 的 order 包含图中所有 File 节点。
    #[test]
    fn proptest_migration_sequence_completeness(graph in arb_dag()) {
        let seq = migration_sequence(&graph);

        // 收集图中所有 File 节点 ID
        let file_ids: HashSet<&str> = graph
            .nodes()
            .filter(|n| n.node_type == NodeType::File)
            .map(|n| n.id.as_str())
            .collect();

        // order 应包含所有 File 节点
        let order_ids: HashSet<&str> = seq.order.iter().map(|id| id.as_str()).collect();

        for file_id in &file_ids {
            prop_assert!(
                order_ids.contains(file_id),
                "迁移序列缺少节点 {}，order={:?}", file_id, seq.order
            );
        }

        // order 中的节点数应等于 File 节点数（无多余节点）
        prop_assert_eq!(
            order_ids.len(),
            file_ids.len(),
            "迁移序列节点数应与文件节点数一致"
        );
    }

    /// 迁移序列完整性（有环图）：即使有环，order 也应包含所有 File 节点。
    #[test]
    fn proptest_migration_sequence_completeness_cyclic(graph in arb_cyclic_graph()) {
        let seq = migration_sequence(&graph);

        let file_ids: HashSet<&str> = graph
            .nodes()
            .filter(|n| n.node_type == NodeType::File)
            .map(|n| n.id.as_str())
            .collect();

        let order_ids: HashSet<&str> = seq.order.iter().map(|id| id.as_str()).collect();

        for file_id in &file_ids {
            prop_assert!(
                order_ids.contains(file_id),
                "有环图迁移序列缺少节点 {}，order={:?}", file_id, seq.order
            );
        }
    }

    /// 并行层无依赖：同一 sprint 层内的节点互无 Imports 依赖边。
    /// （ORCH-01 收口后并行层由 scc_groups 按 sprint 聚合得到，取代原 parallel_groups。）
    #[test]
    fn proptest_parallel_groups_no_internal_deps(graph in arb_dag()) {
        let seq = migration_sequence(&graph);

        // 收集所有 Imports 边（source -> target）
        let edges: HashSet<(&str, &str)> = graph
            .edges()
            .filter(|e| e.edge_type == EdgeType::Imports)
            .map(|e| (e.source.as_str(), e.target.as_str()))
            .collect();

        // 按 sprint 聚合 scc_groups 成员 → 并行层。
        let mut layers: std::collections::BTreeMap<u32, Vec<&str>> = std::collections::BTreeMap::new();
        for g in &seq.scc_groups {
            for id in &g.members {
                layers.entry(g.sprint).or_default().push(id.as_str());
            }
        }

        for (sprint, members) in &layers {
            let layer_ids: HashSet<&str> = members.iter().copied().collect();

            // 检查同层任意两个节点间无依赖边
            for &a in &layer_ids {
                for &b in &layer_ids {
                    if a != b {
                        prop_assert!(
                            !edges.contains(&(a, b)),
                            "并行层 sprint={} 内节点 {} 和 {} 之间存在依赖边",
                            sprint, a, b
                        );
                    }
                }
            }
        }
    }

    /// 并行分层覆盖所有节点：所有 scc_groups 成员的并集等于全部 File 节点集。
    #[test]
    fn proptest_parallel_groups_cover_all_nodes(graph in arb_dag()) {
        let seq = migration_sequence(&graph);

        let file_ids: HashSet<&str> = graph
            .nodes()
            .filter(|n| n.node_type == NodeType::File)
            .map(|n| n.id.as_str())
            .collect();

        let group_ids: HashSet<&str> = seq
            .scc_groups
            .iter()
            .flat_map(|g| g.members.iter().map(|id| id.as_str()))
            .collect();

        prop_assert_eq!(
            file_ids,
            group_ids,
            "SCC 迁移单位应覆盖所有文件节点"
        );
    }

    /// 有环图的并行层独立性（编排器原子调度的正确性基础）：
    /// 同一 sprint 号的**任意两个不同 SCC 组**之间无 Imports 边——即同层组可安全并行。
    ///
    /// 用多 SCC 生成器（每簇内成环形成独立 SCC，簇间单向）覆盖 `arb_cyclic_graph`
    /// 无法测的场景：后者全图一个大环（单 SCC），测不到「多 SCC 同层并行」。
    /// 这是收口后 scc_groups.sprint 在有环图下取代 parallel_groups 的关键不变量
    /// （异构审查 PR-1 指出的覆盖缺口）。
    #[test]
    fn proptest_scc_groups_same_sprint_independent_cyclic(graph in arb_multi_scc_graph()) {
        let seq = migration_sequence(&graph);

        // Imports 边集（source -> target）。
        let edges: HashSet<(&str, &str)> = graph
            .edges()
            .filter(|e| e.edge_type == EdgeType::Imports)
            .map(|e| (e.source.as_str(), e.target.as_str()))
            .collect();

        // 按 sprint 聚合组索引。
        let mut by_sprint: HashMap<u32, Vec<usize>> = HashMap::new();
        for (gi, g) in seq.scc_groups.iter().enumerate() {
            by_sprint.entry(g.sprint).or_default().push(gi);
        }

        // 同 sprint 的任意两个不同组之间，双向都不应存在 Imports 边。
        for (sprint, group_indices) in &by_sprint {
            for (a_pos, &ga) in group_indices.iter().enumerate() {
                for &gb in &group_indices[a_pos + 1..] {
                    let a_members: Vec<&str> =
                        seq.scc_groups[ga].members.iter().map(|id| id.as_str()).collect();
                    let b_members: Vec<&str> =
                        seq.scc_groups[gb].members.iter().map(|id| id.as_str()).collect();
                    for &am in &a_members {
                        for &bm in &b_members {
                            prop_assert!(
                                !edges.contains(&(am, bm)) && !edges.contains(&(bm, am)),
                                "同 sprint={} 的两组间存在依赖边：{} <-> {}",
                                sprint, am, bm
                            );
                        }
                    }
                }
            }
        }
    }
}
