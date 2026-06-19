//! 拓扑排序 + 环检测 + 迁移序列生成。
//!
//! 仅考虑 `NodeType::File` 节点和 `EdgeType::Imports` 边，
//! 叶节点（无依赖的文件）排在前面（优先迁移）。

use std::collections::{HashMap, HashSet};

use petgraph::algo;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::Serialize;

use crate::error::{MigrateError, Result};
use crate::types::common::NodeId;
use crate::types::graph::{EdgeType, NodeType};

use super::SourceGraph;

/// 迁移序列：包含迁移顺序、可并行组和环信息。
///
/// 纯数据结构体，字段 `pub`（Rust 惯例：无字段间不变量、不跨 crate 发布、外部仅 `&` 只读，
/// 无需私有化 + getter）。仅由本模块 [`migration_sequence`] 构造。
#[derive(Debug, Clone, Serialize)]
pub struct MigrationSequence {
    /// 迁移顺序（叶节点在前）。
    pub order: Vec<NodeId>,
    /// 可并行迁移的分组（同组内节点无依赖关系；组索引可映射为 sprint 序号）。
    pub parallel_groups: Vec<Vec<NodeId>>,
    /// 所有检测到的环。
    pub cycles: Vec<Vec<NodeId>>,
    /// 缩点后的 SCC 翻译单元（破环：每个 SCC 折叠为一个迁移单位）。
    ///
    /// 覆盖图中**全部** File 节点（单文件 = 单成员组），按缩点 DAG 拓扑层级排序。
    /// populate 以此为迁移单位：单成员组 → 单文件模块；多成员组 → composite 模块组。
    pub scc_groups: Vec<SccGroup>,
}

/// 一个 SCC 翻译单元（缩点后的迁移单位）。
///
/// 破环核心：源码中的循环依赖（互引文件）不再拒绝迁移，而是整组折叠为一个
/// 翻译单元——translator 一次性翻译为一组 Rust `mod`（同 crate 内 mod 间允许循环
/// `use`，无需破环），仅当组大到超上下文预算时才退化为 FFI 切分（TODO，兜底路径）。
#[derive(Debug, Clone, Serialize)]
pub struct SccGroup {
    /// 组内成员文件节点（按 NodeId 字典序排序；第一个作 module key 代表）。
    pub members: Vec<NodeId>,
    /// 迁移 sprint 号（缩点 DAG 拓扑层级 + 1；叶组 = sprint 1）。
    pub sprint: u32,
    /// 是否为真环（多节点 SCC 或自环）；单文件无环组为 `false`。
    pub is_cycle: bool,
}

impl MigrationSequence {
    /// 是否存在循环依赖。
    pub fn has_cycles(&self) -> bool {
        !self.cycles.is_empty()
    }
}

/// 对文件级节点做拓扑排序（仅考虑 Imports 边）。
///
/// 叶节点（无依赖的文件）排在前面，先迁移。
/// 检测到环时返回 `MigrateError::CyclicDependency`。
pub fn topological_sort(graph: &SourceGraph) -> Result<Vec<NodeId>> {
    let (file_graph, index_to_id, _) = build_file_import_graph(graph);

    match algo::toposort(&file_graph, None) {
        Ok(sorted) => {
            // petgraph toposort：对于边 u->v，u 排在 v 前。
            // 在我们的图中 A->B 表示 A imports B（A 依赖 B）。
            // 所以 toposort 返回 [A, B]，但我们需要 B 先迁移 → 反转。
            let result: Vec<NodeId> = sorted
                .into_iter()
                .rev()
                .filter_map(|idx| index_to_id.get(&idx).cloned())
                .collect();
            Ok(result)
        }
        Err(_cycle_node) => {
            // 检测完整的环信息
            let cycles = detect_cycles_internal(&file_graph, &index_to_id);
            let cycle_str = if let Some(first_cycle) = cycles.first() {
                first_cycle
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ")
            } else {
                "unknown".to_string()
            };
            Err(MigrateError::CyclicDependency { cycle: cycle_str })
        }
    }
}

