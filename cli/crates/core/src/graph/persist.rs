//! SQLite 持久化：读写 source-graph.db。
//!
//! 使用 `schema.sql` 建表，支持完整的 save/load round-trip。
//!
//! 字段落库划分（设计原则见 docs/design/04-toolchain.md § 5.7.1）：
//! 通用且需查询的字段用独立列；类型特有的稀疏字段收进 `extra` JSON 列，
//! 避免节点表列过宽。当前 `extra` 承载 Function/Class 专属的
//! is_async / visibility / is_abstract / decorators，以及 RustTarget
//! 专属的 rust_kind / rust_path / crate_name。

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{MigrateError, Result};
use crate::types::common::{NodeId, Span};
use crate::types::graph::{
    Dependency, EdgeType, NodeType, Provenance, RustKind, SourceNode, Visibility,
};

use super::SourceGraph;

/// 将图写入 SQLite 数据库。
///
/// 使用 `schema.sql` 建表，先清空旧数据再插入，全程事务保护。
pub fn save_to_db(graph: &SourceGraph, db_path: &Path) -> Result<()> {
    let mut conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

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
                node.node_type.to_string(),
                node.name,
                node.file_path,
                node.line_range.map(|s| s.start_line),
                node.line_range.map(|s| s.end_line),
                node.is_exported,
                node.complexity.map(|c| c.to_string()),
                node.migration_status.map(|s| s.to_string()),
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
                edge.edge_type.to_string(),
                edge.provenance.to_string(),
                edge.weight,
                edge.sub_kind.map(|s| s.to_string()),
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
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
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

            let node_type: NodeType = r.node_type.parse().map_err(|_| MigrateError::Graph {
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

            let complexity = parse_or_warn(
                r.complexity.as_deref(),
                "complexity",
                &r.id,
                &mut graph.warnings,
            );
            let migration_status = parse_or_warn(
                r.migration_status.as_deref(),
                "migration_status",
                &r.id,
                &mut graph.warnings,
            );
            let extra = parse_node_extra(r.extra.as_deref(), &r.id, &mut graph.warnings);

            graph.add_node(SourceNode {
                id: NodeId::new(r.id),
                node_type,
                name: r.name,
                file_path: r.file_path,
                line_range,
                is_exported: r.is_exported,
                complexity,
                is_async: extra.is_async,
                visibility: extra.visibility,
                is_abstract: extra.is_abstract,
                decorators: extra.decorators,
                migration_status,
                migration_priority: r.migration_priority,
                rust_kind: extra.rust_kind,
                rust_path: extra.rust_path,
                crate_name: extra.crate_name,
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

            let edge_type: EdgeType = r.edge_type.parse().map_err(|_| MigrateError::Graph {
                message: format!("未知边类型: {}", r.edge_type),
                file: String::new(),
            })?;

            let provenance: Provenance = r.provenance.parse().map_err(|_| MigrateError::Graph {
                message: format!("未知 provenance: {}", r.provenance),
                file: String::new(),
            })?;

            let edge_id = format!("{}→{}", r.source, r.target);
            let sub_kind = parse_or_warn(
                r.sub_kind.as_deref(),
                "sub_kind",
                &edge_id,
                &mut graph.warnings,
            );
            graph.add_edge(Dependency {
                source: NodeId::new(r.source),
                target: NodeId::new(r.target),
                edge_type,
                provenance,
                weight: r.weight,
                sub_kind,
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

// === 节点扩展属性序列化 ===

/// 节点的类型特有扩展属性，持久化到 `nodes.extra` JSON 列。
///
/// 划分原则见 docs/design/04-toolchain.md § 5.7.1：通用且需查询的字段用独立列，
/// 类型特有的稀疏字段收进 extra，避免节点表列过宽。
/// `#[serde(default)]` 保证旧数据缺失的字段（如早期未写入 rust_*）按默认值读取。
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct NodeExtra {
    /// Function/Class 专属。
    is_async: bool,
    #[serde(default, deserialize_with = "lenient_enum")]
    visibility: Option<Visibility>,
    is_abstract: bool,
    decorators: Vec<String>,
    /// RustTarget 专属。
    #[serde(default, deserialize_with = "lenient_enum")]
    rust_kind: Option<RustKind>,
    rust_path: Option<String>,
    crate_name: Option<String>,
}

impl From<&SourceNode> for NodeExtra {
    fn from(node: &SourceNode) -> Self {
        Self {
            is_async: node.is_async,
            visibility: node.visibility,
            is_abstract: node.is_abstract,
            decorators: node.decorators.clone(),
            rust_kind: node.rust_kind,
            rust_path: node.rust_path.clone(),
            crate_name: node.crate_name.clone(),
        }
    }
}

/// 解析枚举值，失败时记录 warning 而非静默丢弃（前向兼容：未来新增枚举变体时旧版本能
/// 感知而非静默降级为 None）。
fn parse_or_warn<T: std::str::FromStr>(
    value: Option<&str>,
    field: &str,
    id: &str,
    warnings: &mut Vec<String>,
) -> Option<T> {
    value.and_then(|s| match s.parse() {
        Ok(v) => Some(v),
        Err(_) => {
            warnings.push(format!("{id} 的 {field} '{s}' 无法识别，已按 None 处理"));
            None
        }
    })
}

/// serde 宽容反序列化器：未知枚举值降级为 None，避免单个字段失败导致整个 struct
/// 反序列化失败（保护 NodeExtra 的其他字段不被级联丢失）。
fn lenient_enum<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    use serde::Deserialize;
    let v: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match v {
        None => Ok(None),
        Some(val) => match serde_json::from_value::<T>(val.clone()) {
            Ok(parsed) => Ok(Some(parsed)),
            Err(_) => {
                eprintln!("[warn] NodeExtra 字段值 '{val}' 无法识别，已按 None 处理");
                Ok(None)
            }
        },
    }
}

/// 序列化节点的扩展属性为 JSON。
fn serialize_node_extra(node: &SourceNode) -> Result<String> {
    Ok(serde_json::to_string(&NodeExtra::from(node))?)
}

/// 解析节点的扩展属性 JSON。
///
/// `extra` 列非空但 JSON 解析失败时，向 `warnings` 记录一条警告（区别于"本就无扩展属性"
/// 的合法情况），避免静默把损坏行当作全默认值丢失数据。
fn parse_node_extra(extra: Option<&str>, node_id: &str, warnings: &mut Vec<String>) -> NodeExtra {
    let Some(json_str) = extra else {
        return NodeExtra::default();
    };
    serde_json::from_str(json_str).unwrap_or_else(|e| {
        warnings.push(format!(
            "节点 {node_id} 的 extra 列 JSON 解析失败（{e}），扩展属性已按默认值忽略"
        ));
        NodeExtra::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build::build_graph_ts;
    use crate::types::common::Complexity;
    use crate::types::state::ModuleStatus;
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
        let mut n = SourceNode::new(
            NodeId::new("file:test.ts"),
            NodeType::File,
            "test.ts".to_string(),
            "test.ts".to_string(),
        );
        n.line_range = Some(Span {
            start_line: 1,
            end_line: 50,
        });
        n.is_exported = true;
        n.complexity = Some(Complexity::Complex);
        n.is_async = true;
        n.visibility = Some(Visibility::Public);
        n.decorators = vec!["deprecated".to_string()];
        n.migration_status = Some(ModuleStatus::Pending);
        n.migration_priority = Some(1);
        graph.add_node(n);

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
        assert_eq!(node.migration_status, Some(ModuleStatus::Pending));
        assert_eq!(node.migration_priority, Some(1));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_preserves_rust_target_attributes() {
        // RustTarget 专属字段（rust_kind / rust_path / crate_name）经 extra JSON
        // round-trip 后应完整保留，而非被静默丢弃。
        let mut graph = SourceGraph::new();
        let mut n = SourceNode::new(
            NodeId::new("rust_target:my_crate::utils::capitalize"),
            NodeType::RustTarget,
            "capitalize".to_string(),
            String::new(),
        );
        n.rust_kind = Some(RustKind::Function);
        n.rust_path = Some("my_crate::utils::capitalize".to_string());
        n.crate_name = Some("my-crate".to_string());
        graph.add_node(n);

        let db_path = temp_db_path("rust_target");
        save_to_db(&graph, &db_path).unwrap();
        let loaded = load_from_db(&db_path).unwrap();

        let node = loaded
            .node(
                loaded
                    .node_index(&NodeId::new("rust_target:my_crate::utils::capitalize"))
                    .unwrap(),
            )
            .unwrap();

        assert_eq!(node.node_type, NodeType::RustTarget);
        assert_eq!(node.rust_kind, Some(RustKind::Function));
        assert_eq!(
            node.rust_path,
            Some("my_crate::utils::capitalize".to_string())
        );
        assert_eq!(node.crate_name, Some("my-crate".to_string()));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn persist_preserves_edges() {
        let mut graph = SourceGraph::new();
        graph.add_node(SourceNode::new(
            NodeId::new("file:a.ts"),
            NodeType::File,
            "a.ts".to_string(),
            "a.ts".to_string(),
        ));
        graph.add_node(SourceNode::new(
            NodeId::new("file:b.ts"),
            NodeType::File,
            "b.ts".to_string(),
            "b.ts".to_string(),
        ));
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
        g1.add_node(SourceNode::new(
            NodeId::new("file:old.ts"),
            NodeType::File,
            "old.ts".to_string(),
            "old.ts".to_string(),
        ));
        save_to_db(&g1, &db_path).unwrap();

        // 第二次写入（不同的图）
        let mut g2 = SourceGraph::new();
        g2.add_node(SourceNode::new(
            NodeId::new("file:new1.ts"),
            NodeType::File,
            "new1.ts".to_string(),
            "new1.ts".to_string(),
        ));
        g2.add_node(SourceNode::new(
            NodeId::new("file:new2.ts"),
            NodeType::File,
            "new2.ts".to_string(),
            "new2.ts".to_string(),
        ));
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
    fn unknown_enum_values_produce_warnings_not_silent_loss() {
        let db_path = temp_db_path("unknown_enum");

        // 写入一个正常的节点
        let mut graph = SourceGraph::new();
        graph.add_node(SourceNode::new(
            NodeId::new("file:test.ts"),
            NodeType::File,
            "test.ts".to_string(),
            "test.ts".to_string(),
        ));
        save_to_db(&graph, &db_path).unwrap();

        // 手动写入不在枚举中的 migration_status 值
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE nodes SET migration_status = 'future_status' WHERE id = 'file:test.ts'",
            [],
        )
        .unwrap();

        let loaded = load_from_db(&db_path).unwrap();
        let node = loaded
            .nodes()
            .find(|n| n.id.as_str() == "file:test.ts")
            .unwrap();
        // 未知值应降级为 None 而非 panic
        assert_eq!(node.migration_status, None);
        // 但应产生 warning
        assert!(
            loaded
                .warnings()
                .iter()
                .any(|w| w.contains("future_status")),
            "未知 migration_status 应产生 warning: {:?}",
            loaded.warnings()
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn load_nonexistent_db_returns_error() {
        let result = load_from_db(Path::new("/nonexistent/path/test.db"));
        assert!(result.is_err());
    }

    #[test]
    fn save_to_invalid_path_returns_error() {
        let graph = SourceGraph::new();
        let result = save_to_db(&graph, Path::new("/nonexistent/dir/test.db"));
        assert!(result.is_err(), "写入不存在的路径应失败");
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

    /// REFAC-05：parse_node_extra 遇到非法 JSON 应返回默认值并在 warnings 中记录含节点 id 的错误。
    #[test]
    fn parse_node_extra_invalid_json_emits_warning_with_node_id() {
        let mut warnings: Vec<String> = Vec::new();
        let result = parse_node_extra(Some("{ 非法json"), "test-node", &mut warnings);
        // 应返回默认值
        assert_eq!(result, NodeExtra::default());
        // warnings 非空
        assert!(!warnings.is_empty(), "应有 warning 记录");
        // warning 文案中含节点 id
        assert!(
            warnings[0].contains("test-node"),
            "warning 应含节点 id，实际: {}",
            warnings[0]
        );
    }
}
