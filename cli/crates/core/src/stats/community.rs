//! 社区检测与结构偏离度诊断（Tier 1）。
//!
//! 对 File 级耦合图跑 Louvain 社区检测，与目录结构分区比较 NMI/ARI，
//! 输出结构偏离度分数。

use crate::graph::SourceGraph;
use crate::types::graph::{EdgeType, NodeType};
use anyhow::Result;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// 社区检测报告。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommunityReport {
    pub community_count: usize,
    pub directory_group_count: usize,
    pub nmi: f64,
    pub ari: f64,
    pub deviation_score: f64,
    pub file_count: usize,
    pub communities: Vec<CommunityDetail>,
}

/// 单个社区明细。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommunityDetail {
    pub id: usize,
    pub members: Vec<String>,
    pub primary_directory: String,
    pub directory_purity: f64,
}

/// 对源码图跑社区检测，比较社区分区与目录分区的一致性。
///
/// Tier 1 仅使用 File→File 的 Imports 边构建耦合图（Calls/Extends/UsesType
/// 端点多为非 File 节点，需符号→文件投影，留待 Tier 2）。
pub fn detect_community_deviation(graph: &SourceGraph) -> Result<CommunityReport> {
    let file_ids: Vec<String> = graph
        .nodes()
        .filter(|n| n.node_type == NodeType::File)
        .map(|n| n.id.as_str().to_string())
        .collect();

    let file_count = file_ids.len();
    if file_count < 2 {
        return Ok(CommunityReport {
            community_count: file_count,
            directory_group_count: if file_count == 0 { 0 } else { 1 },
            nmi: 1.0,
            ari: 1.0,
            deviation_score: 0.0,
            file_count,
            communities: Vec::new(),
        });
    }

    let file_set: HashSet<&str> = file_ids.iter().map(|s| s.as_str()).collect();
    let mut edge_weights: HashMap<(String, String), f64> = HashMap::new();

    for dep in graph.edges() {
        if dep.edge_type != EdgeType::Imports {
            continue;
        }
        let src = dep.source.as_str();
        let tgt = dep.target.as_str();
        if !file_set.contains(src) || !file_set.contains(tgt) {
            continue;
        }
        if src == tgt {
            continue;
        }
        let pair = if src < tgt {
            (src.to_string(), tgt.to_string())
        } else {
            (tgt.to_string(), src.to_string())
        };
        *edge_weights.entry(pair).or_insert(0.0) += 1.0;
    }

    let communities = louvain(&file_ids, &edge_weights);

    let assigned: usize = communities.iter().map(|c| c.len()).sum();
    debug_assert_eq!(
        assigned, file_count,
        "Louvain 输出应覆盖所有 {file_count} 个文件节点，实际 {assigned}"
    );

    let dir_partition = directory_partition(&file_ids);

    let community_labels = partition_to_labels(&file_ids, &communities);
    let dir_labels = partition_to_labels_from_map(&file_ids, &dir_partition);

    let nmi = compute_nmi(&community_labels, &dir_labels);
    let ari = compute_ari(&community_labels, &dir_labels);
    let deviation_score = 1.0 - nmi;

    let details = build_details(&communities);

    Ok(CommunityReport {
        community_count: communities.len(),
        directory_group_count: dir_partition.values().collect::<HashSet<_>>().len(),
        nmi,
        ari,
        deviation_score,
        file_count,
        communities: details,
    })
}

// ─── Louvain 社区检测（直接操作邻接表，无外部依赖） ─────────────

