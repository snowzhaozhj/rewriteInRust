//! SQLite 持久化：读写 source-graph.db。
//!
//! 使用 `schema.sql` 建表，支持完整的 save/load round-trip。
//! 节点的扩展属性（is_async / visibility / is_abstract / decorators）
//! 存储在 `extra` JSON 列中。

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::{MigrateError, Result};
use crate::types::common::{Complexity, NodeId, Span};
use crate::types::graph::{Dependency, EdgeType, NodeType, Provenance, SourceNode, Visibility};

use super::SourceGraph;

/// 将图写入 SQLite 数据库。
///
/// 使用 `schema.sql` 建表，先清空旧数据再插入，全程事务保护。
pub fn save_to_db(graph: &SourceGraph, db_path: &Path) -> Result<()> {
    let mut conn = Connection::open(db_path)?;

    // 建表（IF NOT EXISTS，幂等）
    conn.execute_batch(include_str!("../schema.sql"))?;

    let tx = conn.transaction()?;

    // 清空旧数据（保留 schema_versions / metadata / file_fingerprints）
    tx.execute("DELETE FROM edges", [])?;
    tx.execute("DELETE FROM nodes", [])?;

    // 插入节点
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO nodes \
             (id, node_type, name, file_path, start_line, end_line, \
              is_exported, complexity, migration_status, migration_priority, extra) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;

        for node in graph.nodes() {
            let extra = serialize_node_extra(node)?;
            stmt.execute(params![
                node.id.as_str(),
                node_type_to_str(node.node_type),
                node.name,
                node.file_path,
                node.line_range.map(|s| s.start_line),
                node.line_range.map(|s| s.end_line),
                node.is_exported,
                node.complexity.map(complexity_to_str),
                node.migration_status,
                node.migration_priority,
                extra,
            ])?;
        }
    }

    // 插入边
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO edges \
             (source, target, edge_type, provenance, weight, sub_kind, mapping_notes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;

        for edge in graph.edges() {
            stmt.execute(params![
                edge.source.as_str(),
                edge.target.as_str(),
                edge_type_to_str(edge.edge_type),
                provenance_to_str(edge.provenance),
                edge.weight,
                edge.sub_kind,
                edge.mapping_notes,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

/// 从 SQLite 数据库加载图。
///
/// 文件不存在时返回 `MigrateError::FileNotFound`。
pub fn load_from_db(db_path: &Path) -> Result<SourceGraph> {
    if !db_path.exists() {
        return Err(MigrateError::FileNotFound(db_path.to_path_buf()));
    }

    let conn = Connection::open(db_path)?;
    let mut graph = SourceGraph::new();

    // 加载节点
    {
        let mut stmt = conn.prepare(
            "SELECT id, node_type, name, file_path, start_line, end_line, \
             is_exported, complexity, migration_status, migration_priority, extra \
             FROM nodes",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(NodeRow {
                id: row.get(0)?,
                node_type: row.get(1)?,
                name: row.get(2)?,
                file_path: row.get(3)?,
                start_line: row.get(4)?,
                end_line: row.get(5)?,
                is_exported: row.get(6)?,
                complexity: row.get(7)?,
                migration_status: row.get(8)?,
                migration_priority: row.get(9)?,
                extra: row.get(10)?,
            })
        })?;

        for row in rows {
            let r = row?;

            let node_type = str_to_node_type(&r.node_type).ok_or_else(|| MigrateError::Graph {
                message: format!("未知节点类型: {}", r.node_type),
                file: r.file_path.clone(),
            })?;

            let line_range = match (r.start_line, r.end_line) {
                (Some(s), Some(e)) => Some(Span {
                    start_line: s,
                    end_line: e,
                }),
                _ => None,
            };

            let complexity = r.complexity.as_deref().and_then(str_to_complexity);
            let (is_async, visibility, is_abstract, decorators) =
                parse_node_extra(r.extra.as_deref());

            graph.add_node(SourceNode {
                id: NodeId::new(r.id),
                node_type,
                name: r.name,
                file_path: r.file_path,
                line_range,
                is_exported: r.is_exported,
                complexity,
                is_async,
                visibility,
                is_abstract,
                decorators,
                migration_status: r.migration_status,
                migration_priority: r.migration_priority,
                rust_kind: None,
                rust_path: None,
                crate_name: None,
            });
        }
    }

    // 加载边
    {
        let mut stmt = conn.prepare(
            "SELECT source, target, edge_type, provenance, weight, sub_kind, mapping_notes FROM edges",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(EdgeRow {
                source: row.get(0)?,
                target: row.get(1)?,
                edge_type: row.get(2)?,
                provenance: row.get(3)?,
                weight: row.get(4)?,
                sub_kind: row.get(5)?,
                mapping_notes: row.get(6)?,
            })
        })?;

        for row in rows {
            let r = row?;

            let edge_type = str_to_edge_type(&r.edge_type).ok_or_else(|| MigrateError::Graph {
                message: format!("未知边类型: {}", r.edge_type),
                file: String::new(),
            })?;

            let provenance =
                str_to_provenance(&r.provenance).ok_or_else(|| MigrateError::Graph {
                    message: format!("未知 provenance: {}", r.provenance),
                    file: String::new(),
                })?;

            graph.add_edge(Dependency {
                source: NodeId::new(r.source),
                target: NodeId::new(r.target),
                edge_type,
                provenance,
                weight: r.weight,
                sub_kind: r.sub_kind,
                mapping_notes: r.mapping_notes,
            });
        }
    }

    Ok(graph)
}

// === 内部行结构 ===

/// 节点行（从 SQLite 读取的原始数据）。
struct NodeRow {
    id: String,
    node_type: String,
    name: String,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    is_exported: bool,
    complexity: Option<String>,
    migration_status: Option<String>,
    migration_priority: Option<u32>,
    extra: Option<String>,
}

/// 边行（从 SQLite 读取的原始数据）。
struct EdgeRow {
    source: String,
    target: String,
    edge_type: String,
    provenance: String,
    weight: f64,
    sub_kind: Option<String>,
    mapping_notes: Option<String>,
}

// === 类型 ↔ 字符串转换 ===

fn node_type_to_str(nt: NodeType) -> &'static str {
    match nt {
        NodeType::File => "File",
        NodeType::Module => "Module",
        NodeType::Package => "Package",
        NodeType::Function => "Function",
        NodeType::Class => "Class",
        NodeType::Interface => "Interface",
        NodeType::Enum => "Enum",
        NodeType::RustTarget => "RustTarget",
        NodeType::TestFixture => "TestFixture",
        NodeType::TypeAlias => "TypeAlias",
        NodeType::Variable => "Variable",
    }
}

fn str_to_node_type(s: &str) -> Option<NodeType> {
    match s {
        "File" => Some(NodeType::File),
        "Module" => Some(NodeType::Module),
        "Package" => Some(NodeType::Package),
        "Function" => Some(NodeType::Function),
        "Class" => Some(NodeType::Class),
        "Interface" => Some(NodeType::Interface),
        "Enum" => Some(NodeType::Enum),
        "RustTarget" => Some(NodeType::RustTarget),
        "TestFixture" => Some(NodeType::TestFixture),
        "TypeAlias" => Some(NodeType::TypeAlias),
        "Variable" => Some(NodeType::Variable),
        _ => None,
    }
}

fn edge_type_to_str(et: EdgeType) -> &'static str {
    match et {
        EdgeType::Contains => "contains",
        EdgeType::Imports => "imports",
        EdgeType::Calls => "calls",
        EdgeType::Extends => "extends",
        EdgeType::UsesType => "uses_type",
        EdgeType::Exports => "exports",
        EdgeType::MapsTo => "maps_to",
        EdgeType::TestedBy => "tested_by",
    }
}

