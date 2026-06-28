//! 模块拆解引擎（MDR-011）：在 SCC 缩点 DAG 上做"目录优先两阶段凝聚合并"。
//!
//! 把"每文件=一个翻译单元"改为"沿目录/调用簇双轴、在 DAG 约束下凝聚成耦合内聚的翻译单元"，
//! 治理一文件一模块的过度碎片化（推翻原"机械小文件 footprint 装箱"，方案权威见
//! `docs/decisions/011-coupling-agglomerative-decomposition.md`）。本模块是**纯算法**：self_size
//! 与 footprint 由调用方（CLI）计算后传入，便于单测；不读文件系统、不依赖 adapter。
//!
//! 产出 [`DecompositionPlan`] 供 dry-run 报告消费（不进 active dispatch）。

use std::collections::{BTreeSet, HashMap};

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
    /// 单文件模块（无同目录可并对、又无耦合可并，或预算/凸性受限无法合并）。
    Single,
    /// 凝聚簇（≥2 文件，目录优先两阶段按耦合/目录合并，DAG 约束保凸）。
    Batch,
    /// 循环依赖组（多成员 SCC 或自环）——走现有契约重路径，冻结不参与合并。
    Cycle,
    /// 超自身源码预算的单文件——标记转人工，不进自动 dispatch。
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

/// 在 SCC 缩点 DAG 上做"目录优先两阶段凝聚合并"，产出拆解计划（MDR-011）。
///
/// - `seq`：迁移序列（提供 SCC 组、确定性顺序、is_cycle）。
/// - `self_sizes`：每个**文件** NodeId 的自身源码 token（≈bytes/4）——**预算门控对象**。
///   缺失按 0（保守：倾向合并，仍受凸性约束）。
/// - `footprints`：每个文件 NodeId 的 footprint（自身+依赖签名）——仅供单元体积报告。
/// - `budget`：自身源码预算 B（单元成员 self_size 之和上限；单文件超此值 → ManualOverBudget）。
///
/// 算法：① 循环组 → `Cycle`（冻结、不合并，但参与 quotient 拓扑）；② 阶段 1 在同目录内按
/// 耦合权重凝聚（含零耦合同目录对）；③ 阶段 2 跨目录按耦合权重(>0)凝聚；每步约束 self_size
/// 之和 ≤ budget 且当前 quotient 仍凸（`is_convex`，不成环）；④ 收尾分类。凸性/DAG 仅按
/// Imports 边（与 `unit_graph_acyclic` 一致）；耦合权重计 Imports+Calls+Extends+UsesType。
pub fn plan_decomposition(
    graph: &SourceGraph,
    seq: &MigrationSequence,
    self_sizes: &HashMap<NodeId, usize>,
    footprints: &HashMap<NodeId, usize>,
    budget: usize,
) -> DecompositionPlan {
    let groups = &seq.scc_groups;
    let n = groups.len();

    // 文件路径 → 缩点组索引（符号节点的 file_path 也落到其所属文件的组）。
    let mut group_of_path: HashMap<&str, usize> = HashMap::new();
    for (gi, g) in groups.iter().enumerate() {
        for m in &g.members {
            if let Some(p) = m.file_path() {
                group_of_path.insert(p, gi);
            }
        }
    }

    // 组级 Imports 邻接（凸性/DAG 专用，与 unit_graph_acyclic 同源；缩点保证此图无环）。
    let mut gsucc: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    // 组级耦合权重（无向，Imports+Calls+Extends+UsesType 计数；仅作合并优先级打分）。
    let mut gcoup: HashMap<(usize, usize), u64> = HashMap::new();
    for dep in graph.edges() {
        let (sp, tp) = match (dep.source.file_path(), dep.target.file_path()) {
            (Some(s), Some(t)) => (s, t),
            _ => continue,
        };
        let (si, ti) = match (group_of_path.get(sp), group_of_path.get(tp)) {
            (Some(&s), Some(&t)) => (s, t),
            _ => continue,
        };
        if si == ti {
            continue;
        }
        match dep.edge_type {
            EdgeType::Imports => {
                gsucc[si].insert(ti);
                *gcoup.entry((si.min(ti), si.max(ti))).or_default() += 1;
            }
            EdgeType::Calls | EdgeType::Extends | EdgeType::UsesType => {
                *gcoup.entry((si.min(ti), si.max(ti))).or_default() += 1;
            }
            _ => {}
        }
    }

    // 每组（=单文件，循环组除外）的 self_size / footprint 之和。
    let group_sum = |g: &super::topo::SccGroup, tab: &HashMap<NodeId, usize>| -> usize {
        g.members
            .iter()
            .map(|m| tab.get(m).copied().unwrap_or(0))
            .sum()
    };

    // 初始化：每个缩点组一个单元（循环组冻结，不参与合并但占据 quotient 节点）。
    let mut units: Vec<Unit> = groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let mut members: Vec<String> = g.members.iter().map(|m| m.to_string()).collect();
            members.sort();
            Unit {
                gset: BTreeSet::from([gi]),
                alive: true,
                frozen: g.is_cycle,
                self_size: group_sum(g, self_sizes),
                footprint: group_sum(g, footprints),
                sprint: g.sprint,
                dir: unit_dir(&members),
                key: members.join("\n"),
                members,
            }
        })
        .collect();

    // 阶段 1：同目录凝聚（含零耦合同目录对）。阶段 2：跨目录按耦合权重凝聚。
    agglomerate(&mut units, &gsucc, &gcoup, n, budget, MergeAxis::SameDir);
    agglomerate(&mut units, &gsucc, &gcoup, n, budget, MergeAxis::Coupled);

    // 收尾分类。
    let mut out: Vec<DecompUnit> = units
        .into_iter()
        .filter(|u| u.alive)
        .map(|u| {
            let kind = if u.frozen {
                UnitKind::Cycle
            } else if u.members.len() == 1 && u.self_size > budget {
                UnitKind::ManualOverBudget
            } else if u.members.len() >= 2 {
                UnitKind::Batch
            } else {
                UnitKind::Single
            };
            DecompUnit {
                members: u.members,
                kind,
                footprint: u.footprint,
                sprint: u.sprint,
            }
        })
        .collect();

    // 稳定排序：按首成员字典序（units 内 members 已排序）。
    out.sort_by(|a, b| {
        a.members
            .first()
            .map(String::as_str)
            .unwrap_or("")
            .cmp(b.members.first().map(String::as_str).unwrap_or(""))
    });

    DecompositionPlan { units: out }
}