/// One-level Louvain：贪心模块度局部优化（无超节点聚合阶段）。
/// Tier 1 诊断用途，结果可能比完整 Louvain 更碎片化。
fn louvain(file_ids: &[String], edge_weights: &HashMap<(String, String), f64>) -> Vec<Vec<String>> {
    let n = file_ids.len();
    if n == 0 {
        return Vec::new();
    }

    let id_to_idx: HashMap<&str, usize> = file_ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();

    // 邻接表：adj[i] = [(j, weight), ...]
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut total_weight = 0.0f64;

    for ((src, tgt), &w) in edge_weights {
        if let (Some(&i), Some(&j)) = (id_to_idx.get(src.as_str()), id_to_idx.get(tgt.as_str())) {
            adj[i].push((j, w));
            adj[j].push((i, w));
            total_weight += w;
        }
    }

    if total_weight == 0.0 {
        return file_ids.iter().map(|id| vec![id.clone()]).collect();
    }

    // 每个节点的加权度
    let degree: Vec<f64> = (0..n)
        .map(|i| adj[i].iter().map(|(_, w)| w).sum())
        .collect();

    // 初始：每个节点独立社区
    let mut community: Vec<usize> = (0..n).collect();
    // 社区内部总权重
    let mut sigma_in: Vec<f64> = vec![0.0; n];
    // 社区关联总权重（成员度之和）
    let mut sigma_tot: Vec<f64> = degree.clone();

    let m2 = 2.0 * total_weight;

    loop {
        let mut improved = false;

        for i in 0..n {
            let ci = community[i];
            let ki = degree[i];

            // 计算 i 与各邻居社区的连接权重
            let mut neighbor_comm_weights: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in &adj[i] {
                *neighbor_comm_weights.entry(community[j]).or_insert(0.0) += w;
            }

            // i 与自身社区的内部连接
            let ki_in_own = neighbor_comm_weights.get(&ci).copied().unwrap_or(0.0);

            // 尝试移除 i（sigma_in 按双计数和维护）
            sigma_in[ci] -= 2.0 * ki_in_own;
            sigma_tot[ci] -= ki;
            community[i] = usize::MAX; // 临时标记

            let mut best_comm = ci;
            let mut best_delta = 0.0f64;

            for (&cj, &ki_in_cj) in &neighbor_comm_weights {
                // ΔQ = 2*k_{i,in}/m2 - 2*σ_tot*k_i/m2²（标准 Louvain，m2=2m）
                let delta = 2.0 * ki_in_cj / m2 - 2.0 * sigma_tot[cj] * ki / (m2 * m2);
                if delta > best_delta {
                    best_delta = delta;
                    best_comm = cj;
                }
            }

            // 也考虑留在原社区
            let delta_stay = 2.0 * ki_in_own / m2 - 2.0 * sigma_tot[ci] * ki / (m2 * m2);
            if delta_stay >= best_delta {
                best_comm = ci;
            }

            community[i] = best_comm;
            let ki_in_best = neighbor_comm_weights
                .get(&best_comm)
                .copied()
                .unwrap_or(0.0);
            sigma_in[best_comm] += 2.0 * ki_in_best;
            sigma_tot[best_comm] += ki;

            if best_comm != ci {
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }

    // 收集社区
    let mut comm_members: HashMap<usize, Vec<String>> = HashMap::new();
    for (i, &c) in community.iter().enumerate() {
        comm_members.entry(c).or_default().push(file_ids[i].clone());
    }

    let mut result: Vec<Vec<String>> = comm_members.into_values().collect();
    for members in &mut result {
        members.sort();
    }
    result.sort();
    result
}

// ─── 分区比较工具 ─────────────────────────────────────

/// 按父目录分组。
fn directory_partition(file_ids: &[String]) -> HashMap<String, usize> {
    let mut dir_to_id: HashMap<String, usize> = HashMap::new();
    let mut result: HashMap<String, usize> = HashMap::new();
    let mut next_id = 0usize;

    for fid in file_ids {
        let path_str = fid.strip_prefix("file:").unwrap_or(fid);
        let dir = Path::new(path_str)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let id = *dir_to_id.entry(dir).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            id
        });
        result.insert(fid.clone(), id);
    }
    result
}

fn partition_to_labels(file_ids: &[String], communities: &[Vec<String>]) -> Vec<usize> {
    let mut label_map: HashMap<&str, usize> = HashMap::new();
    for (idx, community) in communities.iter().enumerate() {
        for member in community {
            label_map.insert(member.as_str(), idx);
        }
    }
    file_ids
        .iter()
        .map(|id| *label_map.get(id.as_str()).unwrap_or(&0))
        .collect()
}

fn partition_to_labels_from_map(
    file_ids: &[String],
    partition: &HashMap<String, usize>,
) -> Vec<usize> {
    file_ids
        .iter()
        .map(|id| *partition.get(id).unwrap_or(&0))
        .collect()
}

