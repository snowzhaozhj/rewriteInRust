//! 源码图构建——语言无关。
//!
//! 遍历项目目录，通过 `LanguageAdapter` trait 分析每个文件，
//! 组装成完整的 `SourceGraph`。不依赖任何特定语言实现。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::lang::{FileAnalysis, LanguageAdapter};
use crate::types::common::{NodeId, EXCLUDED_DIRS};
use crate::types::graph::{Dependency, EdgeSubKind, EdgeType, NodeType, Provenance, SourceNode};

use super::SourceGraph;

/// 从项目根目录构建源码图。
///
/// `adapters` 是语言适配器列表，每个文件会尝试匹配第一个能处理它的适配器。
pub fn build_graph(root: &Path, adapters: &mut [Box<dyn LanguageAdapter>]) -> Result<SourceGraph> {
    let root = root
        .canonicalize()
        .map_err(|_| MigrateError::FileNotFound(root.to_path_buf()))?;

    let files = collect_source_files(&root, adapters)?;
    if files.is_empty() {
        return Ok(SourceGraph::new());
    }

    let mut graph = SourceGraph::new();
    let mut file_analyses: HashMap<String, FileAnalysis> = HashMap::new();
    let mut all_edges: Vec<Dependency> = Vec::new();

    // 第一遍：添加所有节点，收集所有边
    for (file_path, adapter_idx) in &files {
        let rel = make_relative(file_path, &root);
        let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;

        let analysis = match adapters[*adapter_idx].analyze_file(&source, &rel) {
            Ok(a) => a,
            Err(MigrateError::Parse { .. }) => {
                graph
                    .warnings
                    .push(format!("解析跳过 {rel}: tree-sitter 解析失败"));
                continue;
            }
            Err(e) => return Err(e),
        };

        for node in &analysis.nodes {
            graph.add_node(node.clone());
        }
        all_edges.extend(analysis.edges.iter().cloned());

        file_analyses.insert(rel, analysis);
    }

    // 修正 extends 边的目标 ID（跨文件查找），然后添加所有边
    let fixed_edges = fixup_extends_in_edges(&graph, all_edges);
    for edge in &fixed_edges {
        graph.add_edge(edge.clone());
    }

    // 收集所有 adapter 的解析扩展名（去重 + 排序保证确定性）
    let mut resolve_exts: Vec<&str> = adapters
        .iter()
        .flat_map(|a| a.resolve_extensions().iter().copied())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    resolve_exts.sort();

    // 构建跨文件边（Imports + Calls）
    let file_set: HashSet<String> = files.iter().map(|(p, _)| make_relative(p, &root)).collect();
    add_cross_file_edges(&mut graph, &file_analyses, &file_set, &resolve_exts);

    Ok(graph)
}

/// 便捷函数：用默认 TypeScript adapter 构建图。
pub fn build_graph_ts(root: &Path) -> Result<SourceGraph> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new()?)];
    build_graph(root, &mut adapters)
}

