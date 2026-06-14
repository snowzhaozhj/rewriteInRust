//! 导出自研源码图的「符号级 Calls 边 + Extends/Implements 边」为 JSON，
//! 供符号级精度差分校验 harness（SYMBOL-PRECISION）使用。
//!
//! 与 `dump_import_graph`（文件级 import 图）平行：本 example 直调 core API
//! [`build_graph_ts`] 建图，提取 [`EdgeType::Calls`] 与 [`EdgeType::Extends`] 边，
//! 每条边输出 caller 与 callee 的 (文件路径, 符号名, 符号类型)。
//!
//! 用法：
//! ```bash
//! cargo run -p rustmigrate-core --example dump_symbol_graph -- <项目根目录>
//! ```
//!
//! ## 自研图的边形态（口径对齐关键）
//!
//! - **Calls 边**：`source = file:{rel}`（文件节点，**caller 侧粒度即文件级**——
//!   自研启发式不追踪「调用发生在哪个函数体内」），`target = function:{rel}:{name}`
//!   或 `class:{rel}:{name}`（构造调用，`sub_kind="constructor"`）。
//!   => caller 只有文件路径、没有符号名；callee 有 (文件, 符号名, 符号类型)。
//! - **Extends 边**：`source = class:{rel}:{name}`，`target = interface|class|enum:{rel}:{name}`；
//!   `sub_kind="implements"` 区分 implements，否则为 extends。
//!   => 两端都有 (文件, 符号名, 符号类型)。
//!
//! ## 忠实导出原则
//!
//! 本 example 只做忠实导出，**不做**路径归一化（去扩展名 / 去 index）、
//! 不做符号名归一化。归一化与口径对齐全部留给 compare-symbol.js，
//! 对自研图与 ts-morph oracle 两侧用同一套规则执行，避免口径漂移。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::graph::SourceGraph;
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::{EdgeType, NodeType};
use serde::Serialize;

/// 调用边的一端（callee 侧；caller 侧只有 file）。
#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct SymbolRef {
    /// 文件路径（相对建图根目录，含扩展名）。
    file: String,
    /// 符号名（callee 名；可能含命名空间前缀，归一化留给 compare）。
    symbol: String,
    /// 符号类型（`function` / `class` / `interface` / `enum`）。
    kind: String,
}

/// 一条 Calls 边：caller 文件 → callee 符号。
#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct CallEdgeOut {
    /// 调用方文件路径（自研图 caller 侧只有文件，无符号名）。
    caller_file: String,
    /// 被调方 (文件, 符号名, 符号类型)。
    callee: SymbolRef,
    /// 是否构造调用（`new Foo()`）。
    is_constructor: bool,
}

/// 一条 Extends/Implements 边：子类型 → 基类型/接口。
#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct HeritageEdgeOut {
    /// 子类型 (文件, 符号名, 符号类型)。
    child: SymbolRef,
    /// 父类型/接口 (文件, 符号名, 符号类型)。
    parent: SymbolRef,
    /// `true` = implements，`false` = extends。
    is_implements: bool,
}

/// 导出结构。
#[derive(Serialize)]
struct DumpOutput {
    /// 建图根目录（命令行传入的原始值）。
    root: String,
    /// Calls 边总数。
    call_edge_count: usize,
    /// Extends + Implements 边总数。
    heritage_edge_count: usize,
    /// 符号级 Calls 边集合。
    calls: Vec<CallEdgeOut>,
    /// Extends 边集合（继承）。
    extends: Vec<HeritageEdgeOut>,
    /// Implements 边集合（接口实现）。
    implements: Vec<HeritageEdgeOut>,
}

/// 从 NodeId 解析 (文件, 符号名, 符号类型)。非符号节点（如 File）返回 None。
fn symbol_ref(graph: &SourceGraph, id: &NodeId) -> Option<SymbolRef> {
    let file = id.file_path()?.to_string();
    let symbol = id.symbol_name()?.to_string();
    // 优先用节点自身记录的类型（权威），回退到 ID 前缀解析。
    let kind = graph
        .node_index(id)
        .and_then(|idx| graph.node(idx))
        .map(|n| n.node_type.to_string())
        .or_else(|| id.kind().map(|k| k.to_string()))?;
    Some(SymbolRef { file, symbol, kind })
}

fn main() -> ExitCode {
    let root_arg = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("用法: dump_symbol_graph <项目根目录>");
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

    // --- Calls 边：source 是 File 节点，target 是 Function/Class 符号 ---
    // 用 BTreeSet 去重 + 确定性排序（图允许平行边，调用关系按集合语义处理）。
    let mut call_set: BTreeSet<CallEdgeOut> = BTreeSet::new();
    for e in graph.edges().filter(|e| e.edge_type == EdgeType::Calls) {
        let Some(caller_file) = e.source.file_path().map(|s| s.to_string()) else {
            continue;
        };
        let Some(callee) = symbol_ref(&graph, &e.target) else {
            continue;
        };
        let is_constructor = e.sub_kind.as_deref() == Some("constructor")
            || e.target.kind() == Some(NodeType::Class);
        call_set.insert(CallEdgeOut {
            caller_file,
            callee,
            is_constructor,
        });
    }

    // --- Extends/Implements 边：source 与 target 均为符号节点 ---
    let mut extends_set: BTreeSet<HeritageEdgeOut> = BTreeSet::new();
    let mut implements_set: BTreeSet<HeritageEdgeOut> = BTreeSet::new();
    for e in graph.edges().filter(|e| e.edge_type == EdgeType::Extends) {
        let (Some(child), Some(parent)) =
            (symbol_ref(&graph, &e.source), symbol_ref(&graph, &e.target))
        else {
            continue;
        };
        let is_implements = e.sub_kind.as_deref() == Some("implements");
        let edge = HeritageEdgeOut {
            child,
            parent,
            is_implements,
        };
        if is_implements {
            implements_set.insert(edge);
        } else {
            extends_set.insert(edge);
        }
    }

    let calls: Vec<CallEdgeOut> = call_set.into_iter().collect();
    let extends: Vec<HeritageEdgeOut> = extends_set.into_iter().collect();
    let implements: Vec<HeritageEdgeOut> = implements_set.into_iter().collect();

    let out = DumpOutput {
        root: root_arg,
        call_edge_count: calls.len(),
        heritage_edge_count: extends.len() + implements.len(),
        calls,
        extends,
        implements,
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