/// NMI（归一化互信息）：NMI(U,V) = 2 * I(U;V) / (H(U) + H(V))。
fn compute_nmi(labels_a: &[usize], labels_b: &[usize]) -> f64 {
    let n = labels_a.len();
    if n == 0 {
        return 1.0;
    }
    let n_f = n as f64;

    let max_a = labels_a.iter().copied().max().unwrap_or(0) + 1;
    let max_b = labels_b.iter().copied().max().unwrap_or(0) + 1;

    let mut contingency = vec![vec![0usize; max_b]; max_a];
    let mut row_sums = vec![0usize; max_a];
    let mut col_sums = vec![0usize; max_b];

    for i in 0..n {
        contingency[labels_a[i]][labels_b[i]] += 1;
        row_sums[labels_a[i]] += 1;
        col_sums[labels_b[i]] += 1;
    }

    let mut mi = 0.0f64;
    for i in 0..max_a {
        for j in 0..max_b {
            let nij = contingency[i][j] as f64;
            if nij > 0.0 {
                mi += nij / n_f * (nij * n_f / (row_sums[i] as f64 * col_sums[j] as f64)).ln();
            }
        }
    }

    let h_a: f64 = row_sums
        .iter()
        .filter(|&&s| s > 0)
        .map(|&s| {
            let p = s as f64 / n_f;
            -p * p.ln()
        })
        .sum();

    let h_b: f64 = col_sums
        .iter()
        .filter(|&&s| s > 0)
        .map(|&s| {
            let p = s as f64 / n_f;
            -p * p.ln()
        })
        .sum();

    if (h_a + h_b).abs() < 1e-15 {
        return 1.0;
    }

    (2.0 * mi / (h_a + h_b)).clamp(0.0, 1.0)
}

/// ARI（调整兰德指数）。
fn compute_ari(labels_a: &[usize], labels_b: &[usize]) -> f64 {
    let n = labels_a.len();
    if n < 2 {
        return 1.0;
    }

    let max_a = labels_a.iter().copied().max().unwrap_or(0) + 1;
    let max_b = labels_b.iter().copied().max().unwrap_or(0) + 1;

    let mut contingency = vec![vec![0i64; max_b]; max_a];
    let mut row_sums = vec![0i64; max_a];
    let mut col_sums = vec![0i64; max_b];

    for i in 0..n {
        contingency[labels_a[i]][labels_b[i]] += 1;
        row_sums[labels_a[i]] += 1;
        col_sums[labels_b[i]] += 1;
    }

    let comb2 = |x: i64| -> i64 { x * (x - 1) / 2 };

    let sum_comb_nij: i64 = contingency
        .iter()
        .flat_map(|row| row.iter())
        .map(|&nij| comb2(nij))
        .sum();

    let sum_comb_a: i64 = row_sums.iter().map(|&a| comb2(a)).sum();
    let sum_comb_b: i64 = col_sums.iter().map(|&b| comb2(b)).sum();
    let comb_n = comb2(n as i64);

    if comb_n == 0 {
        return 1.0;
    }

    let expected = sum_comb_a as f64 * sum_comb_b as f64 / comb_n as f64;
    let max_index = (sum_comb_a as f64 + sum_comb_b as f64) / 2.0;

    if (max_index - expected).abs() < 1e-15 {
        return if (sum_comb_nij as f64 - expected).abs() < 1e-15 {
            1.0
        } else {
            0.0
        };
    }

    (sum_comb_nij as f64 - expected) / (max_index - expected)
}