/// 收集所有可被适配器处理的源文件，返回 (路径, 适配器索引)。
fn collect_source_files(
    root: &Path,
    adapters: &[Box<dyn LanguageAdapter>],
) -> Result<Vec<(PathBuf, usize)>> {
    let mut files = Vec::new();
    collect_recursive(root, adapters, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn collect_recursive(
    dir: &Path,
    adapters: &[Box<dyn LanguageAdapter>],
    files: &mut Vec<(PathBuf, usize)>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(MigrateError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MigrateError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if EXCLUDED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_recursive(&path, adapters, files)?;
        } else if let Some(idx) = adapters.iter().position(|a| a.can_handle(&path)) {
            files.push((path, idx));
        }
    }
    Ok(())
}

fn make_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 修正 extends 边：适配器用当前文件路径作为目标前缀，实际目标可能在其他文件。
/// 在边添加到图之前调用（因为 add_edge 会丢弃目标不存在的边）。
fn fixup_extends_in_edges(graph: &SourceGraph, mut edges: Vec<Dependency>) -> Vec<Dependency> {
    for edge in &mut edges {
        if edge.edge_type != EdgeType::Extends {
            continue;
        }
        if graph.node_index(&edge.target).is_some() {
            continue;
        }
        let Some(rel) = edge.target.file_path().map(|s| s.to_owned()) else {
            continue;
        };
        let Some(name) = edge.target.symbol_name().map(|s| s.to_owned()) else {
            continue;
        };

        // 1. 同文件内查找备选类型前缀
        let candidates = [
            NodeId::symbol(NodeType::Class, &rel, &name),
            NodeId::symbol(NodeType::Enum, &rel, &name),
        ];
        let mut resolved = false;
        for candidate in &candidates {
            if graph.node_index(candidate).is_some() {
                edge.target = candidate.clone();
                resolved = true;
                break;
            }
        }
        // 2. 跨文件按名称搜索：仅在全图唯一命中时绑定。
        //    命中多个同名类型则放弃（保持目标为占位 ID，add_edge 会丢弃该边），
        //    避免把 Extends 边连到同名但错误文件的类型。
        if !resolved {
            if let Some(target) = find_unique_node(graph, |n| {
                n.name == name
                    && matches!(
                        n.node_type,
                        NodeType::Interface | NodeType::Class | NodeType::Enum
                    )
            }) {
                edge.target = target;
            }
        }
    }
    edges
}

/// 按谓词在图中查找唯一节点；命中多个时返回 None（避免在歧义时猜测）。
fn find_unique_node<F>(graph: &SourceGraph, pred: F) -> Option<NodeId>
where
    F: Fn(&SourceNode) -> bool,
{
    let mut it = graph.nodes().filter(|n| pred(n));
    let first = it.next()?;
    if it.next().is_some() {
        None
    } else {
        Some(first.id.clone())
    }
}

/// 剥离 callee 的导入基名前缀，得到目标模块内的符号名。
/// `ns.clamp`（base=`ns`）→ `clamp`；`fn`（base=`fn`）→ `fn`。
fn cross_symbol_name<'a>(callee: &'a str, base: &str) -> &'a str {
    callee
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('.'))
        .filter(|s| !s.is_empty())
        .unwrap_or(callee)
}

