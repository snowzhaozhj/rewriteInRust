//! 模块拆解引擎（M3-DEC-01）：在 SCC 缩点 DAG 上做凸性拓扑 first-fit 装箱。
//!
//! 把"每文件=一个翻译单元"改为"机械小文件按 footprint 预算合批"，治理小而机械文件被
//! 过度处理（方案权威见 `docs/decomposition-redesign.md`）。本模块是**纯算法**：footprint
//! 与机械判定由调用方（CLI）计算后传入，便于单测；不读文件系统、不依赖 adapter。
//!
//! 产出 [`DecompositionPlan`] 供 dry-run 报告消费（PR-1 不进 active dispatch，见方案 §7 B1）。

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::Serialize;

use crate::types::common::NodeId;
use crate::types::graph::EdgeType;

use super::fingerprint;
use super::topo::MigrationSequence;
use super::SourceGraph;

/// 拆解单元类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    /// 单文件模块（落预算带内、非机械，或机械但无法合批）。
    Single,
    /// 机械合批组（≥2 个机械小文件，凸性 first-fit 装箱）。
    Batch,
    /// 循环依赖组（多成员 SCC 或自环）——走现有契约重路径。
    Cycle,
    /// 超 footprint 预算的单文件——标记转人工，不进自动 dispatch。
    ManualOverBudget,
}

/// 一个拆解单元。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecompUnit {
    /// 成员文件（NodeId 字典序；第一个作 module key 代表）。
    pub members: Vec<String>,
    /// 单元类型。
    pub kind: UnitKind,
    /// 单元 footprint（成员 footprint 之和，token≈bytes/4）。
    pub footprint: usize,
    /// 迁移 sprint（取首个构成组的 sprint）。
    pub sprint: u32,
}

/// 完整拆解计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecompositionPlan {
    /// 拆解单元（按首成员字典序稳定排序，确定性输出）。
    pub units: Vec<DecompUnit>,
}

impl DecompositionPlan {
    /// 拆解计划的 canonical content hash（M3-DEC-01，Codex I-4）。
    ///
    /// 不依赖 `HashMap`/`serde_json::to_string_pretty` 的键序——基于已稳定排序的 units +
    /// 各成员有序列表手工拼装 canonical 串后取 SHA256，保证"跑两次字节级一致"。
    pub fn canonical_hash(&self) -> String {
        // 长度前缀编码（`<len>:<value>;`），避免成员路径含分隔符时不同 plan 碰撞同串（Codex nit）。
        let mut s = String::new();
        for u in &self.units {
            push_field(&mut s, unit_kind_key(u.kind));
            push_field(&mut s, &u.footprint.to_string());
            push_field(&mut s, &u.sprint.to_string());
            push_field(&mut s, &u.members.len().to_string());
            for m in &u.members {
                push_field(&mut s, m);
            }
            s.push('\n');
        }
        fingerprint::content_hash(&s)
    }

    /// 模块数缩减统计：before（1 文件=1 模块）vs after（拆解后单元数）。
    pub fn module_count_after(&self) -> usize {
        self.units.len()
    }

    /// 被合批文件总数（§8「被合批文件占比」的分子）：所有 Batch 单元成员数之和。
    pub fn batched_file_count(&self) -> usize {
        self.units
            .iter()
            .filter(|u| u.kind == UnitKind::Batch)
            .map(|u| u.members.len())
            .sum()
    }
}

/// canonical_hash 用：把字段以 `<len>:<value>;` 追加，长度前缀消除分隔符歧义。
fn push_field(s: &mut String, field: &str) {
    s.push_str(&field.len().to_string());
    s.push(':');
    s.push_str(field);
    s.push(';');
}

fn unit_kind_key(k: UnitKind) -> &'static str {
    match k {
        UnitKind::Single => "single",
        UnitKind::Batch => "batch",
        UnitKind::Cycle => "cycle",
        UnitKind::ManualOverBudget => "manual_over_budget",
    }
}