/// 检测所有强连通分量（大小 > 1 即为环）。
///
/// 使用 Tarjan 算法查找所有 SCC，过滤出包含多个节点的分量。
pub fn detect_cycles(graph: &SourceGraph) -> Vec<Vec<NodeId>> {
    let (file_graph, index_to_id, _) = build_file_import_graph(graph);
    detect_cycles_internal(&file_graph, &index_to_id)
}

/// 生成完整迁移序列（含并行分组和环信息）。
///
/// 即使存在环也会尽力生成排序（基于 SCC 凝缩图的拓扑序）。
pub fn migration_sequence(graph: &SourceGraph) -> MigrationSequence {
    let (file_graph, index_to_id, id_to_index) = build_file_import_graph(graph);

    // 单次 Tarjan SCC 计算，同时提取环信息和有环时的排序
    let sccs = algo::tarjan_scc(&file_graph);
    let cycles: Vec<Vec<NodeId>> = sccs
        .iter()
        .filter(|scc| scc_is_cycle(scc, &file_graph))
        .map(|scc| {
            scc.iter()
                .filter_map(|idx| index_to_id.get(idx).cloned())
                .collect()
        })
        .collect();
    let has_cycles = !cycles.is_empty();

    // 缩点为 SCC 翻译单元（破环：覆盖全部 File 节点，含单文件单成员组）
    let scc_groups = build_scc_groups(&sccs, &file_graph, &index_to_id);

    // 计算迁移顺序
    let order = if !has_cycles {
        // 无环：标准拓扑排序
        match algo::toposort(&file_graph, None) {
            Ok(sorted) => sorted
                .into_iter()
                .rev()
                .filter_map(|idx| index_to_id.get(&idx).cloned())
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        // 有环：复用 tarjan_scc 结果（逆拓扑序 SCC，叶节点在前）
        sccs.into_iter()
            .flat_map(|scc| {
                scc.into_iter()
                    .filter_map(|idx| index_to_id.get(&idx).cloned())
            })
            .collect()
    };

    // 计算并行分组
    let parallel_groups = compute_parallel_groups(&file_graph, &index_to_id, &id_to_index);

    MigrationSequence {
        order,
        parallel_groups,
        cycles,
        scc_groups,
    }
}

/// 把 Tarjan SCC 结果缩点为迁移单元，并在缩点 DAG 上计算每组 sprint 层级。
///
/// 输入 `sccs` 为 `tarjan_scc` 输出（每个 SCC 是 NodeIndex 列表）。输出覆盖全部
/// File 节点：单节点 SCC → 单文件组，多节点（或自环）SCC → 循环依赖模块组。
/// 输出按 `(sprint, 首成员 NodeId)` 稳定排序，保证确定性。
fn build_scc_groups(
    sccs: &[Vec<NodeIndex>],
    file_graph: &StableGraph<NodeId, ()>,
    index_to_id: &HashMap<NodeIndex, NodeId>,
) -> Vec<SccGroup> {
    let n = sccs.len();
    if n == 0 {
        return Vec::new();
    }

    // node_idx -> scc_id
    let mut scc_of: HashMap<NodeIndex, usize> = HashMap::new();
    for (sid, scc) in sccs.iter().enumerate() {
        for &idx in scc {
            scc_of.insert(idx, sid);
        }
    }

    // 缩点 DAG 出边邻接（scc -> scc，去自环、去重）
    let mut succ: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for (&idx, &sid) in &scc_of {
        for edge in file_graph.edges_directed(idx, Direction::Outgoing) {
            if let Some(&tgt_sid) = scc_of.get(&edge.target()) {
                if tgt_sid != sid {
                    succ[sid].insert(tgt_sid);
                }
            }
        }
    }

    // 缩点 DAG 层级（叶组 = 0），迭代式避免深链栈溢出
    let mut levels: Vec<Option<usize>> = vec![None; n];
    for start in 0..n {
        if levels[start].is_none() {
            compute_scc_level(start, &succ, &mut levels);
        }
    }

    let mut groups: Vec<SccGroup> = sccs
        .iter()
        .enumerate()
        .map(|(sid, scc)| {
            let mut members: Vec<NodeId> = scc
                .iter()
                .filter_map(|idx| index_to_id.get(idx).cloned())
                .collect();
            members.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            SccGroup {
                members,
                sprint: (levels[sid].unwrap_or(0) as u32) + 1,
                is_cycle: scc_is_cycle(scc, file_graph),
            }
        })
        .collect();

    // 稳定排序：sprint 升序，同 sprint 按首成员字典序
    groups.sort_by(|a, b| {
        a.sprint.cmp(&b.sprint).then_with(|| {
            let ka = a.members.first().map(|x| x.as_str()).unwrap_or("");
            let kb = b.members.first().map(|x| x.as_str()).unwrap_or("");
            ka.cmp(kb)
        })
    });
    groups
}

/// 计算缩点 DAG 中单个 SCC 节点的层级（迭代式 DFS + 记忆化）。
///
/// 层级 0 = 叶组（无出边），层级 n = 依赖链最长为 n 的组。缩点图本身无环，
/// `on_path` 仅作防御。复用与 [`compute_level`] 相同的「Enter/Exit 显式栈」骨架。
fn compute_scc_level(start: usize, succ: &[HashSet<usize>], levels: &mut [Option<usize>]) {
    enum Work {
        Enter(usize),
        Exit(usize),
    }
    let mut stack = vec![Work::Enter(start)];
    let mut on_path: HashSet<usize> = HashSet::new();

    while let Some(work) = stack.pop() {
        match work {
            Work::Enter(i) => {
                if levels[i].is_some() || on_path.contains(&i) {
                    continue;
                }
                on_path.insert(i);
                stack.push(Work::Exit(i));
                for &j in &succ[i] {
                    if levels[j].is_none() && !on_path.contains(&j) {
                        stack.push(Work::Enter(j));
                    }
                }
            }
            Work::Exit(i) => {
                let level = succ[i]
                    .iter()
                    .map(|&j| levels[j].unwrap_or(0))
                    .max()
                    .map(|l| l + 1)
                    .unwrap_or(0);
                on_path.remove(&i);
                levels[i] = Some(level);
            }
        }
    }
}

/// 构建仅包含 File 节点和 Imports 边的子图。
///
/// 返回 (子图, NodeIndex->NodeId 映射, NodeId->NodeIndex 映射)。
fn build_file_import_graph(
    graph: &SourceGraph,
) -> (
    StableGraph<NodeId, ()>,
    HashMap<NodeIndex, NodeId>,
    HashMap<NodeId, NodeIndex>,
) {
    let mut file_graph: StableGraph<NodeId, ()> = StableGraph::new();
    let mut orig_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    let mut index_to_id: HashMap<NodeIndex, NodeId> = HashMap::new();
    let mut id_to_index: HashMap<NodeId, NodeIndex> = HashMap::new();

    // 添加 File 节点
    for node in graph.nodes() {
        if node.node_type == NodeType::File {
            if let Some(&orig_idx) = graph.index.get(&node.id) {
                let new_idx = file_graph.add_node(node.id.clone());
                orig_to_new.insert(orig_idx, new_idx);
                index_to_id.insert(new_idx, node.id.clone());
                id_to_index.insert(node.id.clone(), new_idx);
            }
        }
    }

    // 添加 Imports 边
    for edge in graph.edges() {
        if edge.edge_type == EdgeType::Imports {
            let src_orig = graph.index.get(&edge.source);
            let tgt_orig = graph.index.get(&edge.target);
            if let (Some(&src_orig), Some(&tgt_orig)) = (src_orig, tgt_orig) {
                if let (Some(&src_new), Some(&tgt_new)) =
                    (orig_to_new.get(&src_orig), orig_to_new.get(&tgt_orig))
                {
                    file_graph.add_edge(src_new, tgt_new, ());
                }
            }
        }
    }

    (file_graph, index_to_id, id_to_index)
}

/// 判断一个 SCC 是否构成环：多节点 SCC，或带自环的单节点（文件自导入）。
///
/// 注意：仅靠 `scc.len() > 1` 会漏掉自环——单节点 SCC 但存在指向自身的边时，
/// petgraph `toposort` 仍会判定为环，导致环检测与拓扑排序结论不一致。
fn scc_is_cycle(scc: &[NodeIndex], graph: &StableGraph<NodeId, ()>) -> bool {
    if scc.len() > 1 {
        return true;
    }
    scc.len() == 1
        && graph
            .edges_directed(scc[0], Direction::Outgoing)
            .any(|e| e.target() == scc[0])
}

/// 用 Tarjan SCC 检测环（内部实现）。
fn detect_cycles_internal(
    file_graph: &StableGraph<NodeId, ()>,
    index_to_id: &HashMap<NodeIndex, NodeId>,
) -> Vec<Vec<NodeId>> {
    algo::tarjan_scc(file_graph)
        .into_iter()
        .filter(|scc| scc_is_cycle(scc, file_graph))
        .map(|scc| {
            scc.into_iter()
                .filter_map(|idx| index_to_id.get(&idx).cloned())
                .collect()
        })
        .collect()
}

/// 计算可并行迁移的分组。
///
/// 基于拓扑层级：无出边（无 imports）的节点为第 0 层，
/// 仅依赖第 0 层的节点为第 1 层，以此类推。
/// 同层节点间无依赖关系，可并行迁移。
fn compute_parallel_groups(
    file_graph: &StableGraph<NodeId, ()>,
    index_to_id: &HashMap<NodeIndex, NodeId>,
    id_to_index: &HashMap<NodeId, NodeIndex>,
) -> Vec<Vec<NodeId>> {
    if id_to_index.is_empty() {
        return Vec::new();
    }

    // 计算每个节点的层级（到叶节点的最长路径距离）
    let mut levels: HashMap<NodeIndex, usize> = HashMap::new();
    // 排序根节点遍历顺序，保证有环图中各节点层级计算确定（id_to_index 是 HashMap）
    let mut indices: Vec<NodeIndex> = id_to_index.values().copied().collect();
    indices.sort();

    for &idx in &indices {
        if !levels.contains_key(&idx) {
            compute_level(idx, file_graph, &mut levels);
        }
    }

    // 按层级分组
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut groups: Vec<Vec<NodeId>> = vec![Vec::new(); max_level + 1];

    for (&idx, &level) in &levels {
        if let Some(id) = index_to_id.get(&idx) {
            groups[level].push(id.clone());
        }
    }

    // 过滤空组，并对每组内节点按 ID 排序，保证输出确定（levels 是 HashMap）
    let mut result: Vec<Vec<NodeId>> = groups.into_iter().filter(|g| !g.is_empty()).collect();
    for group in &mut result {
        group.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    }
    result
}

/// 计算节点层级（显式栈的迭代式 DFS + 记忆化），结果写入 `levels`。
///
/// 层级 0 = 叶节点（无出边），层级 n = 依赖链最长为 n 的节点。
/// 环上的后边贡献 0（不参与 +1），避免无限递归。
/// 使用显式栈而非递归，避免超深依赖链导致调用栈溢出。
fn compute_level(
    start: NodeIndex,
    graph: &StableGraph<NodeId, ()>,
    levels: &mut HashMap<NodeIndex, usize>,
) {
    enum Work {
        Enter(NodeIndex),
        Exit(NodeIndex),
    }
    let mut stack = vec![Work::Enter(start)];
    // 当前 DFS 路径上的节点（用于后边/环检测）
    let mut on_path: HashSet<NodeIndex> = HashSet::new();

    while let Some(work) = stack.pop() {
        match work {
            Work::Enter(idx) => {
                if levels.contains_key(&idx) || on_path.contains(&idx) {
                    continue;
                }
                on_path.insert(idx);
                // 先压 Exit（后于所有后代出栈），再压未访问的后继
                stack.push(Work::Exit(idx));
                for edge in graph.edges_directed(idx, Direction::Outgoing) {
                    let succ = edge.target();
                    if !levels.contains_key(&succ) && !on_path.contains(&succ) {
                        stack.push(Work::Enter(succ));
                    }
                }
            }
            Work::Exit(idx) => {
                // 此刻所有非后边的后继都已记忆化；后边（仍在 on_path、未记忆化）贡献 0
                let level = graph
                    .edges_directed(idx, Direction::Outgoing)
                    .map(|e| levels.get(&e.target()).copied().unwrap_or(0))
                    .max()
                    .map(|l| l + 1)
                    .unwrap_or(0);
                on_path.remove(&idx);
                levels.insert(idx, level);
            }
        }
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

    /// 辅助函数：在排序结果中查找节点位置（兼容有无 src/ 前缀）。
    fn find_position(order: &[NodeId], name: &str) -> Option<usize> {
        order.iter().position(|id| {
            let s = id.as_str();
            s == format!("file:{name}") || s == format!("file:src/{name}")
        })
    }

    #[test]
    fn self_import_is_detected_as_cycle() {
        let dir = std::env::temp_dir().join("rustmigrate_self_import_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("self.ts"),
            "import { x } from './self';\nexport const x = 1;\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let cycles = detect_cycles(&graph);
        let topo = topological_sort(&graph);
        let _ = std::fs::remove_dir_all(&dir);

        // 自环应被识别为环（不能因单节点 SCC 被过滤而漏报）
        assert!(
            cycles
                .iter()
                .any(|c| c.iter().any(|id| id.as_str().contains("self.ts"))),
            "自导入应被识别为环，实际: {cycles:?}"
        );
        // 且与拓扑排序结论一致：toposort 同样应判定为环而返回错误
        assert!(topo.is_err(), "自环图的拓扑排序应返回错误");
    }

    #[test]
    fn compute_level_handles_deep_chain_without_overflow() {
        // 超深线性依赖链：递归实现会栈溢出，迭代实现应正常计算
        let n: usize = 50_000;
        let mut g: StableGraph<NodeId, ()> = StableGraph::new();
        let first = g.add_node(NodeId::new("file:0.ts"));
        let mut prev = first;
        for i in 1..n {
            let cur = g.add_node(NodeId::new(format!("file:{i}.ts")));
            g.add_edge(prev, cur, ()); // i-1 imports i（importer → imported）
            prev = cur;
        }

        let mut levels: HashMap<NodeIndex, usize> = HashMap::new();
        compute_level(first, &g, &mut levels);

        // 链头（0.ts）依赖链最长，层级应为 n-1；链尾为叶子，层级 0
        assert_eq!(levels.get(&first).copied(), Some(n - 1));
        assert_eq!(levels.get(&prev).copied(), Some(0));
    }

    // === linear-deps 测试 ===

    #[test]
    fn topo_sort_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let order = topological_sort(&graph).unwrap();

        // ground-truth 偏序约束：utils < service < index
        let pos_utils = find_position(&order, "utils.ts").expect("应包含 utils.ts");
        let pos_service = find_position(&order, "service.ts").expect("应包含 service.ts");
        let pos_index = find_position(&order, "index.ts").expect("应包含 index.ts");

        assert!(
            pos_utils < pos_service,
            "utils.ts 应排在 service.ts 前，实际顺序: {order:?}"
        );
        assert!(
            pos_service < pos_index,
            "service.ts 应排在 index.ts 前，实际顺序: {order:?}"
        );
    }

    #[test]
    fn migration_sequence_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let seq = migration_sequence(&graph);

        assert!(!seq.has_cycles(), "linear-deps 不应有环");
        assert!(seq.cycles.is_empty());
        assert!(!seq.order.is_empty());
        assert!(!seq.parallel_groups.is_empty());

        // 验证并行组的层级关系
        // 第 0 组应包含叶节点（utils.ts），最后一组应包含根节点（index.ts）
        let first_group = &seq.parallel_groups[0];
        let has_leaf = first_group.iter().any(|id| {
            let s = id.as_str();
            s.ends_with("utils.ts")
        });
        assert!(
            has_leaf,
            "第一组应包含叶节点 utils.ts，实际: {first_group:?}"
        );
    }

    // === diamond-deps 测试 ===

    #[test]
    fn topo_sort_diamond_deps() {
        let root = fixtures_dir().join("diamond-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let order = topological_sort(&graph).unwrap();

        // ground-truth 偏序约束
        let pos_types = find_position(&order, "types.ts").expect("应包含 types.ts");
        let pos_db = find_position(&order, "db.ts").expect("应包含 db.ts");
        let pos_auth = find_position(&order, "auth.ts").expect("应包含 auth.ts");
        let pos_index = find_position(&order, "index.ts").expect("应包含 index.ts");

        assert!(
            pos_types < pos_db,
            "types.ts 应排在 db.ts 前，实际顺序: {order:?}"
        );
        assert!(
            pos_types < pos_auth,
            "types.ts 应排在 auth.ts 前，实际顺序: {order:?}"
        );
        assert!(
            pos_db < pos_auth,
            "db.ts 应排在 auth.ts 前，实际顺序: {order:?}"
        );
        assert!(
            pos_auth < pos_index,
            "auth.ts 应排在 index.ts 前，实际顺序: {order:?}"
        );
    }

    #[test]
    fn topo_sort_diamond_deps_barrel_constraints() {
        let root = fixtures_dir().join("diamond-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let order = topological_sort(&graph).unwrap();

        // barrel.ts 依赖 types/db/auth，应排在它们后面
        if let Some(pos_barrel) = find_position(&order, "barrel.ts") {
            let pos_auth = find_position(&order, "auth.ts").expect("应包含 auth.ts");
            let pos_db = find_position(&order, "db.ts").expect("应包含 db.ts");
            assert!(
                pos_auth < pos_barrel,
                "auth.ts 应排在 barrel.ts 前，实际顺序: {order:?}"
            );
            assert!(
                pos_db < pos_barrel,
                "db.ts 应排在 barrel.ts 前，实际顺序: {order:?}"
            );
        }
    }

    // === circular-deps 测试 ===

    #[test]
    fn topo_sort_circular_deps_returns_error() {
        let root = fixtures_dir().join("circular-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let result = topological_sort(&graph);

        assert!(
            result.is_err(),
            "circular-deps 应返回 CyclicDependency 错误"
        );

        if let Err(MigrateError::CyclicDependency { cycle }) = result {
            // 环应包含 event-bus/handler/emitter
            let has_event_bus = cycle.contains("event-bus");
            let has_handler = cycle.contains("handler");
            let has_emitter = cycle.contains("emitter");
            assert!(
                has_event_bus || has_handler || has_emitter,
                "环应包含 event-bus/handler/emitter 中的至少一个，实际: {cycle}"
            );
        }
    }

    #[test]
    fn detect_cycles_circular_deps() {
        let root = fixtures_dir().join("circular-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let cycles = detect_cycles(&graph);

        assert!(!cycles.is_empty(), "应检测到至少一个环");

        // 至少有一个环包含 event-bus、handler、emitter
        let has_expected_cycle = cycles.iter().any(|cycle| {
            let ids: Vec<&str> = cycle.iter().map(|id| id.as_str()).collect();
            let has_eb = ids.iter().any(|s| s.contains("event-bus"));
            let has_h = ids.iter().any(|s| s.contains("handler"));
            let has_e = ids.iter().any(|s| s.contains("emitter"));
            has_eb && has_h && has_e
        });
        assert!(
            has_expected_cycle,
            "应包含 event-bus -> handler -> emitter 的环，实际: {cycles:?}"
        );
    }

    #[test]
    fn migration_sequence_circular_deps() {
        let root = fixtures_dir().join("circular-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let seq = migration_sequence(&graph);

        assert!(seq.has_cycles(), "circular-deps 应标记 has_cycles=true");
        assert!(!seq.cycles.is_empty(), "应包含环信息");
        // 即使有环也应生成排序
        assert!(!seq.order.is_empty(), "有环时仍应生成尽力排序");
    }

    // === SCC 模块组（破环 M2-SCALE-SCC）测试 ===

    #[test]
    fn scc_groups_circular_deps_folds_cycle_into_one_group() {
        // 破环核心：event-bus ↔ handler ↔ emitter 的环应折叠为单个多成员 SCC 组，
        // 而非拒绝或拆成多个单文件组。
        let root = fixtures_dir().join("circular-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let seq = migration_sequence(&graph);

        // scc_groups 覆盖全部文件
        let total_members: usize = seq.scc_groups.iter().map(|g| g.members.len()).sum();
        assert_eq!(
            total_members,
            seq.order.len(),
            "scc_groups 成员总数应覆盖全部文件节点"
        );

        // 应存在一个多成员环组，含 event-bus/handler/emitter
        let cycle_group = seq
            .scc_groups
            .iter()
            .find(|g| g.is_cycle && g.members.len() > 1)
            .expect("应有一个多成员环组");
        let ids: Vec<&str> = cycle_group.members.iter().map(|id| id.as_str()).collect();
        assert!(
            ids.iter().any(|s| s.contains("event-bus"))
                && ids.iter().any(|s| s.contains("handler"))
                && ids.iter().any(|s| s.contains("emitter")),
            "环组应含 event-bus/handler/emitter，实际: {ids:?}"
        );
        // 成员有序（字典序），首成员作 key 代表
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "组成员应按字典序排列");
    }

    #[test]
    fn scc_groups_linear_deps_all_single_member() {
        // 无环图：每个文件自成一组，无环标记，sprint 按层级递增。
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let seq = migration_sequence(&graph);

        assert!(!seq.scc_groups.is_empty());
        assert!(
            seq.scc_groups
                .iter()
                .all(|g| g.members.len() == 1 && !g.is_cycle),
            "无环图每组应为单成员且非环"
        );
        // 叶组 sprint=1；存在更高 sprint（链式依赖）
        assert!(seq.scc_groups.iter().any(|g| g.sprint == 1));
        assert!(
            seq.scc_groups.iter().map(|g| g.sprint).max().unwrap() > 1,
            "线性依赖应产生多个 sprint 层级"
        );
    }

    #[test]
    fn scc_groups_empty_graph() {
        let graph = SourceGraph::new();
        let seq = migration_sequence(&graph);
        assert!(seq.scc_groups.is_empty());
    }

    // === 边界情况测试 ===

    #[test]
    fn topo_sort_empty_graph() {
        let graph = SourceGraph::new();
        let order = topological_sort(&graph).unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn detect_cycles_no_cycles() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();
        let cycles = detect_cycles(&graph);
        assert!(cycles.is_empty(), "linear-deps 不应有环");
    }

    #[test]
    fn migration_sequence_empty_graph() {
        let graph = SourceGraph::new();
        let seq = migration_sequence(&graph);
        assert!(seq.order.is_empty());
        assert!(seq.parallel_groups.is_empty());
        assert!(!seq.has_cycles());
        assert!(seq.cycles.is_empty());
    }
}
