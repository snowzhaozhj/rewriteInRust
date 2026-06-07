//! tree-sitter 解析 TS 项目 → SourceGraph 构建。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{MigrateError, Result};
use crate::ts_extract::{Extraction, TsExtractor};
use crate::types::common::{NodeId, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, Provenance, SourceNode};

use super::SourceGraph;

/// 从项目根目录构建源码图。
///
/// `root` 应指向包含 TS 源码的目录（如 `src/`）。
/// 扫描所有 `.ts` 文件（排除 `.d.ts`），提取节点和边。
pub fn build_graph(root: &Path) -> Result<SourceGraph> {
    let root = root
        .canonicalize()
        .map_err(|_| MigrateError::FileNotFound(root.to_path_buf()))?;
    let ts_files = collect_ts_files(&root)?;

    if ts_files.is_empty() {
        return Ok(SourceGraph::new());
    }

    let mut extractor = TsExtractor::new();
    let mut graph = SourceGraph::new();
    let mut file_exports: HashMap<String, Vec<String>> = HashMap::new();

    for file_path in &ts_files {
        let rel = make_relative(file_path, &root);
        let source = std::fs::read_to_string(file_path).map_err(MigrateError::Io)?;

        let extraction = match extractor.extract(&source, file_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let file_node_id = format!("file:{rel}");
        add_file_node(&mut graph, &file_node_id, &rel);
        add_symbol_nodes(&mut graph, &source, file_path, &rel, &extraction);

        let exported_names: Vec<String> = extraction.exports.iter().cloned().collect();
        file_exports.insert(rel.clone(), exported_names);

        add_intra_file_edges(&mut graph, &file_node_id, &rel, &extraction);
    }

    add_import_edges(&mut graph, &file_exports, &ts_files, &root);

    Ok(graph)
}

fn collect_ts_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_ts_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_ts_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(MigrateError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MigrateError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "node_modules" || name == ".git" || name == "dist" {
                continue;
            }
            collect_ts_files_recursive(&path, files)?;
        } else if is_ts_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_ts_file(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.ends_with(".ts") && !name.ends_with(".d.ts")
}

fn make_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn add_file_node(graph: &mut SourceGraph, id: &str, rel: &str) {
    graph.add_node(SourceNode {
        id: NodeId::new(id),
        node_type: NodeType::File,
        name: rel.to_string(),
        file_path: rel.to_string(),
        line_range: None,
        is_exported: false,
        complexity: None,
        is_async: false,
        visibility: None,
        is_abstract: false,
        decorators: Vec::new(),
        migration_status: None,
        migration_priority: None,
    });
}

fn add_symbol_nodes(
    graph: &mut SourceGraph,
    source: &str,
    _file_path: &Path,
    rel: &str,
    extraction: &Extraction,
) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::language_typescript())
        .expect("TS language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return,
    };

    walk_for_symbols(graph, tree.root_node(), source, rel, extraction);
}

fn walk_for_symbols(
    graph: &mut SourceGraph,
    node: tree_sitter::Node,
    source: &str,
    rel: &str,
    extraction: &Extraction,
) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = text(name_node, source);
                let is_async = has_child_kind(node, "async");
                let is_exported = extraction.exports.contains(&name);
                let id = format!("function:{rel}:{name}");
                graph.add_node(SourceNode {
                    id: NodeId::new(&id),
                    node_type: NodeType::Function,
                    name: name.clone(),
                    file_path: rel.to_string(),
                    line_range: Some(span(node)),
                    is_exported,
                    complexity: None,
                    is_async,
                    visibility: None,
                    is_abstract: false,
                    decorators: Vec::new(),
                    migration_status: None,
                    migration_priority: None,
                });
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = text(name_node, source);
                let is_exported = extraction.exports.contains(&name);
                let is_abstract = node.kind() == "abstract_class_declaration";
                let class_id = format!("class:{rel}:{name}");
                graph.add_node(SourceNode {
                    id: NodeId::new(&class_id),
                    node_type: NodeType::Class,
                    name: name.clone(),
                    file_path: rel.to_string(),
                    line_range: Some(span(node)),
                    is_exported,
                    complexity: None,
                    is_async: false,
                    visibility: None,
                    is_abstract,
                    decorators: Vec::new(),
                    migration_status: None,
                    migration_priority: None,
                });

                if let Some(body) = node.child_by_field_name("body") {
                    add_class_methods(graph, body, source, rel, &name);
                }

                add_extends_edges(graph, node, source, rel, &class_id);
            }
            return; // 已递归处理 body
        }
        "interface_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = text(name_node, source);
                let is_exported = extraction.exports.contains(&name);
                graph.add_node(SourceNode {
                    id: NodeId::new(format!("interface:{rel}:{name}")),
                    node_type: NodeType::Interface,
                    name,
                    file_path: rel.to_string(),
                    line_range: Some(span(node)),
                    is_exported,
                    complexity: None,
                    is_async: false,
                    visibility: None,
                    is_abstract: false,
                    decorators: Vec::new(),
                    migration_status: None,
                    migration_priority: None,
                });
            }
        }
        "enum_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = text(name_node, source);
                let is_exported = extraction.exports.contains(&name);
                graph.add_node(SourceNode {
                    id: NodeId::new(format!("enum:{rel}:{name}")),
                    node_type: NodeType::Enum,
                    name,
                    file_path: rel.to_string(),
                    line_range: Some(span(node)),
                    is_exported,
                    complexity: None,
                    is_async: false,
                    visibility: None,
                    is_abstract: false,
                    decorators: Vec::new(),
                    migration_status: None,
                    migration_priority: None,
                });
            }
        }
        "export_statement" => {
            if let Some(decl) = node.child_by_field_name("declaration") {
                walk_for_symbols(graph, decl, source, rel, extraction);
                return;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_symbols(graph, child, source, rel, extraction);
    }
}