/// 在 SCC 缩点 DAG 上做凸性拓扑 first-fit 装箱，产出拆解计划。
///
/// - `seq`：迁移序列（提供 SCC 组、确定性顺序、is_cycle）。
/// - `footprints`：每个**文件** NodeId 的 footprint（自身源码 + 被用依赖签名，token≈bytes/4）。
///   缺失的文件按 0 处理（保守：倾向合批，但同时受凸性约束）。
/// - `mechanical`：机械文件 NodeId 集合（CLI 用 `classify_file` 判定）。
/// - `budget`：footprint 预算 B。
///
/// 规则：① 循环组 → `Cycle`；② 单文件超预算 → `ManualOverBudget`；③ 单文件机械且不超预算
/// → 进 first-fit 合批（满预算或破凸性即封口）；④ 其余单文件 → `Single`。合批组 size==1
/// 退化为 `Single`。凸性 = 合并不得制造跨外部组的回边。
pub fn plan_decomposition(
    graph: &SourceGraph,
    seq: &MigrationSequence,
    footprints: &HashMap<NodeId, usize>,
    mechanical: &HashSet<NodeId>,
    budget: usize,
) -> DecompositionPlan {
    let groups = &seq.scc_groups;
    let n = groups.len();

    // 文件 NodeId → 组索引。
    let mut group_of: HashMap<&NodeId, usize> = HashMap::new();
    for (gi, g) in groups.iter().enumerate() {
        for m in &g.members {
            group_of.insert(m, gi);
        }
    }

    // 缩点 DAG 出边邻接（去自环）。
    let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for dep in graph.edges() {
        if dep.edge_type != EdgeType::Imports {
            continue;
        }
        if let (Some(&si), Some(&ti)) = (group_of.get(&dep.source), group_of.get(&dep.target)) {
            if si != ti {
                succ[si].insert(ti);
            }
        }
    }
    let reach = transitive_reachability(&succ);

    // 组 footprint = 成员 footprint 之和。
    let group_footprint = |g: &super::topo::SccGroup| -> usize {
        g.members
            .iter()
            .map(|m| footprints.get(m).copied().unwrap_or(0))
            .sum()
    };

    let mut units: Vec<DecompUnit> = Vec::new();
    // 当前开放的合批：组索引集 + footprint 累计 + 起始 sprint。
    let mut open: Vec<usize> = Vec::new();
    let mut open_fp: usize = 0;

    let flush = |open: &mut Vec<usize>, open_fp: &mut usize, units: &mut Vec<DecompUnit>| {
        if open.is_empty() {
            return;
        }
        let mut members: Vec<String> = open
            .iter()
            .flat_map(|&gi| groups[gi].members.iter().map(|m| m.to_string()))
            .collect();
        members.sort();
        let kind = if open.len() > 1 {
            UnitKind::Batch
        } else {
            UnitKind::Single
        };
        let sprint = open.iter().map(|&gi| groups[gi].sprint).min().unwrap_or(1);
        units.push(DecompUnit {
            members,
            kind,
            footprint: *open_fp,
            sprint,
        });
        open.clear();
        *open_fp = 0;
    };

    for (gi, g) in groups.iter().enumerate() {
        let fp = group_footprint(g);

        // 循环组：封口当前合批，单独成单元。
        if g.is_cycle {
            flush(&mut open, &mut open_fp, &mut units);
            let mut members: Vec<String> = g.members.iter().map(|m| m.to_string()).collect();
            members.sort();
            units.push(DecompUnit {
                members,
                kind: UnitKind::Cycle,
                footprint: fp,
                sprint: g.sprint,
            });
            continue;
        }

        // 单文件组。
        let is_mech = g.members.iter().all(|m| mechanical.contains(m));
        if fp > budget {
            // 超预算 → 转人工（即便机械也装不下）。
            flush(&mut open, &mut open_fp, &mut units);
            let mut members: Vec<String> = g.members.iter().map(|m| m.to_string()).collect();
            members.sort();
            units.push(DecompUnit {
                members,
                kind: UnitKind::ManualOverBudget,
                footprint: fp,
                sprint: g.sprint,
            });
            continue;
        }

        if !is_mech {
            // 非机械单文件：封口合批，单独成 Single。
            flush(&mut open, &mut open_fp, &mut units);
            units.push(DecompUnit {
                members: vec![groups[gi].members[0].to_string()],
                kind: UnitKind::Single,
                footprint: fp,
                sprint: g.sprint,
            });
            continue;
        }

        // 机械且不超预算 → 尝试加入当前合批。
        let fits_budget = open_fp + fp <= budget;
        let stays_convex = {
            let mut candidate: BTreeSet<usize> = open.iter().copied().collect();
            candidate.insert(gi);
            is_convex(&candidate, &reach, n)
        };
        if !open.is_empty() && fits_budget && stays_convex {
            open.push(gi);
            open_fp += fp;
        } else {
            // 预算/凸性不允许并入 → 封口旧组，自身开新组。
            flush(&mut open, &mut open_fp, &mut units);
            open.push(gi);
            open_fp = fp;
        }
    }
    flush(&mut open, &mut open_fp, &mut units);

    // 稳定排序：按首成员字典序（units 内 members 已排序）。
    units.sort_by(|a, b| {
        a.members
            .first()
            .map(String::as_str)
            .unwrap_or("")
            .cmp(b.members.first().map(String::as_str).unwrap_or(""))
    });

    DecompositionPlan { units }
}