/// 构建跨文件的 Imports 和 Calls 边。
fn add_cross_file_edges(
    graph: &mut SourceGraph,
    analyses: &HashMap<String, FileAnalysis>,
    file_set: &HashSet<String>,
    resolve_exts: &[&str],
) {
    // 按文件相对路径排序遍历，保证跨文件边的插入顺序确定（analyses 是 HashMap）
    let mut rels: Vec<&String> = analyses.keys().collect();
    rels.sort();
    for rel in rels {
        let analysis = &analyses[rel];
        let file_id = NodeId::file(rel);

        // Imports 边 + 构建导入符号 → 源文件的映射
        let mut import_map: HashMap<String, String> = HashMap::new();
        // 同一本地别名被绑定到不同模块时视为歧义，移除并禁用，避免把调用连到错误模块
        let mut ambiguous: HashSet<String> = HashSet::new();
        for import in &analysis.imports {
            if let Some(target_rel) =
                resolve_import(&import.module_path, rel, file_set, resolve_exts)
            {
                graph.add_edge(Dependency {
                    source: file_id.clone(),
                    target: NodeId::file(&target_rel),
                    edge_type: EdgeType::Imports,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: None,
                    mapping_notes: None,
                });
                for sym in &import.symbols {
                    let local_name = sym.alias.as_deref().unwrap_or(&sym.name);
                    if ambiguous.contains(local_name) {
                        continue;
                    }
                    match import_map.get(local_name) {
                        Some(existing) if existing != &target_rel => {
                            import_map.remove(local_name);
                            ambiguous.insert(local_name.to_string());
                        }
                        _ => {
                            import_map.insert(local_name.to_string(), target_rel.clone());
                        }
                    }
                }
            }
        }

        // Calls 边（跨文件：通过 imports 解析调用目标）
        for call in &analysis.calls {
            let callee_base = call.callee.split('.').next().unwrap_or(&call.callee);
            if call.is_constructor {
                let target_id = NodeId::symbol(NodeType::Class, rel, &call.callee);
                let resolved = if graph.node_index(&target_id).is_some() {
                    Some(target_id)
                } else if let Some(src_file) = import_map.get(callee_base) {
                    // 命名空间构造 `new ns.Foo()`：剥离基名后用 `Foo` 在目标模块查找
                    let sym = cross_symbol_name(&call.callee, callee_base);
                    let cross_id = NodeId::symbol(NodeType::Class, src_file, sym);
                    graph.node_index(&cross_id).is_some().then_some(cross_id)
                } else {
                    // 全局唯一同名兜底（命中多个则放弃，避免连到错误文件）
                    find_unique_node(graph, |n| {
                        n.name == call.callee && n.node_type == NodeType::Class
                    })
                };
                if let Some(target) = resolved {
                    graph.add_edge(Dependency {
                        source: file_id.clone(),
                        target,
                        edge_type: EdgeType::Calls,
                        provenance: Provenance::TreeSitter,
                        weight: 1.0,
                        sub_kind: Some(EdgeSubKind::Constructor),
                        mapping_notes: None,
                    });
                }
            } else {
                // 1. 当前文件的函数
                let target_id = NodeId::symbol(NodeType::Function, rel, &call.callee);
                if graph.node_index(&target_id).is_some() {
                    graph.add_edge(Dependency {
                        source: file_id.clone(),
                        target: target_id,
                        edge_type: EdgeType::Calls,
                        provenance: Provenance::TreeSitter,
                        weight: 1.0,
                        sub_kind: None,
                        mapping_notes: None,
                    });
                } else if let Some(src_file) = import_map.get(callee_base) {
                    // 2. 通过 import 解析到其他文件的函数。
                    //    命名空间调用 `ns.fn()`：剥离基名后用 `fn` 查找目标模块的函数；
                    //    普通导入 `fn()`：cross_symbol_name 返回完整 `fn`。
                    let sym = cross_symbol_name(&call.callee, callee_base);
                    let cross_id = NodeId::symbol(NodeType::Function, src_file, sym);
                    if graph.node_index(&cross_id).is_some() {
                        graph.add_edge(Dependency {
                            source: file_id.clone(),
                            target: cross_id,
                            edge_type: EdgeType::Calls,
                            provenance: Provenance::TreeSitter,
                            weight: 1.0,
                            sub_kind: None,
                            mapping_notes: None,
                        });
                    }
                }
            }
        }
    }
}

fn resolve_import(
    module_path: &str,
    current_rel: &str,
    file_set: &HashSet<String>,
    extensions: &[&str],
) -> Option<String> {
    if !module_path.starts_with('.') {
        return None;
    }

    let current_dir = Path::new(current_rel).parent().unwrap_or(Path::new(""));
    let resolved = current_dir.join(module_path);
    let normalized = normalize_path(&resolved)?;

    // 精确匹配（已带扩展名的路径）
    if file_set.contains(&normalized) {
        return Some(normalized);
    }

    // 按 adapter 提供的扩展名生成候选：{path}.ext, {path}/index.ext
    for ext in extensions {
        // normalized 为空 = import 解析到 src 根目录（如 `__tests__/x.ts` 的 `from ".."`）。
        // 根目录本身不是文件，跳过 `{path}.ext`；barrel 候选不能带前导斜杠，否则
        // `/index.ext` 永不匹配根下的 `index.ext`，导致 `from ".."` 这类 barrel 导入漏边
        // （进而 SCC 断裂、漏报循环依赖）。
        if !normalized.is_empty() {
            let with_ext = format!("{normalized}.{ext}");
            if file_set.contains(&with_ext) {
                return Some(with_ext);
            }
        }
        let barrel = if normalized.is_empty() {
            format!("index.{ext}")
        } else {
            format!("{normalized}/index.{ext}")
        };
        if file_set.contains(&barrel) {
            return Some(barrel);
        }
    }

    None
}

