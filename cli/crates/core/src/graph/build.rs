//! 源码图构建——语言无关。
//!
//! 遍历项目目录，通过 `LanguageAdapter` trait 分析每个文件，
//! 组装成完整的 `SourceGraph`。不依赖任何特定语言实现。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::lang::{FileAnalysis, LanguageAdapter};
use crate::types::common::NodeId;
use crate::types::graph::{Dependency, EdgeType, NodeType, Provenance};

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

    for (file_path, adapter_idx) in &files {
        let rel = make_relative(file_path, &root);
        let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;

        let analysis = match adapters[*adapter_idx].analyze_file(&source, &rel) {
            Ok(a) => a,
            Err(e) => {
                graph.warnings.push(format!("解析跳过 {rel}: {e}"));
                continue;
            }
        };

        for node in &analysis.nodes {
            graph.add_node(node.clone());
        }
        for edge in &analysis.edges {
            graph.add_edge(edge.clone());
        }

        file_analyses.insert(rel, analysis);
    }

    // 修正 extends 边的目标 ID（适配器产出时可能用了错误的前缀）
    fixup_extends_targets(&mut graph);

    // 构建跨文件边（Imports + Calls）
    let file_rels: Vec<String> = files.iter().map(|(p, _)| make_relative(p, &root)).collect();
    add_cross_file_edges(&mut graph, &file_analyses, &file_rels);

    Ok(graph)
}

/// 便捷函数：用默认 TypeScript adapter 构建图。
pub fn build_graph_ts(root: &Path) -> Result<SourceGraph> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> =
        vec![Box::new(crate::lang::typescript::TypeScriptAdapter::new())];
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
            if name == "node_modules" || name == ".git" || name == "dist" || name == "target" {
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

/// 修正 extends 边：适配器可能用了 `interface:` 前缀，实际目标可能是 class/enum。
fn fixup_extends_targets(graph: &mut SourceGraph) {
    let extends_edges: Vec<Dependency> = graph
        .edges()
        .filter(|e| e.edge_type == EdgeType::Extends)
        .cloned()
        .collect();

    for edge in extends_edges {
        if graph.node_index(&edge.target).is_some() {
            continue;
        }
        let target_str = edge.target.as_str();
        let parts: Vec<&str> = target_str.splitn(3, ':').collect();
        if parts.len() < 3 {
            continue;
        }
        let (rel, name) = (parts[1], parts[2]);

        let candidates = [format!("class:{rel}:{name}"), format!("enum:{rel}:{name}")];
        for candidate in &candidates {
            if graph.node_index(&NodeId::new(candidate)).is_some() {
                graph.add_edge(Dependency {
                    source: edge.source.clone(),
                    target: NodeId::new(candidate),
                    edge_type: edge.edge_type,
                    provenance: edge.provenance,
                    weight: edge.weight,
                    sub_kind: edge.sub_kind.clone(),
                    mapping_notes: edge.mapping_notes.clone(),
                });
                break;
            }
        }
    }
}

/// 构建跨文件的 Imports 和 Calls 边。
fn add_cross_file_edges(
    graph: &mut SourceGraph,
    analyses: &HashMap<String, FileAnalysis>,
    file_rels: &[String],
) {
    for (rel, analysis) in analyses {
        let file_id = NodeId::new(format!("file:{rel}"));

        // Imports 边
        for import in &analysis.imports {
            if let Some(target_rel) = resolve_import(&import.module_path, rel, file_rels) {
                let target_file_id = NodeId::new(format!("file:{target_rel}"));
                graph.add_edge(Dependency {
                    source: file_id.clone(),
                    target: target_file_id,
                    edge_type: EdgeType::Imports,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: None,
                    mapping_notes: None,
                });
            }
        }

        // Calls 边（跨文件：从当前文件指向其他文件的函数/类）
        for call in &analysis.calls {
            if call.is_constructor {
                let target_id = format!("class:{rel}:{}", call.callee);
                let resolved = if graph.node_index(&NodeId::new(&target_id)).is_some() {
                    Some(NodeId::new(&target_id))
                } else {
                    graph
                        .nodes()
                        .find(|n| n.name == call.callee && n.node_type == NodeType::Class)
                        .map(|n| n.id.clone())
                };
                if let Some(target) = resolved {
                    graph.add_edge(Dependency {
                        source: file_id.clone(),
                        target,
                        edge_type: EdgeType::Calls,
                        provenance: Provenance::TreeSitter,
                        weight: 1.0,
                        sub_kind: Some("constructor".to_string()),
                        mapping_notes: None,
                    });
                }
            } else {
                let target_id = format!("function:{rel}:{}", call.callee);
                if graph.node_index(&NodeId::new(&target_id)).is_some() {
                    graph.add_edge(Dependency {
                        source: file_id.clone(),
                        target: NodeId::new(&target_id),
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

fn resolve_import(module_path: &str, current_rel: &str, file_rels: &[String]) -> Option<String> {
    if !module_path.starts_with('.') {
        return None;
    }

    let current_dir = Path::new(current_rel).parent().unwrap_or(Path::new(""));
    let resolved = current_dir.join(module_path);
    let normalized = normalize_path(&resolved);

    let candidates = [
        format!("{normalized}.ts"),
        format!("{normalized}/index.ts"),
        format!("{normalized}.py"),
        format!("{normalized}.c"),
        format!("{normalized}.go"),
        normalized.clone(),
    ];

    for candidate in &candidates {
        if file_rels.contains(candidate) {
            return Some(candidate.clone());
        }
    }
    None
}

fn normalize_path(path: &Path) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(s) => {
                parts.push(s.to_str().unwrap_or(""));
            }
            _ => {}
        }
    }
    parts.join("/")
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
}