fn build_details(communities: &[Vec<String>]) -> Vec<CommunityDetail> {
    communities
        .iter()
        .enumerate()
        .map(|(idx, members)| {
            let mut dir_counts: HashMap<String, usize> = HashMap::new();
            for m in members {
                let path_str = m.strip_prefix("file:").unwrap_or(m);
                let dir = Path::new(path_str)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                *dir_counts.entry(dir).or_insert(0) += 1;
            }
            let primary = dir_counts
                .iter()
                .max_by_key(|(_, &c)| c)
                .map(|(d, _)| d.clone())
                .unwrap_or_default();
            let purity = if members.is_empty() {
                0.0
            } else {
                *dir_counts.get(&primary).unwrap_or(&0) as f64 / members.len() as f64
            };

            CommunityDetail {
                id: idx,
                members: members.clone(),
                primary_directory: primary,
                directory_purity: purity,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nmi_identical_partitions() {
        let a = vec![0, 0, 1, 1, 2, 2];
        let b = vec![0, 0, 1, 1, 2, 2];
        let nmi = compute_nmi(&a, &b);
        assert!((nmi - 1.0).abs() < 1e-9, "相同分区 NMI 应为 1.0: {nmi}");
    }

    #[test]
    fn test_nmi_permuted_labels() {
        let a = vec![0, 0, 1, 1, 2, 2];
        let b = vec![2, 2, 0, 0, 1, 1];
        let nmi = compute_nmi(&a, &b);
        assert!((nmi - 1.0).abs() < 1e-9, "标签置换不影响 NMI: {nmi}");
    }

    #[test]
    fn test_nmi_independent() {
        let a = vec![0, 0, 0, 1, 1, 1];
        let b = vec![0, 1, 0, 1, 0, 1];
        let nmi = compute_nmi(&a, &b);
        assert!(nmi < 0.2, "独立分区 NMI 应接近 0: {nmi}");
    }

    #[test]
    fn test_ari_identical() {
        let a = vec![0, 0, 1, 1, 2, 2];
        let b = vec![0, 0, 1, 1, 2, 2];
        let ari = compute_ari(&a, &b);
        assert!((ari - 1.0).abs() < 1e-9, "相同分区 ARI 应为 1.0: {ari}");
    }

    #[test]
    fn test_ari_random_baseline() {
        let a = vec![0, 0, 0, 1, 1, 1];
        let b = vec![0, 1, 0, 1, 0, 1];
        let ari = compute_ari(&a, &b);
        assert!(ari.abs() < 0.3, "交错分区 ARI 应接近 0（修正后）: {ari}");
    }

    #[test]
    fn test_directory_partition() {
        let files = vec![
            "file:src/a.ts".to_string(),
            "file:src/b.ts".to_string(),
            "file:lib/c.ts".to_string(),
        ];
        let part = directory_partition(&files);
        assert_eq!(part[&files[0]], part[&files[1]]);
        assert_ne!(part[&files[0]], part[&files[2]]);
    }

    #[test]
    fn test_nmi_empty() {
        let nmi = compute_nmi(&[], &[]);
        assert!((nmi - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_ari_single_element() {
        let ari = compute_ari(&[0], &[0]);
        assert!((ari - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_louvain_disconnected() {
        let ids: Vec<String> = (0..4).map(|i| format!("file:f{i}.ts")).collect();
        let edges = HashMap::new();
        let comms = louvain(&ids, &edges);
        assert_eq!(comms.len(), 4, "无边 → 每个节点独立社区");
    }

    #[test]
    fn test_louvain_k2_merges() {
        let ids = vec!["file:a.ts".to_string(), "file:b.ts".to_string()];
        let mut edges = HashMap::new();
        edges.insert((ids[0].clone(), ids[1].clone()), 1.0);
        let comms = louvain(&ids, &edges);
        assert_eq!(comms.len(), 1, "K2 应合并为单社区: {comms:?}");
    }

    #[test]
    fn test_louvain_two_cliques() {
        let ids: Vec<String> = (0..6).map(|i| format!("file:f{i}.ts")).collect();
        let mut edges = HashMap::new();
        // 簇 1: f0-f1-f2 全连接
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            edges.insert((ids[a].clone(), ids[b].clone()), 1.0);
        }
        // 簇 2: f3-f4-f5 全连接
        for (a, b) in [(3, 4), (3, 5), (4, 5)] {
            edges.insert((ids[a].clone(), ids[b].clone()), 1.0);
        }
        // 簇间弱连接
        edges.insert((ids[2].clone(), ids[3].clone()), 0.1);

        let comms = louvain(&ids, &edges);
        assert_eq!(comms.len(), 2, "两个强连通簇应识别为 2 个社区");
    }
}