/// 缩点 DAG 的传递可达集（reach[i] = 从 i 出发可达的组，不含 i 自身）。
fn transitive_reachability(succ: &[BTreeSet<usize>]) -> Vec<BTreeSet<usize>> {
    let n = succ.len();
    let mut reach: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for start in 0..n {
        let mut stack: Vec<usize> = succ[start].iter().copied().collect();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            for &nx in &succ[cur] {
                if !seen.contains(&nx) {
                    stack.push(nx);
                }
            }
        }
        reach[start] = seen;
    }
    reach
}

/// 凸性判定：不存在外部组 x，使得"集合内某组可达 x"且"x 可达集合内某组"。
///
/// 该外部 x 会在合并后制造 merged↔x 回边（成环 / 破缩点 DAG），故禁止。
fn is_convex(set: &BTreeSet<usize>, reach: &[BTreeSet<usize>], n: usize) -> bool {
    for x in 0..n {
        if set.contains(&x) {
            continue;
        }
        let set_reaches_x = set.iter().any(|&a| reach[a].contains(&x));
        if !set_reaches_x {
            continue;
        }
        let x_reaches_set = set.iter().any(|&b| reach[x].contains(&b));
        if x_reaches_set {
            return false;
        }
    }
    true
}

// === 验收度量（M3-DEC-01 §8 验收门）===

/// 内聚质量（MQ）报告（§8 内聚维度，硬门）。
#[derive(Debug, Clone, Serialize)]
pub struct CohesionMq {
    /// 实际加权 MQ（各 batch 内部/(内部+外部) 按边数加权平均）。
    pub actual: f64,
    /// 随机基线 MQ（固定 seed、保持各 batch 大小重排成员，N 次均值）。
    pub baseline: f64,
    /// actual / baseline；要求 ≥1.5（首版阈值）。
    pub ratio: f64,
    /// 参与统计的合批文件数。
    pub batched_files: usize,
}

/// 拆解正确性不变量报告（§8 硬门 100%）。
#[derive(Debug, Clone, Serialize)]
pub struct Invariants {
    /// 每文件恰好归属一个单元（无重复、无遗漏）。
    pub partition_ok: bool,
    /// 单元级依赖图无环（凸性 + 环隔离的后验）。
    pub dag_acyclic: bool,
    /// 超预算转人工单元数。
    pub over_budget_count: usize,
    /// 拆解计划 canonical hash（两次运行应一致）。
    pub plan_hash: String,
}

