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
        let mut s = String::new();
        for u in &self.units {
            // kind|footprint|sprint|m1,m2,...  —— units 已按首成员排序、members 已排序。
            s.push_str(&format!(
                "{}|{}|{}|{}\n",
                unit_kind_key(u.kind),
                u.footprint,
                u.sprint,
                u.members.join(",")
            ));
        }
        fingerprint::content_hash(&s)
    }

    /// 模块数缩减统计：before（1 文件=1 模块）vs after（拆解后单元数）。
    pub fn module_count_after(&self) -> usize {
        self.units.len()
    }

    /// 残留机械单文件模块数（应≈0）：kind=Single 且成员单一且属于机械集合的单元数。
    /// 由调用方传入机械集合判定（此处仅提供按 Single+单成员的候选计数辅助见报告层）。
    pub fn batched_file_count(&self) -> usize {
        self.units
            .iter()
            .filter(|u| u.kind == UnitKind::Batch)
            .map(|u| u.members.len())
            .sum()
    }
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