fn add_class_methods(
    graph: &mut SourceGraph,
    body: tree_sitter::Node,
    source: &str,
    rel: &str,
    class_name: &str,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_definition" || child.kind() == "public_field_definition" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let method_name = text(name_node, source);
                let is_async = has_child_kind(child, "async");
                let method_id = format!("function:{rel}:{class_name}.{method_name}");
                let class_id = format!("class:{rel}:{class_name}");

                graph.add_node(SourceNode {
                    id: NodeId::new(&method_id),
                    node_type: NodeType::Function,
                    name: format!("{class_name}.{method_name}"),
                    file_path: rel.to_string(),
                    line_range: Some(span(child)),
                    is_exported: false,
                    complexity: None,
                    is_async,
                    visibility: None,
                    is_abstract: false,
                    decorators: Vec::new(),
                    migration_status: None,
                    migration_priority: None,
                });

                graph.add_edge(Dependency {
                    source: NodeId::new(&class_id),
                    target: NodeId::new(&method_id),
                    edge_type: EdgeType::Contains,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: None,
                    mapping_notes: None,
                });
            }
        }
    }
}

fn add_extends_edges(
    graph: &mut SourceGraph,
    class_node: tree_sitter::Node,
    source: &str,
    rel: &str,
    class_id: &str,
) {
    let child_count = class_node.child_count();
    for i in 0..child_count {
        let child = match class_node.child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "class_heritage" {
            let clause_count = child.child_count();
            for j in 0..clause_count {
                let clause = match child.child(j) {
                    Some(c) => c,
                    None => continue,
                };
                if clause.kind() == "extends_clause" || clause.kind() == "implements_clause" {
                    let is_implements = clause.kind() == "implements_clause";
                    let tn_count = clause.child_count();
                    for k in 0..tn_count {
                        let type_node = match clause.child(k) {
                            Some(c) => c,
                            None => continue,
                        };
                        if type_node.is_named()
                            && type_node.kind() != "extends"
                            && type_node.kind() != "implements"
                        {
                            let target_name = text(type_node, source);
                            if !target_name.is_empty() {
                                let target_id = find_type_node_id(rel, &target_name);
                                graph.add_edge(Dependency {
                                    source: NodeId::new(class_id),
                                    target: NodeId::new(&target_id),
                                    edge_type: EdgeType::Extends,
                                    provenance: Provenance::TreeSitter,
                                    weight: 1.0,
                                    sub_kind: if is_implements {
                                        Some("implements".to_string())
                                    } else {
                                        None
                                    },
                                    mapping_notes: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn find_type_node_id(rel: &str, name: &str) -> String {
    format!("interface:{rel}:{name}")
}

fn add_intra_file_edges(
    graph: &mut SourceGraph,
    file_node_id: &str,
    rel: &str,
    extraction: &Extraction,
) {
    let file_id = NodeId::new(file_node_id);
    for call in &extraction.calls {
        if let Some(callee) = call.strip_prefix("call:") {
            let target_id = format!("function:{rel}:{callee}");
            graph.add_edge(Dependency {
                source: file_id.clone(),
                target: NodeId::new(&target_id),
                edge_type: EdgeType::Calls,
                provenance: Provenance::TreeSitter,
                weight: 1.0,
                sub_kind: None,
                mapping_notes: None,
            });
        } else if let Some(ctor) = call.strip_prefix("new:") {
            let target_id = format!("class:{rel}:{ctor}");
            graph.add_edge(Dependency {
                source: file_id.clone(),
                target: NodeId::new(&target_id),
                edge_type: EdgeType::Calls,
                provenance: Provenance::TreeSitter,
                weight: 1.0,
                sub_kind: Some("constructor".to_string()),
                mapping_notes: None,
            });
        }
    }
}

fn add_import_edges(
    graph: &mut SourceGraph,
    _file_exports: &HashMap<String, Vec<String>>,
    ts_files: &[PathBuf],
    root: &Path,
) {
    let file_rels: Vec<String> = ts_files.iter().map(|f| make_relative(f, root)).collect();

    let cloned_edges: Vec<(String, Vec<String>)> = {
        let mut result = Vec::new();
        for node in graph.nodes() {
            if node.node_type == NodeType::File {
                let rel = &node.file_path;
                let ts_file_path = root.join(rel);
                let source = match std::fs::read_to_string(&ts_file_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut extractor = TsExtractor::new();
                let extraction = match extractor.extract(&source, &ts_file_path) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                result.push((rel.clone(), extraction.imports.into_iter().collect()));
            }
        }
        result
    };

    for (rel, imports) in &cloned_edges {
        let file_id = format!("file:{rel}");
        for imp in imports {
            if let Some(target_rel) = resolve_import(imp, rel, &file_rels) {
                let target_file_id = format!("file:{target_rel}");
                graph.add_edge(Dependency {
                    source: NodeId::new(&file_id),
                    target: NodeId::new(&target_file_id),
                    edge_type: EdgeType::Imports,
                    provenance: Provenance::TreeSitter,
                    weight: 1.0,
                    sub_kind: None,
                    mapping_notes: None,
                });

                add_cross_file_call_edges(graph, &file_id, &target_rel, imp);
            }
        }
    }
}

fn add_cross_file_call_edges(
    graph: &mut SourceGraph,
    source_file_id: &str,
    target_rel: &str,
    import_spec: &str,
) {
    let parts: Vec<&str> = import_spec.split("<-").collect();
    if parts.len() < 2 {
        return;
    }
    let symbol_part = parts[0];

    let symbols: Vec<&str> = if let Some((_, after)) = symbol_part.split_once(':') {
        vec![after]
    } else {
        vec![symbol_part]
    };

    for sym in symbols {
        if sym.is_empty() || sym == "*" || sym == "default" || sym.starts_with("type:") {
            continue;
        }
        let target_fn_id = format!("function:{target_rel}:{sym}");
        if graph.node_index(&NodeId::new(&target_fn_id)).is_some() {
            graph.add_edge(Dependency {
                source: NodeId::new(source_file_id),
                target: NodeId::new(&target_fn_id),
                edge_type: EdgeType::Calls,
                provenance: Provenance::TreeSitter,
                weight: 1.0,
                sub_kind: None,
                mapping_notes: None,
            });
        }
    }
}

fn resolve_import(import_spec: &str, current_rel: &str, file_rels: &[String]) -> Option<String> {
    let parts: Vec<&str> = import_spec.split("<-").collect();
    if parts.len() < 2 {
        return None;
    }
    let module_path = parts[parts.len() - 1];

    if !module_path.starts_with('.') {
        return None;
    }

    let current_dir = Path::new(current_rel).parent().unwrap_or(Path::new(""));
    let resolved = current_dir.join(module_path);

    let normalized = normalize_path(&resolved);

    let candidates = [
        format!("{normalized}.ts"),
        format!("{normalized}/index.ts"),
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

fn text(node: tree_sitter::Node, source: &str) -> String {
    source.get(node.byte_range()).unwrap_or("").to_string()
}

fn span(node: tree_sitter::Node) -> Span {
    Span {
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

fn has_child_kind(node: tree_sitter::Node, kind: &str) -> bool {
    let mut c = node.walk();
    let found = node.children(&mut c).any(|ch| ch.kind() == kind);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // cli/crates/core -> cli/crates -> cli -> repo root
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    #[test]
    fn build_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let graph = build_graph(&root).unwrap();

        assert!(
            graph
                .node_index(&NodeId::new("file:src/utils.ts"))
                .is_some()
                || graph.node_index(&NodeId::new("file:utils.ts")).is_some(),
            "should have utils.ts node, nodes: {:?}",
            graph.nodes().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn build_empty_dir() {
        let dir = std::env::temp_dir().join("rustmigrate_empty_test");
        let _ = std::fs::create_dir_all(&dir);
        let graph = build_graph(&dir).unwrap();
        assert_eq!(graph.node_count(), 0);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn build_nonexistent_dir() {
        let result = build_graph(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