fn str_to_edge_type(s: &str) -> Option<EdgeType> {
    match s {
        "contains" => Some(EdgeType::Contains),
        "imports" => Some(EdgeType::Imports),
        "calls" => Some(EdgeType::Calls),
        "extends" => Some(EdgeType::Extends),
        "uses_type" => Some(EdgeType::UsesType),
        "exports" => Some(EdgeType::Exports),
        "maps_to" => Some(EdgeType::MapsTo),
        "tested_by" => Some(EdgeType::TestedBy),
        _ => None,
    }
}

fn provenance_to_str(p: Provenance) -> &'static str {
    match p {
        Provenance::TreeSitter => "tree-sitter",
        Provenance::ToolAssisted => "tool-assisted",
        Provenance::Llm => "llm",
        Provenance::Manual => "manual",
    }
}

fn str_to_provenance(s: &str) -> Option<Provenance> {
    match s {
        "tree-sitter" => Some(Provenance::TreeSitter),
        "tool-assisted" => Some(Provenance::ToolAssisted),
        "llm" => Some(Provenance::Llm),
        "manual" => Some(Provenance::Manual),
        _ => None,
    }
}

fn complexity_to_str(c: Complexity) -> &'static str {
    match c {
        Complexity::Simple => "simple",
        Complexity::Moderate => "moderate",
        Complexity::Complex => "complex",
    }
}