/// 凝聚过程中的可变单元状态。
struct Unit {
    /// 成员缩点组索引集。
    gset: BTreeSet<usize>,
    /// 成员文件 NodeId 串（字典序），首个作 module key 代表。
    members: Vec<String>,
    alive: bool,
    /// 循环组：冻结，不参与合并（但作为 quotient 节点参与凸性）。
    frozen: bool,
    self_size: usize,
    footprint: usize,
    sprint: u32,
    /// 公共直接父目录（混目录 None）。
    dir: Option<String>,
    /// 成员串拼接，决定性破平。
    key: String,
}

/// 合并轴：阶段 1 限同目录、阶段 2 限耦合>0。
#[derive(Clone, Copy, PartialEq)]
enum MergeAxis {
    SameDir,
    Coupled,
}

/// 单元成员的公共直接父目录；不一致返回 None。`members` 为 File NodeId 串。
fn unit_dir(members: &[String]) -> Option<String> {
    let dir_of = |s: &str| -> String {
        // NodeId 形如 `file:path/x.py`；取 file_path 再去掉最后一段。
        let path = NodeId::new(s.to_string());
        let p = path.file_path().unwrap_or("");
        match p.rfind('/') {
            Some(i) => p[..i].to_string(),
            None => String::new(),
        }
    };
    let mut it = members.iter();
    let first = dir_of(it.next()?);
    for m in it {
        if dir_of(m) != first {
            return None;
        }
    }
    Some(first)
}

