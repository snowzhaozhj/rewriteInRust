//! 对项目源文件做 per-module tier 分档（M2-TIER-01a），输出分布统计。
//!
//! populate-modules 在含环项目上会被拒（环挡 sprint 分配），但 tier 是 per-file、
//! 与环无关。本 example 绕过 populate，直接用 [`build_graph_ts`] 拿 File 节点 +
//! [`detect_tier`] 评估，用于 Sprint F 验收前查看真实项目的 tier 分布。
//!
//! 用法：cargo run -p rustmigrate-core --example dump_tiers -- <项目根目录>

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use rustmigrate_core::detect::detect_tier;
use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::types::graph::NodeType;

fn main() -> ExitCode {
    let root_arg = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("用法: dump_tiers <项目根目录>");
            return ExitCode::FAILURE;
        }
    };
    let root = PathBuf::from(&root_arg);

    let graph = match build_graph_ts(&root) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("建图失败: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut files_by_tier: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut skipped = 0usize;

    for node in graph.nodes_by_type(NodeType::File) {
        let Some(rel) = node.id.file_path() else {
            continue;
        };
        let full = root.join(rel);
        match detect_tier(&full) {
            Ok(tier) => {
                let key = format!("{tier:?}");
                *counts.entry(key.clone()).or_default() += 1;
                files_by_tier.entry(key).or_default().push(rel.to_string());
            }
            Err(e) => {
                eprintln!("跳过 {rel}: {e}");
                skipped += 1;
            }
        }
    }

    let total: usize = counts.values().sum();
    println!("root={root_arg}  total_files={total}  skipped={skipped}");
    for (tier, n) in &counts {
        let pct = if total > 0 {
            (*n as f64) * 100.0 / (total as f64)
        } else {
            0.0
        };
        println!("  {tier:<10} {n:>4}  ({pct:.1}%)");
    }
    // Trivial 样例（验收烟囱测试优先从这些挑）
    if let Some(triv) = files_by_tier.get("Trivial") {
        println!("\nTrivial 样例 (前 15):");
        for f in triv.iter().take(15) {
            println!("  {f}");
        }
    }

    ExitCode::SUCCESS
}
