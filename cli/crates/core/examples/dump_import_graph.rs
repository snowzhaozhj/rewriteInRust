//! 导出自研源码图的「文件级 import 边 + 环集合」为 JSON，供差分校验 harness 使用。
//!
//! 绕过仍是 `todo!` 的 `graph` CLI 子命令，直调 core API：
//! [`build_graph_ts`] 建图 + [`detect_cycles`] 检测环，
//! 只取 [`EdgeType::Imports`] 边（File → File）。
//!
//! 用法：
//! ```bash
//! cargo run -p rustmigrate-core --example dump_import_graph -- <项目根目录>
//! ```
//!
//! 输出到 stdout（结构见 `DumpOutput`）：
//! - `edges`：`{from, to}` 列表，路径为相对建图根目录的 posix 路径（含扩展名，
//!   归一化口径由下游 compare 脚本统一处理，两侧一致）。
//! - `cycles`：环集合，每个环是一组文件路径。
//!
//! 注意：本 example 只做「忠实导出」，不做路径归一化（去扩展名/去 src 前缀），
//! 归一化在 compare 脚本里对自研图与 oracle 两侧用同一套规则执行，避免口径漂移。

use std::path::PathBuf;
use std::process::ExitCode;

use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::graph::topo::detect_cycles;
use rustmigrate_core::types::graph::{EdgeType, NodeType};
use serde::Serialize;

/// 单条 import 边（文件 → 文件）。
#[derive(Serialize)]
struct EdgeOut {
    /// 导入方文件路径（相对建图根目录）。
    from: String,
    /// 被导入方文件路径（相对建图根目录）。
    to: String,
}

/// 导出结构。
#[derive(Serialize)]
struct DumpOutput {
    /// 建图根目录（命令行传入的原始值）。
    root: String,
    /// File 节点数。
    file_count: usize,
    /// Imports 边总数。
    import_edge_count: usize,
    /// 文件级 import 边集合。
    edges: Vec<EdgeOut>,
    /// 环集合（每个环为一组文件路径）。
    cycles: Vec<Vec<String>>,
}

fn main() -> ExitCode {
    let root_arg = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("用法: dump_import_graph <项目根目录>");
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

    // 提取 Imports 边。NodeId 形如 `file:{rel_path}`，用 file_path() 取出相对路径。
    // 用 BTreeSet 去重（图允许平行边，但 import 关系是集合语义）并保证确定性输出。
    let edge_set: std::collections::BTreeSet<(String, String)> = graph
        .edges()
        .filter(|e| e.edge_type == EdgeType::Imports)
        .filter_map(|e| {
            let from = e.source.file_path()?.to_string();
            let to = e.target.file_path()?.to_string();
            Some((from, to))
        })
        .collect();
    let edges: Vec<EdgeOut> = edge_set
        .into_iter()
        .map(|(from, to)| EdgeOut { from, to })
        .collect();

    // 环集合：每个环转为文件路径列表。
    let mut cycles: Vec<Vec<String>> = detect_cycles(&graph)
        .into_iter()
        .map(|cycle| {
            let mut files: Vec<String> = cycle
                .iter()
                .filter_map(|id| id.file_path().map(|s| s.to_string()))
                .collect();
            files.sort();
            files
        })
        .collect();
    cycles.sort();

    let file_count = graph
        .nodes()
        .filter(|n| n.node_type == NodeType::File)
        .count();

    let out = DumpOutput {
        root: root_arg,
        file_count,
        import_edge_count: edges.len(),
        edges,
        cycles,
    };

    match serde_json::to_string_pretty(&out) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("序列化失败: {e}");
            ExitCode::FAILURE
        }
    }
}