/// 极简种子 PRNG（xorshift64*），用于内聚随机基线——无外部依赖、确定可复现。
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// 计算给定「文件→batch 编号」分配下的全局加权 MQ。
fn weighted_mq(assign: &HashMap<String, usize>, graph: &SourceGraph) -> f64 {
    use std::collections::BTreeMap;
    let mut internal: BTreeMap<usize, u64> = BTreeMap::new();
    let mut external: BTreeMap<usize, u64> = BTreeMap::new();
    for dep in graph.edges() {
        if dep.edge_type != EdgeType::Imports {
            continue;
        }
        // 统计任一端在 batch 内的边（Codex：原版只看 source 在 batch，漏掉外部→batch 的入边）。
        let bu = assign.get(dep.source.as_str()).copied();
        let bv = assign.get(dep.target.as_str()).copied();
        match (bu, bv) {
            (Some(a), Some(b)) if a == b => *internal.entry(a).or_insert(0) += 1,
            // 跨 batch 边：对两端各算一次外部边（出边离开 a、入边进入 b）。
            (Some(a), Some(b)) => {
                *external.entry(a).or_insert(0) += 1;
                *external.entry(b).or_insert(0) += 1;
            }
            (Some(a), None) => *external.entry(a).or_insert(0) += 1,
            (None, Some(b)) => *external.entry(b).or_insert(0) += 1,
            (None, None) => {}
        }
    }
    // 各 batch 内聚 = 内部/(内部+外部)，按 batch 边数(内部+外部)加权平均
    // = Σ内部 / Σ(内部+外部)（边数加权的代数化简）。
    let mut total_internal = 0.0;
    let mut total_edges = 0.0;
    let batches: BTreeSet<usize> = assign.values().copied().collect();
    for b in batches {
        let i = *internal.get(&b).unwrap_or(&0) as f64;
        let e = *external.get(&b).unwrap_or(&0) as f64;
        total_internal += i;
        total_edges += i + e;
    }
    if total_edges > 0.0 {
        total_internal / total_edges
    } else {
        0.0
    }
}

/// 内聚 MQ：实际 vs 随机基线（§8 硬门，Codex N-2 固定参数）。
///
/// 随机基线：保持各 batch 大小不变，固定 seed 把全部合批文件随机重排进同样大小的桶，
/// 算同一加权 MQ，取 `samples` 次均值。无合批组时返回中性 ratio=1.0（门不适用）。
pub fn cohesion_mq(
    plan: &DecompositionPlan,
    graph: &SourceGraph,
    seed: u64,
    samples: usize,
) -> CohesionMq {
    // 收集合批文件 → batch 编号 + 各 batch 大小。
    let mut assign: HashMap<String, usize> = HashMap::new();
    let mut sizes: Vec<usize> = Vec::new();
    for u in &plan.units {
        if u.kind == UnitKind::Batch {
            let bid = sizes.len();
            for m in &u.members {
                assign.insert(m.clone(), bid);
            }
            sizes.push(u.members.len());
        }
    }
    let batched_files = assign.len();
    if batched_files == 0 {
        return CohesionMq {
            actual: 0.0,
            baseline: 0.0,
            ratio: 1.0,
            batched_files: 0,
        };
    }

    let actual = weighted_mq(&assign, graph);

    // 全部合批文件（确定序），随机重排进同样大小的桶。
    let mut files: Vec<String> = assign.keys().cloned().collect();
    files.sort();
    let mut rng = XorShift64::new(seed);
    let mut acc = 0.0;
    let samples = samples.max(1);
    for _ in 0..samples {
        // Fisher-Yates 重排。
        let mut perm = files.clone();
        for i in (1..perm.len()).rev() {
            let j = rng.below(i + 1);
            perm.swap(i, j);
        }
        // 按各 batch 大小切片重新分配。
        let mut shuffled: HashMap<String, usize> = HashMap::new();
        let mut idx = 0;
        for (bid, &sz) in sizes.iter().enumerate() {
            for _ in 0..sz {
                shuffled.insert(perm[idx].clone(), bid);
                idx += 1;
            }
        }
        acc += weighted_mq(&shuffled, graph);
    }
    let baseline = acc / samples as f64;
    // baseline=0 且 actual>0 = 随机基线退化为零内聚、实际有内聚 → 视作远超阈值；用有限哨兵
    // RATIO_SENTINEL_MAX 而非 f64::INFINITY，避免 serde_json 把非有限浮点序列化为 null（多审查交叉确认）。
    const RATIO_SENTINEL_MAX: f64 = 999.0;
    let ratio = if baseline > 0.0 {
        actual / baseline
    } else if actual > 0.0 {
        RATIO_SENTINEL_MAX
    } else {
        1.0
    };
    CohesionMq {
        actual,
        baseline,
        ratio,
        batched_files,
    }
}

