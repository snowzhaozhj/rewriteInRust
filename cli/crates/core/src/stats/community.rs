//! 社区检测与结构偏离度诊断（Tier 1）。
//!
//! 对 File 级耦合图跑 Leiden 社区检测，与目录结构分区比较 NMI/ARI，
//! 输出结构偏离度分数。

use crate::graph::SourceGraph;
use crate::types::graph::{EdgeType, NodeType};
use anyhow::Result;
use graphrs::{algorithms::community::leiden, Edge, Graph, GraphSpecs, Node};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

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

/// 耦合边类型（同 decompose.rs 定义）。
const COUPLING_EDGE_TYPES: [EdgeType; 4] = [
    EdgeType::Imports,
    EdgeType::Calls,
    EdgeType::Extends,
    EdgeType::UsesType,
];

/// 对源码图跑社区检测，比较社区分区与目录分区的一致性。
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
    let mut edge_pairs: HashSet<(String, String)> = HashSet::new();

    for dep in graph.edges() {
        if !COUPLING_EDGE_TYPES.contains(&dep.edge_type) {
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
        edge_pairs.insert(pair);
    }

    let communities = run_leiden(&file_ids, &edge_pairs)?;
    let dir_partition = directory_partition(&file_ids);

    let leiden_labels = partition_to_labels(&file_ids, &communities);
    let dir_labels = partition_to_labels_from_map(&file_ids, &dir_partition);

    let nmi = compute_nmi(&leiden_labels, &dir_labels);
    let ari = compute_ari(&leiden_labels, &dir_labels);
    let deviation_score = 1.0 - nmi;

    let details = build_details(&communities, &dir_partition);

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

fn run_leiden(
    file_ids: &[String],
    edge_pairs: &HashSet<(String, String)>,
) -> Result<Vec<Vec<String>>> {
    let nodes: Vec<Arc<Node<String, f64>>> = file_ids
        .iter()
        .map(|id| Node::from_name_and_attributes(id.clone(), 0.0))
        .collect();

    let edges: Vec<Arc<Edge<String, f64>>> = edge_pairs
        .iter()
        .map(|(src, tgt)| Edge::with_weight(src.clone(), tgt.clone(), 1.0))
        .collect();

    if edges.is_empty() {
        return Ok(file_ids.iter().map(|id| vec![id.clone()]).collect());
    }

    let g = Graph::<String, f64>::new_from_nodes_and_edges(
        nodes,
        edges,
        GraphSpecs::undirected_create_missing(),
    )
    .map_err(|e| anyhow::anyhow!("graphrs 图构建失败: {}", e.message))?;

    let result = leiden::leiden(&g, true, leiden::QualityFunction::CPM, None, None, None)
        .map_err(|e| anyhow::anyhow!("Leiden 算法失败: {}", e.message))?;

    Ok(result
        .into_iter()
        .map(|set| {
            let mut v: Vec<String> = set.into_iter().collect();
            v.sort();
            v
        })
        .collect())
}

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

/// 将社区列表转为 per-node label 向量（按 file_ids 顺序）。
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

fn build_details(
    communities: &[Vec<String>],
    dir_partition: &HashMap<String, usize>,
) -> Vec<CommunityDetail> {
    let mut id_to_dir: HashMap<usize, String> = HashMap::new();
    for (file, &dir_id) in dir_partition {
        id_to_dir.entry(dir_id).or_insert_with(|| {
            let path_str = file.strip_prefix("file:").unwrap_or(file);
            Path::new(path_str)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });
    }

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
}