/// 在当前 `units` 上按给定轴反复合并，直到无合法对。每步：选 (耦合权重 desc, pair_key asc)
/// 首个满足 self_size 预算 + 当前 quotient 凸性 的对，合并；合并后重算 reach（正确性优先）。
fn agglomerate(
    units: &mut [Unit],
    gsucc: &[BTreeSet<usize>],
    gcoup: &HashMap<(usize, usize), u64>,
    n_groups: usize,
    budget: usize,
    axis: MergeAxis,
) {
    loop {
        // 存活单元 → 紧凑索引；组 → 紧凑索引。
        let alive: Vec<usize> = (0..units.len()).filter(|&i| units[i].alive).collect();
        let m = alive.len();
        if m < 2 {
            return;
        }
        let mut compact_of_group = vec![usize::MAX; n_groups];
        for (ai, &ui) in alive.iter().enumerate() {
            for &g in &units[ui].gset {
                compact_of_group[g] = ai;
            }
        }
        // 紧凑单元级 Imports 邻接 + 可达（凸性专用）。
        let mut usucc: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); m];
        for gi in 0..n_groups {
            let ai = compact_of_group[gi];
            if ai == usize::MAX {
                continue;
            }
            for &gj in &gsucc[gi] {
                let aj = compact_of_group[gj];
                if aj != usize::MAX && ai != aj {
                    usucc[ai].insert(aj);
                }
            }
        }
        let reach = transitive_reachability(&usucc);
        // 紧凑单元级耦合权重。
        let mut ucoup: HashMap<(usize, usize), u64> = HashMap::new();
        for (&(gi, gj), &w) in gcoup {
            let ai = compact_of_group[gi];
            let aj = compact_of_group[gj];
            if ai == usize::MAX || aj == usize::MAX || ai == aj {
                continue;
            }
            *ucoup.entry((ai.min(aj), ai.max(aj))).or_default() += w;
        }

        // 候选对生成（紧凑索引 a<b），按轴限定域。
        let mut cands: Vec<(usize, usize)> = Vec::new();
        match axis {
            MergeAxis::SameDir => {
                // 按目录分桶，桶内全对（含零耦合）。
                let mut buckets: HashMap<&str, Vec<usize>> = HashMap::new();
                for (ai, &ui) in alive.iter().enumerate() {
                    if units[ui].frozen {
                        continue;
                    }
                    if let Some(d) = &units[ui].dir {
                        buckets.entry(d.as_str()).or_default().push(ai);
                    }
                }
                for ids in buckets.values() {
                    for i in 0..ids.len() {
                        for j in (i + 1)..ids.len() {
                            cands.push((ids[i].min(ids[j]), ids[i].max(ids[j])));
                        }
                    }
                }
            }
            MergeAxis::Coupled => {
                for &(a, b) in ucoup.keys() {
                    if !units[alive[a]].frozen && !units[alive[b]].frozen {
                        cands.push((a, b));
                    }
                }
            }
        }
        if cands.is_empty() {
            return;
        }

        // 选本轮要合并的紧凑索引对：所有对 units 的不可变借用都限制在本块内，
        // 块结束后借用释放，下面才能可变借用 units 做合并（否则借用检查冲突）。
        let picked: Option<(usize, usize)> = {
            // 排序：耦合权重 desc → pair_key asc（pair_key = 两侧成员串规范化对，破平到底）。
            cands.sort_by(|&(a1, b1), &(a2, b2)| {
                let w = |a: usize, b: usize| ucoup.get(&(a.min(b), a.max(b))).copied().unwrap_or(0);
                let pk = |a: usize, b: usize| -> (&str, &str) {
                    let ka = units[alive[a]].key.as_str();
                    let kb = units[alive[b]].key.as_str();
                    if ka <= kb {
                        (ka, kb)
                    } else {
                        (kb, ka)
                    }
                };
                w(a2, b2)
                    .cmp(&w(a1, b1))
                    .then_with(|| pk(a1, b1).cmp(&pk(a2, b2)))
            });
            // 取首个满足预算 + 凸性 的对。
            let mut chosen = None;
            for &(a, b) in &cands {
                if units[alive[a]].self_size + units[alive[b]].self_size > budget {
                    continue;
                }
                let set: BTreeSet<usize> = BTreeSet::from([a, b]);
                if is_convex(&set, &reach, m) {
                    chosen = Some((a, b));
                    break;
                }
            }
            chosen
        };
        let (a, b) = match picked {
            Some(p) => p,
            None => return,
        };

        // 合并 alive[b] → alive[a]。成员由两单元自身成员列表归并，无需回溯 groups。
        let (ua, ub) = (alive[a], alive[b]);
        let bset = std::mem::take(&mut units[ub].gset);
        let b_members = std::mem::take(&mut units[ub].members);
        let b_self = units[ub].self_size;
        let b_fp = units[ub].footprint;
        let b_sprint = units[ub].sprint;
        let b_dir = units[ub].dir.take();
        units[ub].alive = false;
        units[ua].gset.extend(bset);
        units[ua].self_size += b_self;
        units[ua].footprint += b_fp;
        units[ua].sprint = units[ua].sprint.min(b_sprint);
        // dir 增量 O(1)：两侧同目录则保留，否则 None（等价于对全成员重算 unit_dir，免去 O(n) 重分配）。
        units[ua].dir = match (units[ua].dir.take(), b_dir) {
            (Some(x), Some(y)) if x == y => Some(x),
            _ => None,
        };
        units[ua].members.extend(b_members);
        units[ua].members.sort();
        units[ua].key = units[ua].members.join("\n");
    }
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
    /// 与合批文件相关的耦合边总数（内部+外部）。为 0 = 批内文件零耦合（空/孤立文件），
    /// 此时 ratio 测试退化、无低内聚风险 → 内聚门真空满足（不强求 ≥1.5×）。
    pub coupling_edges: u64,
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
/// 返回 (加权 MQ, 与合批文件相关的耦合边总数)。耦合边数用于内聚门退化判定。
fn weighted_mq(assign: &HashMap<String, usize>, graph: &SourceGraph) -> (f64, u64) {
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
    let mq = if total_edges > 0.0 {
        total_internal / total_edges
    } else {
        0.0
    };
    (mq, total_edges as u64)
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
            coupling_edges: 0,
        };
    }

    let (actual, coupling_edges) = weighted_mq(&assign, graph);

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
        acc += weighted_mq(&shuffled, graph).0;
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
        coupling_edges,
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

    /// 用同一 sizes 同时作 self_sizes 与 footprints 跑拆解（MDR-011 两阶段凝聚）。
    fn plan(g: &SourceGraph, sizes: &HashMap<NodeId, usize>, budget: usize) -> DecompositionPlan {
        let seq = migration_sequence(g);
        plan_decomposition(g, &seq, sizes, sizes, budget)
    }

    fn unit_for<'a>(plan: &'a DecompositionPlan, first_member: &str) -> &'a DecompUnit {
        plan.units
            .iter()
            .find(|u| u.members.first().map(String::as_str) == Some(first_member))
            .expect("unit not found")
    }

    #[test]
    fn same_dir_chain_merges_all() {
        // a→b→c 同目录(根)、预算充裕 → 阶段1 全并成单 Batch（不再依赖"机械"门）。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let plan = plan(&g, &fp(&[("a", 10), ("b", 10), ("c", 10)]), 100);
        assert_eq!(plan.units.len(), 1, "同目录应全并: {:?}", plan.units);
        assert_eq!(plan.units[0].kind, UnitKind::Batch);
        assert_eq!(plan.units[0].members, vec!["file:a", "file:b", "file:c"]);
        assert_eq!(plan.units[0].footprint, 30);
    }

    #[test]
    fn same_dir_independent_merges_all() {
        // funcy 类：同目录、互不 import（零耦合）也应被阶段1 目录轴并起来。
        let g = graph_from(&["a", "b", "c"], &[]); // 无边
        let plan = plan(&g, &fp(&[("a", 10), ("b", 10), ("c", 10)]), 100);
        assert_eq!(plan.units.len(), 1, "零耦合同目录应并: {:?}", plan.units);
        assert_eq!(plan.units[0].kind, UnitKind::Batch);
        assert_eq!(plan.units[0].members, vec!["file:a", "file:b", "file:c"]);
    }

    #[test]
    fn different_dirs_no_coupling_stay_separate() {
        // 退化情形（codex[6]）：不同目录 + 零耦合 → 无候选，正确地保持各自独立。
        let g = graph_from(&["x/a", "y/b"], &[]);
        let plan = plan(&g, &fp(&[("x/a", 10), ("y/b", 10)]), 100);
        assert_eq!(plan.units.len(), 2);
        assert!(plan.units.iter().all(|u| u.kind == UnitKind::Single));
    }

    #[test]
    fn cross_dir_coupling_merges_phase2() {
        // 跨目录但有耦合边 → 阶段2 调用簇轴并起来。
        let g = graph_from(&["pkg/a", "other/b"], &[("pkg/a", "other/b")]);
        let plan = plan(&g, &fp(&[("pkg/a", 10), ("other/b", 10)]), 100);
        assert_eq!(plan.units.len(), 1, "跨目录耦合应并: {:?}", plan.units);
        assert_eq!(plan.units[0].kind, UnitKind::Batch);
        assert_eq!(plan.units[0].members, vec!["file:other/b", "file:pkg/a"]);
    }

    #[test]
    fn budget_caps_merge() {
        // a→b→c 同目录、单个 40、预算 100 → (a,b) 先并(pair_key 破平)，c 因预算溢出独立。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let plan = plan(&g, &fp(&[("a", 40), ("b", 40), ("c", 40)]), 100);
        let batch = unit_for(&plan, "file:a");
        assert_eq!(batch.kind, UnitKind::Batch);
        assert_eq!(batch.members, vec!["file:a", "file:b"]);
        let c = unit_for(&plan, "file:c");
        assert_eq!(c.kind, UnitKind::Single);
    }

    #[test]
    fn convexity_blocks_cyclic_merge() {
        // 1→2、3→4；目录 p={1,4}、q={2,3}。阶段1 先并 {1,4}（凸），随后 {2,3} 虽同目录、
        // 预算够（20≤25），但合并会与 {1,4} 成环 → is_convex 必须阻断，2、3 保持分离。
        let g = graph_from(
            &["p/n1", "q/n2", "q/n3", "p/n4"],
            &[("p/n1", "q/n2"), ("q/n3", "p/n4")],
        );
        let plan = plan(
            &g,
            &fp(&[("p/n1", 10), ("q/n2", 10), ("q/n3", 10), ("p/n4", 10)]),
            25,
        );
        let pq = unit_for(&plan, "file:p/n1");
        assert_eq!(pq.kind, UnitKind::Batch);
        assert_eq!(pq.members, vec!["file:p/n1", "file:p/n4"]);
        // n2、n3 因凸性（非预算）不得并入同一单元。
        let u2 = unit_for(&plan, "file:q/n2");
        let u3 = unit_for(&plan, "file:q/n3");
        assert_eq!(u2.kind, UnitKind::Single);
        assert_eq!(u3.kind, UnitKind::Single);
        let inv = verify_invariants(&plan, &g, 4);
        assert!(inv.dag_acyclic, "单元图必须无环");
        assert!(inv.partition_ok);
    }

    #[test]
    fn over_budget_single_is_manual() {
        let g = graph_from(&["a"], &[]);
        let plan = plan(&g, &fp(&[("a", 200)]), 100);
        assert_eq!(plan.units.len(), 1);
        assert_eq!(plan.units[0].kind, UnitKind::ManualOverBudget);
    }

    #[test]
    fn cycle_group_is_cycle_unit() {
        // a↔b 互引（循环组冻结，不参与合并）+ 独立 c。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "a")]);
        let plan = plan(&g, &fp(&[("a", 5), ("b", 5), ("c", 5)]), 100);
        let cyc = unit_for(&plan, "file:a");
        assert_eq!(cyc.kind, UnitKind::Cycle);
        assert_eq!(cyc.members, vec!["file:a", "file:b"]);
        // c 在根目录但唯一非冻结成员 → 无同目录可并对 → 保持 Single。
        let c = unit_for(&plan, "file:c");
        assert_eq!(c.kind, UnitKind::Single);
    }

    #[test]
    fn cohesion_mq_single_cohesive_batch() {
        // a→b→c 合成单 batch，内部边 a→b、b→c，无外部 → actual MQ = 1.0。
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let plan = plan(&g, &fp(&[("a", 10), ("b", 10), ("c", 10)]), 100);
        let mq = cohesion_mq(&plan, &g, 42, 50);
        assert_eq!(mq.batched_files, 3);
        assert!(
            (mq.actual - 1.0).abs() < 1e-9,
            "纯内部 batch MQ 应为 1.0: {mq:?}"
        );
    }

    #[test]
    fn cohesion_mq_no_batch_is_neutral() {
        // 不同目录 + 无边 → 无 batch。
        let g = graph_from(&["x/a", "y/b"], &[]);
        let plan = plan(&g, &fp(&[("x/a", 10), ("y/b", 10)]), 100);
        let mq = cohesion_mq(&plan, &g, 1, 10);
        assert_eq!(mq.batched_files, 0);
        assert_eq!(mq.ratio, 1.0);
    }

    #[test]
    fn cohesion_mq_zero_coupling_batch_is_neutral() {
        // 同目录、无边 → 阶段1 合批，但 coupling_edges=0（内聚门据此真空满足）。
        let g = graph_from(&["a", "b"], &[]);
        let plan = plan(&g, &fp(&[("a", 0), ("b", 0)]), 100);
        let batch = unit_for(&plan, "file:a");
        assert_eq!(batch.kind, UnitKind::Batch);
        let mq = cohesion_mq(&plan, &g, 1, 10);
        assert_eq!(mq.batched_files, 2);
        assert_eq!(mq.coupling_edges, 0, "孤立文件批耦合边应为 0: {mq:?}");
    }

    #[test]
    fn invariants_partition_and_acyclic() {
        let g = graph_from(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let plan = plan(&g, &fp(&[("a", 10), ("b", 10), ("c", 10)]), 100);
        let inv = verify_invariants(&plan, &g, 3);
        assert!(inv.partition_ok, "每文件恰好一个单元");
        assert!(inv.dag_acyclic, "单元图应无环");
        assert_eq!(inv.over_budget_count, 0);
        assert!(!inv.plan_hash.is_empty());
    }

    #[test]
    fn canonical_hash_deterministic() {
        // 含跨目录 + 多候选平局，验证两阶段凝聚确定性（跑两次字节级一致）。
        let g = graph_from(
            &["p/n1", "q/n2", "q/n3", "p/n4"],
            &[("p/n1", "q/n2"), ("q/n3", "p/n4")],
        );
        let sizes = fp(&[("p/n1", 10), ("q/n2", 10), ("q/n3", 10), ("p/n4", 10)]);
        let h1 = plan(&g, &sizes, 100).canonical_hash();
        let h2 = plan(&g, &sizes, 100).canonical_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn empty_graph_yields_no_units() {
        let g = graph_from(&[], &[]);
        let p = plan(&g, &fp(&[]), 100);
        assert!(p.units.is_empty());
        let inv = verify_invariants(&p, &g, 0);
        assert!(inv.partition_ok);
        assert!(inv.dag_acyclic);
    }

    #[test]
    fn directory_first_resolves_phase_order_sensitivity() {
        // codex[7] 反例：pkg/a 跨目录耦合 other/b，pkg/c 同目录但零耦合。
        // 目录优先两阶段保证 pkg/c 不被遗弃（阶段1 先并 pkg/{a,c}，阶段2 再并 other/b）。
        let g = graph_from(&["pkg/a", "other/b", "pkg/c"], &[("pkg/a", "other/b")]);
        let p = plan(
            &g,
            &fp(&[("pkg/a", 10), ("other/b", 10), ("pkg/c", 10)]),
            100,
        );
        assert_eq!(p.units.len(), 1, "应全并、pkg/c 不残留: {:?}", p.units);
        assert_eq!(p.units[0].kind, UnitKind::Batch);
        assert_eq!(
            p.units[0].members,
            vec!["file:other/b", "file:pkg/a", "file:pkg/c"]
        );
    }
}