fn str_to_complexity(s: &str) -> Option<Complexity> {
    match s {
        "simple" => Some(Complexity::Simple),
        "moderate" => Some(Complexity::Moderate),
        "complex" => Some(Complexity::Complex),
        _ => None,
    }
}

// === 节点扩展属性序列化 ===

/// 序列化节点的扩展属性为 JSON（is_async / visibility / is_abstract / decorators）。
fn serialize_node_extra(node: &SourceNode) -> Result<String> {
    let extra = serde_json::json!({
        "is_async": node.is_async,
        "is_abstract": node.is_abstract,
        "visibility": node.visibility,
        "decorators": node.decorators,
    });
    Ok(serde_json::to_string(&extra)?)
}

/// 解析节点的扩展属性 JSON。
fn parse_node_extra(extra: Option<&str>) -> (bool, Option<Visibility>, bool, Vec<String>) {
    let default = (false, None, false, Vec::new());
    let Some(json_str) = extra else {
        return default;
    };

    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return default;
    };

    let is_async = value
        .get("is_async")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let is_abstract = value
        .get("is_abstract")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let visibility: Option<Visibility> = value
        .get("visibility")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let decorators: Vec<String> = value
        .get("decorators")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (is_async, visibility, is_abstract, decorators)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build::build_graph_ts;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest.ancestors().nth(3).unwrap();
        repo_root.join("fixtures")
    }

    /// 生成唯一的临时数据库路径。
    fn temp_db_path(name: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("rustmigrate_test_{name}_{ts}.db"))
    }

    /// 计算图中唯一的 (source, target, edge_type) 三元组数量。
    ///
    /// DB 的 PRIMARY KEY 会去重重复的边，所以 round-trip 后
    /// 边数等于唯一三元组数而非原始边数。
    fn unique_edge_count(graph: &SourceGraph) -> usize {
        let mut set = std::collections::HashSet::new();
        for edge in graph.edges() {
            set.insert((
                edge.source.as_str().to_string(),
                edge.target.as_str().to_string(),
                format!("{}", edge.edge_type),
            ));
        }
        set.len()
    }

    #[test]
    fn persist_round_trip_linear_deps() {
        let root = fixtures_dir().join("linear-deps/src");
        let original = build_graph_ts(&root).unwrap();
        let db_path = temp_db_path("linear");

        // 保存
        save_to_db(&original, &db_path).unwrap();

        // 加载
        let loaded = load_from_db(&db_path).unwrap();

        // 节点数应完全一致
        assert_eq!(
            loaded.node_count(),
            original.node_count(),
            "节点数应一致: 原始={}, 加载={}",
            original.node_count(),
            loaded.node_count()
        );

        // 边数 = 原始图中唯一 (source, target, edge_type) 三元组数
        // （DB PRIMARY KEY 去重）
        let expected_edges = unique_edge_count(&original);
        assert_eq!(
            loaded.edge_count(),
            expected_edges,
            "边数应等于去重后的三元组数: 预期={expected_edges}, 实际={}",
            loaded.edge_count()
        );

        // 验证所有原始节点都存在
        for node in original.nodes() {
            assert!(
                loaded.node_index(&node.id).is_some(),
                "加载后应包含节点: {}",
                node.id
            );
        }

        // 清理
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_round_trip_diamond_deps() {
        let root = fixtures_dir().join("diamond-deps/src");
        let original = build_graph_ts(&root).unwrap();
        let db_path = temp_db_path("diamond");

        save_to_db(&original, &db_path).unwrap();
        let loaded = load_from_db(&db_path).unwrap();

        assert_eq!(loaded.node_count(), original.node_count());

        let expected_edges = unique_edge_count(&original);
        assert_eq!(loaded.edge_count(), expected_edges);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_preserves_node_attributes() {
        // 构建一个带扩展属性的节点的图
        let mut graph = SourceGraph::new();
        graph.add_node(SourceNode {
            id: NodeId::new("file:test.ts"),
            node_type: NodeType::File,
            name: "test.ts".to_string(),
            file_path: "test.ts".to_string(),
            line_range: Some(Span {
                start_line: 1,
                end_line: 50,
            }),
            is_exported: true,
            complexity: Some(Complexity::Complex),
            is_async: true,
            visibility: Some(Visibility::Public),
            is_abstract: false,
            decorators: vec!["deprecated".to_string()],
            migration_status: Some("pending".to_string()),
            migration_priority: Some(1),
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });

        let db_path = temp_db_path("attrs");
        save_to_db(&graph, &db_path).unwrap();
        let loaded = load_from_db(&db_path).unwrap();

        let node = loaded
            .node(loaded.node_index(&NodeId::new("file:test.ts")).unwrap())
            .unwrap();

        assert_eq!(node.name, "test.ts");
        assert_eq!(node.node_type, NodeType::File);
        assert_eq!(
            node.line_range,
            Some(Span {
                start_line: 1,
                end_line: 50
            })
        );
        assert!(node.is_exported);
        assert_eq!(node.complexity, Some(Complexity::Complex));
        assert!(node.is_async);
        assert_eq!(node.visibility, Some(Visibility::Public));
        assert!(!node.is_abstract);
        assert_eq!(node.decorators, vec!["deprecated"]);
        assert_eq!(node.migration_status, Some("pending".to_string()));
        assert_eq!(node.migration_priority, Some(1));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_preserves_edges() {
        let mut graph = SourceGraph::new();
        graph.add_node(SourceNode {
            id: NodeId::new("file:a.ts"),
            node_type: NodeType::File,
            name: "a.ts".to_string(),
            file_path: "a.ts".to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
        graph.add_node(SourceNode {
            id: NodeId::new("file:b.ts"),
            node_type: NodeType::File,
            name: "b.ts".to_string(),
            file_path: "b.ts".to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
        graph.add_edge(Dependency {
            source: NodeId::new("file:a.ts"),
            target: NodeId::new("file:b.ts"),
            edge_type: EdgeType::Imports,
            provenance: Provenance::TreeSitter,
            weight: 2.5,
            sub_kind: None,
            mapping_notes: None,
        });

        let db_path = temp_db_path("edges");
        save_to_db(&graph, &db_path).unwrap();
        let loaded = load_from_db(&db_path).unwrap();

        assert_eq!(loaded.node_count(), 2);
        assert_eq!(loaded.edge_count(), 1);

        let edge = loaded.edges().next().unwrap();
        assert_eq!(edge.edge_type, EdgeType::Imports);
        assert_eq!(edge.provenance, Provenance::TreeSitter);
        assert!((edge.weight - 2.5).abs() < f64::EPSILON);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_overwrite_existing_data() {
        let db_path = temp_db_path("overwrite");

        // 第一次写入
        let mut g1 = SourceGraph::new();
        g1.add_node(SourceNode {
            id: NodeId::new("file:old.ts"),
            node_type: NodeType::File,
            name: "old.ts".to_string(),
            file_path: "old.ts".to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
        save_to_db(&g1, &db_path).unwrap();

        // 第二次写入（不同的图）
        let mut g2 = SourceGraph::new();
        g2.add_node(SourceNode {
            id: NodeId::new("file:new1.ts"),
            node_type: NodeType::File,
            name: "new1.ts".to_string(),
            file_path: "new1.ts".to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
        g2.add_node(SourceNode {
            id: NodeId::new("file:new2.ts"),
            node_type: NodeType::File,
            name: "new2.ts".to_string(),
            file_path: "new2.ts".to_string(),
            line_range: None,
            is_exported: false,
            complexity: None,
            is_async: false,
            visibility: None,
            is_abstract: false,
            decorators: Vec::new(),
            migration_status: None,
            migration_priority: None,
            rust_kind: None,
            rust_path: None,
            crate_name: None,
        });
        save_to_db(&g2, &db_path).unwrap();

        // 加载应只包含第二次写入的数据
        let loaded = load_from_db(&db_path).unwrap();
        assert_eq!(loaded.node_count(), 2, "应只包含新写入的 2 个节点");
        assert!(loaded.node_index(&NodeId::new("file:old.ts")).is_none());
        assert!(loaded.node_index(&NodeId::new("file:new1.ts")).is_some());
        assert!(loaded.node_index(&NodeId::new("file:new2.ts")).is_some());

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn load_nonexistent_db_returns_error() {
        let result = load_from_db(Path::new("/nonexistent/path/test.db"));
        assert!(result.is_err());
    }

    #[test]
    fn persist_empty_graph() {
        let db_path = temp_db_path("empty");
        let graph = SourceGraph::new();

        save_to_db(&graph, &db_path).unwrap();
        let loaded = load_from_db(&db_path).unwrap();

        assert_eq!(loaded.node_count(), 0);
        assert_eq!(loaded.edge_count(), 0);

        let _ = std::fs::remove_file(&db_path);
    }
}