/// 归一化相对路径。路径逃逸项目根时返回 None。
fn normalize_path(path: &Path) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::Normal(s) => {
                parts.push(s.to_str().unwrap_or(""));
            }
            _ => {}
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    #[test]
    fn build_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph_ts(&root).unwrap();

        assert!(
            graph.node_index(&NodeId::new("file:utils.ts")).is_some(),
            "should have utils.ts, nodes: {:?}",
            graph.nodes().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );

        let stats = graph.stats();
        assert!(stats.total_nodes >= 3, "at least 3 file nodes");
        assert!(stats.total_edges > 0, "should have edges");
    }

    #[test]
    fn build_empty_dir() {
        let dir = std::env::temp_dir().join("rustmigrate_empty_test");
        let _ = std::fs::create_dir_all(&dir);
        let graph = build_graph_ts(&dir).unwrap();
        assert_eq!(graph.node_count(), 0);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn build_nonexistent_dir() {
        let result = build_graph_ts(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn build_with_no_adapters() {
        let root = fixtures_dir().join("linear-deps/src");
        let mut adapters: Vec<Box<dyn LanguageAdapter>> = vec![];
        let graph = build_graph(&root, &mut adapters).unwrap();
        assert_eq!(graph.node_count(), 0);
    }

    const TS_EXTS: &[&str] = &["ts", "tsx"];

    fn file_set(files: &[&str]) -> HashSet<String> {
        files.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cross_symbol_name_strips_import_base() {
        assert_eq!(cross_symbol_name("ns.clamp", "ns"), "clamp");
        assert_eq!(cross_symbol_name("clamp", "clamp"), "clamp"); // 非点号：返回完整名
        assert_eq!(cross_symbol_name("a.b.c", "a"), "b.c");
    }

    #[test]
    fn namespace_call_resolves_cross_file() {
        let dir = std::env::temp_dir().join("rustmigrate_ns_call_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("utils.ts"),
            "export function clamp(x: number) { return x; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app.ts"),
            "import * as utils from './utils';\nutils.clamp(1);\n",
        )
        .unwrap();

        let graph = build_graph_ts(&dir).unwrap();
        let has_call = graph.edges().any(|e| {
            e.edge_type == EdgeType::Calls
                && e.source.as_str() == "file:app.ts"
                && e.target.as_str() == "function:utils.ts:clamp"
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            has_call,
            "命名空间调用 utils.clamp() 应解析为跨文件 Calls 边"
        );
    }

    #[test]
    fn resolve_import_parent_dir() {
        let files = file_set(&["utils.ts", "sub/service.ts"]);
        assert_eq!(
            resolve_import("../utils", "sub/service.ts", &files, TS_EXTS),
            Some("utils.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_sibling() {
        let files = file_set(&["a/foo.ts", "a/bar.ts"]);
        assert_eq!(
            resolve_import("./bar", "a/foo.ts", &files, TS_EXTS),
            Some("a/bar.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_index_barrel() {
        let files = file_set(&["shared/index.ts"]);
        assert_eq!(
            resolve_import("./shared", "app.ts", &files, TS_EXTS),
            Some("shared/index.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_parent_to_root_barrel() {
        // 深度 1 目录的文件 `from ".."` 应解析到 src 根的 barrel index.ts
        // （zod 测试文件 `import { z } from ".."` 模式，曾因 `/index.ts` 前导斜杠漏边）。
        let files = file_set(&["index.ts", "__tests__/catch.test.ts"]);
        assert_eq!(
            resolve_import("..", "__tests__/catch.test.ts", &files, TS_EXTS),
            Some("index.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_non_relative_returns_none() {
        let files = file_set(&["express.ts"]);
        assert_eq!(resolve_import("express", "app.ts", &files, TS_EXTS), None);
    }

    #[test]
    fn resolve_import_above_root_no_match() {
        let files = file_set(&["utils.ts"]);
        assert_eq!(
            resolve_import("../../escape", "utils.ts", &files, TS_EXTS),
            None
        );
    }

    #[test]
    fn resolve_import_exact_match_with_extension() {
        let files = file_set(&["lib/helper.ts"]);
        assert_eq!(
            resolve_import("./helper.ts", "lib/app.ts", &files, TS_EXTS),
            Some("lib/helper.ts".to_string())
        );
    }

    #[test]
    fn resolve_import_tsx_extension() {
        let files = file_set(&["components/Button.tsx"]);
        assert_eq!(
            resolve_import("./Button", "components/App.tsx", &files, TS_EXTS),
            Some("components/Button.tsx".to_string())
        );
    }
}