/// 校验拆解正确性不变量（§8 硬门）。`expected_files` = 图中 File 节点总数。
pub fn verify_invariants(
    plan: &DecompositionPlan,
    graph: &SourceGraph,
    expected_files: usize,
) -> Invariants {
    // 分区：成员展开后无重复且总数 == 文件数。
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut total = 0usize;
    let mut dup = false;
    for u in &plan.units {
        for m in &u.members {
            total += 1;
            if !seen.insert(m.as_str()) {
                dup = true;
            }
        }
    }
    let partition_ok = !dup && total == expected_files && seen.len() == expected_files;

    let dag_acyclic = unit_graph_acyclic(plan, graph);
    let over_budget_count = plan
        .units
        .iter()
        .filter(|u| u.kind == UnitKind::ManualOverBudget)
        .count();

    Invariants {
        partition_ok,
        dag_acyclic,
        over_budget_count,
        plan_hash: plan.canonical_hash(),
    }
}

/// 单元级依赖图无环检测（按文件→单元映射归并 Imports 边后 DFS 查环）。
fn unit_graph_acyclic(plan: &DecompositionPlan, graph: &SourceGraph) -> bool {
    let mut unit_of: HashMap<&str, usize> = HashMap::new();
    for (ui, u) in plan.units.iter().enumerate() {
        for m in &u.members {
            unit_of.insert(m.as_str(), ui);
        }
    }
    let n = plan.units.len();
    let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for dep in graph.edges() {
        if dep.edge_type != EdgeType::Imports {
            continue;
        }
        if let (Some(&su), Some(&tu)) = (
            unit_of.get(dep.source.as_str()),
            unit_of.get(dep.target.as_str()),
        ) {
            if su != tu {
                succ[su].insert(tu);
            }
        }
    }
    // DFS 三色查环。
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color = vec![Color::White; n];
    for start in 0..n {
        if color[start] != Color::White {
            continue;
        }
        let mut stack: Vec<(usize, bool)> = vec![(start, false)];
        while let Some((node, exiting)) = stack.pop() {
            if exiting {
                color[node] = Color::Black;
                continue;
            }
            // 已访问（Gray 在栈中 / Black 已完成）的节点直接跳过，避免 Black 节点被重染重扫（Codex nit）。
            if color[node] != Color::White {
                continue;
            }
            color[node] = Color::Gray;
            stack.push((node, true));
            for &nx in &succ[node] {
                match color[nx] {
                    Color::Gray => return false, // 回边 → 有环
                    Color::White => stack.push((nx, false)),
                    Color::Black => {}
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::topo::migration_sequence;
    use crate::types::graph::{Dependency, NodeType, SourceNode};

    fn graph_from(files: &[&str], edges: &[(&str, &str)]) -> SourceGraph {
        let mut g = SourceGraph::new();
        for f in files {
            g.add_node(SourceNode::new(
                NodeId::file(f),
                NodeType::File,
                f.to_string(),
                f.to_string(),
            ));
        }
        for (s, t) in edges {
            g.add_edge(Dependency::new(
                NodeId::file(s),
                NodeId::file(t),
                EdgeType::Imports,
            ));
        }
        g
    }

    fn fp(files: &[(&str, usize)]) -> HashMap<NodeId, usize> {
        files.iter().map(|(f, n)| (NodeId::file(f), *n)).collect()
    }

    fn mech(files: &[&str]) -> HashSet<NodeId> {
        files.iter().map(|f| NodeId::file(f)).collect()
    }

    fn unit_for<'a>(plan: &'a DecompositionPlan, first_member: &str) -> &'a DecompUnit {
        plan.units
            .iter()
            .find(|u| u.members.first().map(String::as_str) == Some(first_member))
            .expect("unit not found")
    }

    #[test]
    fn convex_chain_batches_all() {
        // a→b→c 全机械、预算充裕 → 合成一个 Batch。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 10), ("b", 10), ("c", 10)]),
            &mech(&["a", "b", "c"]),
            100,
        );
        assert_eq!(plan.units.len(), 1, "应合成单个 Batch: {:?}", plan.units);
        assert_eq!(plan.units[0].kind, UnitKind::Batch);
        assert_eq!(plan.units[0].members, vec!["file:a", "file:b", "file:c"]);
        assert_eq!(plan.units[0].footprint, 30);
    }

    #[test]
    fn convexity_blocks_merge_across_external() {
        // a→b→c，b 非机械（外部）→ a 与 c 不能跨 b 合批（破凸性）。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 10), ("b", 10), ("c", 10)]),
            &mech(&["a", "c"]), // b 非机械
            100,
        );
        assert!(
            plan.units.iter().all(|u| u.kind == UnitKind::Single),
            "凸性应阻断跨外部节点合批，全为 Single: {:?}",
            plan.units
        );
        assert_eq!(plan.units.len(), 3);
    }

    #[test]
    fn budget_caps_batch() {
        // a→b→c 全机械，单个 40、预算 100 → 只能两两装箱（b+c），a 溢出单独。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 40), ("b", 40), ("c", 40)]),
            &mech(&["a", "b", "c"]),
            100,
        );
        let batch = unit_for(&plan, "file:b");
        assert_eq!(batch.kind, UnitKind::Batch);
        assert_eq!(batch.members, vec!["file:b", "file:c"]);
        let a = unit_for(&plan, "file:a");
        assert_eq!(a.kind, UnitKind::Single);
    }

    #[test]
    fn over_budget_single_is_manual() {
        let g = graph_from(&["a"], &[]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(&g, &seq, &fp(&[("a", 200)]), &mech(&["a"]), 100);
        assert_eq!(plan.units.len(), 1);
        assert_eq!(plan.units[0].kind, UnitKind::ManualOverBudget);
    }

    #[test]
    fn cycle_group_is_cycle_unit() {
        // a↔b 互引 + 独立 c（机械）。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "a")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 5), ("b", 5), ("c", 5)]),
            &mech(&["a", "b", "c"]),
            100,
        );
        let cyc = unit_for(&plan, "file:a");
        assert_eq!(cyc.kind, UnitKind::Cycle);
        assert_eq!(cyc.members, vec!["file:a", "file:b"]);
    }

    #[test]
    fn non_mechanical_single_not_batched() {
        let g = graph_from(&["a", "b"], &[("a", "b")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 10), ("b", 10)]),
            &mech(&["a"]), // b 非机械
            100,
        );
        assert!(plan.units.iter().all(|u| u.kind == UnitKind::Single));
        assert_eq!(plan.units.len(), 2);
    }

    #[test]
    fn cohesion_mq_single_cohesive_batch() {
        // a→b→c 合成单 batch，内部边 a→b、b→c，无外部 → actual MQ = 1.0。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 10), ("b", 10), ("c", 10)]),
            &mech(&["a", "b", "c"]),
            100,
        );
        let mq = cohesion_mq(&plan, &g, 42, 50);
        assert_eq!(mq.batched_files, 3);
        assert!(
            (mq.actual - 1.0).abs() < 1e-9,
            "纯内部 batch MQ 应为 1.0: {mq:?}"
        );
    }

    #[test]
    fn cohesion_mq_no_batch_is_neutral() {
        let g = graph_from(&["a", "b"], &[("a", "b")]);
        let seq = migration_sequence(&g);
        // b 非机械 → 全 Single，无 batch。
        let plan = plan_decomposition(&g, &seq, &fp(&[("a", 10), ("b", 10)]), &mech(&["a"]), 100);
        let mq = cohesion_mq(&plan, &g, 1, 10);
        assert_eq!(mq.batched_files, 0);
        assert_eq!(mq.ratio, 1.0);
    }

    #[test]
    fn invariants_partition_and_acyclic() {
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let plan = plan_decomposition(
            &g,
            &seq,
            &fp(&[("a", 10), ("b", 10), ("c", 10)]),
            &mech(&["a", "b", "c"]),
            100,
        );
        let inv = verify_invariants(&plan, &g, 3);
        assert!(inv.partition_ok, "每文件恰好一个单元");
        assert!(inv.dag_acyclic, "单元图应无环");
        assert_eq!(inv.over_budget_count, 0);
        assert!(!inv.plan_hash.is_empty());
    }

    #[test]
    fn canonical_hash_deterministic() {
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let seq = migration_sequence(&g);
        let mk = || {
            plan_decomposition(
                &g,
                &seq,
                &fp(&[("a", 10), ("b", 10), ("c", 10)]),
                &mech(&["a", "b", "c"]),
                100,
            )
        };
        assert_eq!(mk().canonical_hash(), mk().canonical_hash());
    }
}
