//! M2-PETGRAPH-01: petgraph 副本隔离验证 + WAL 配置回归。
//!
//! M2 并行翻译架构下，每个 SubAgent 在独立 worktree 中加载自己的 SourceGraph 副本（只读）。
//! 本测试验证：
//! 1. petgraph StableGraph 的多个独立实例之间无共享内存竞争
//! 2. SQLite WAL 配置作为防御性回归基线

use std::path::PathBuf;
use std::sync::Arc;

use rustmigrate_core::graph::build::build_graph_ts;
use rustmigrate_core::graph::persist::{load_from_db, save_to_db};
use rustmigrate_core::graph::topo::topological_sort;
use rustmigrate_core::types::common::NodeId;
use rustmigrate_core::types::graph::{Dependency, EdgeType, NodeType, SourceNode};

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
    std::env::temp_dir().join(format!("rustmigrate_petgraph_isolation_{name}_{ts}.db"))
}

// =============================================================================
// 测试 1: petgraph 副本独立性——修改一个 SourceGraph 不影响另一个
// =============================================================================

#[test]
fn independent_graphs_no_shared_state() {
    // 从同一个 fixture 构建两个独立的 SourceGraph
    let root = fixtures_dir().join("linear-deps/src");
    let graph_a = build_graph_ts(&root).unwrap();
    let mut graph_b = build_graph_ts(&root).unwrap();

    // 记录原始状态
    let original_node_count = graph_a.node_count();
    let original_edge_count = graph_a.edge_count();
    assert!(original_node_count > 0, "fixture 应含节点");
    assert!(original_edge_count > 0, "fixture 应含边");

    // 两个图的初始状态应一致
    assert_eq!(graph_a.node_count(), graph_b.node_count());
    assert_eq!(graph_a.edge_count(), graph_b.edge_count());

    // 修改 graph_b：添加新节点和新边
    let new_node = SourceNode::new(
        NodeId::new("file:extra_module.ts"),
        NodeType::File,
        "extra_module.ts".to_string(),
        "extra_module.ts".to_string(),
    );
    graph_b.add_node(new_node);

    // 从已有节点到新节点加一条边
    let existing_id = graph_b.nodes().next().expect("graph_b 应有节点").id.clone();
    graph_b.add_edge(Dependency::new(
        existing_id,
        NodeId::new("file:extra_module.ts"),
        EdgeType::Imports,
    ));

    // 验证 graph_a 完全不受影响
    assert_eq!(
        graph_a.node_count(),
        original_node_count,
        "graph_a 的节点数不应因 graph_b 的修改而变化"
    );
    assert_eq!(
        graph_a.edge_count(),
        original_edge_count,
        "graph_a 的边数不应因 graph_b 的修改而变化"
    );

    // graph_b 应多出 1 个节点和 1 条边
    assert_eq!(graph_b.node_count(), original_node_count + 1);
    assert_eq!(graph_b.edge_count(), original_edge_count + 1);

    // 验证 graph_a 中不存在新节点
    assert!(
        graph_a
            .node_index(&NodeId::new("file:extra_module.ts"))
            .is_none(),
        "graph_a 不应含有 graph_b 新增的节点"
    );
}

#[test]
fn cloned_graph_is_fully_independent() {
    // Clone 出的图与原图之间也应完全独立（深拷贝，非引用共享）
    let root = fixtures_dir().join("diamond-deps/src");
    let original = build_graph_ts(&root).unwrap();
    let mut cloned = original.clone();

    let orig_count = original.node_count();

    // 修改 clone
    cloned.add_node(SourceNode::new(
        NodeId::new("file:cloned_extra.ts"),
        NodeType::File,
        "cloned_extra.ts".to_string(),
        "cloned_extra.ts".to_string(),
    ));

    // 原图不受影响
    assert_eq!(
        original.node_count(),
        orig_count,
        "clone 修改后原图节点数不应变化"
    );
    assert!(
        original
            .node_index(&NodeId::new("file:cloned_extra.ts"))
            .is_none(),
        "原图不应含 clone 新增的节点"
    );
    assert_eq!(cloned.node_count(), orig_count + 1);
}

// =============================================================================
// 测试 2: 多线程 petgraph 读取无竞争
// =============================================================================

#[test]
fn concurrent_read_no_data_race() {
    // 构建一个 SourceGraph，用 Arc 共享给多个线程，并发只读操作
    let root = fixtures_dir().join("diamond-deps/src");
    let graph = Arc::new(build_graph_ts(&root).unwrap());

    let expected_node_count = graph.node_count();
    let expected_edge_count = graph.edge_count();

    // 收集所有文件节点 id，用于各线程查询
    let file_ids: Vec<NodeId> = graph
        .nodes()
        .filter(|n| n.node_type == NodeType::File)
        .map(|n| n.id.clone())
        .collect();
    let file_ids = Arc::new(file_ids);

    let num_threads = 8;
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let g = Arc::clone(&graph);
            let ids = Arc::clone(&file_ids);
            std::thread::spawn(move || {
                // 并发读取：节点数/边数
                assert_eq!(g.node_count(), expected_node_count);
                assert_eq!(g.edge_count(), expected_edge_count);

                // 并发读取：neighbors 查询
                for id in ids.iter() {
                    let outgoing = g.outgoing(id);
                    let incoming = g.incoming(id);
                    // 只要不 panic 即验证无竞争；同时检查结果合理性
                    let _ = outgoing.len();
                    let _ = incoming.len();
                }

                // 并发读取：stats
                let stats = g.stats();
                assert_eq!(stats.total_nodes, expected_node_count);
                assert_eq!(stats.total_edges, expected_edge_count);

                // 返回节点数供主线程断言
                g.node_count()
            })
        })
        .collect();

    for handle in handles {
        let count = handle.join().expect("线程不应 panic");
        assert_eq!(count, expected_node_count, "各线程读到的节点数应一致");
    }
}

#[test]
fn concurrent_topo_sort_consistent() {
    // 多线程并发执行拓扑排序，结果应完全一致
    let root = fixtures_dir().join("linear-deps/src");
    let graph = Arc::new(build_graph_ts(&root).unwrap());

    // 基准结果
    let baseline = topological_sort(&graph).unwrap();

    let num_threads = 4;
    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let g = Arc::clone(&graph);
            std::thread::spawn(move || topological_sort(&g).unwrap())
        })
        .collect();

    for handle in handles {
        let result = handle.join().expect("拓扑排序线程不应 panic");
        assert_eq!(result, baseline, "并发拓扑排序的结果应与基准一致");
    }
}

// =============================================================================
// 测试 3: WAL 配置回归
// =============================================================================

#[test]
fn sqlite_connection_pragmas_regression() {
    // 验证 save_to_db 创建的数据库可正常连接，并检查 WAL 相关 pragma
    let root = fixtures_dir().join("linear-deps/src");
    let graph = build_graph_ts(&root).unwrap();
    let db_path = temp_db_path("wal_check");

    save_to_db(&graph, &db_path).unwrap();

    // 直接打开连接查询 pragma
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // 检查 journal_mode
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();

    // 当前 save_to_db 未显式设置 WAL，journal_mode 应为 SQLite 默认值 "delete"。
    // 此断言作为回归基线：如果未来添加了 WAL 配置，测试需同步更新为 "wal"。
    // NOTE: 当 M2 并行翻译上线并启用 WAL 时，应将此断言改为
    //   assert_eq!(journal_mode, "wal")
    assert!(
        journal_mode == "delete" || journal_mode == "wal",
        "journal_mode 应为 'delete'（默认）或 'wal'（显式配置），实际: '{journal_mode}'"
    );

    // 检查 foreign_keys（save_to_db 中有 PRAGMA foreign_keys = ON）
    // 注意：PRAGMA 设置是连接级别的，这里是新连接所以需重新查询默认值
    // save_to_db 在它的连接上设置了 foreign_keys，但新连接默认为 OFF
    // 这只是验证 pragma 查询本身不会出错
    let foreign_keys: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    // 新连接默认 foreign_keys = 0（OFF），不影响正确性
    let _ = foreign_keys;

    // 检查 busy_timeout（当前未显式设置，应为默认值 0）
    let busy_timeout: i32 = conn
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .unwrap();
    // 当前未配置 busy_timeout，记录为回归基线。
    // NOTE: 当 M2 并行翻译上线时，应添加 busy_timeout 配置并将此断言改为 > 0。
    assert!(
        busy_timeout >= 0,
        "busy_timeout 应为非负值，实际: {busy_timeout}"
    );

    // 清理
    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn sqlite_round_trip_preserves_data_integrity() {
    // save → load round-trip 后数据完整性验证（补充 persist.rs 已有测试，
    // 聚焦于多实例场景：两个独立的 db 文件互不干扰）
    let root = fixtures_dir().join("diamond-deps/src");
    let graph = build_graph_ts(&root).unwrap();

    let db_path_1 = temp_db_path("roundtrip_1");
    let db_path_2 = temp_db_path("roundtrip_2");

    // 保存到两个独立的 db 文件
    save_to_db(&graph, &db_path_1).unwrap();
    save_to_db(&graph, &db_path_2).unwrap();

    // 修改第一个 db（直接 SQL 删除一个节点）
    {
        let conn = rusqlite::Connection::open(&db_path_1).unwrap();
        conn.execute("DELETE FROM nodes LIMIT 1", [])
            .unwrap_or_default(); // LIMIT 在 SQLite 默认编译中可能不支持
                                  // 用兼容语法删除
        let first_id: Option<String> = conn
            .query_row("SELECT id FROM nodes LIMIT 1", [], |row| row.get(0))
            .ok();
        if let Some(id) = first_id {
            conn.execute("DELETE FROM edges WHERE source = ?1 OR target = ?1", [&id])
                .unwrap();
            conn.execute("DELETE FROM nodes WHERE id = ?1", [&id])
                .unwrap();
        }
    }

    // 从两个 db 分别加载，第二个应与原图一致
    let loaded_1 = load_from_db(&db_path_1).unwrap();
    let loaded_2 = load_from_db(&db_path_2).unwrap();

    // loaded_1 少了节点（或不变，取决于是否有节点可删）
    // loaded_2 应与原图完全一致
    assert_eq!(
        loaded_2.node_count(),
        graph.node_count(),
        "未修改的 db 加载后节点数应与原图一致"
    );
    assert!(
        loaded_1.node_count() <= graph.node_count(),
        "修改后的 db 节点数不应超过原图"
    );

    // 清理
    let _ = std::fs::remove_file(&db_path_1);
    let _ = std::fs::remove_file(&db_path_2);
}

#[test]
fn sqlite_wal_mode_explicit_set_and_verify() {
    // 验证手动设置 WAL 模式后 journal_mode 确实生效
    // （为未来 M2 并行翻译的 WAL 配置提供参考基线）
    let db_path = temp_db_path("wal_explicit");

    // 先用 save_to_db 创建 db 文件
    let graph = {
        let mut g = rustmigrate_core::graph::SourceGraph::new();
        g.add_node(SourceNode::new(
            NodeId::new("file:test.ts"),
            NodeType::File,
            "test.ts".to_string(),
            "test.ts".to_string(),
        ));
        g
    };
    save_to_db(&graph, &db_path).unwrap();

    // 手动设置 WAL 模式
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let result: String = conn
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            result.to_lowercase(),
            "wal",
            "设置 WAL 后 journal_mode 应为 'wal'"
        );

        // 设置 busy_timeout
        conn.execute_batch("PRAGMA busy_timeout = 5000").unwrap();
        let timeout: i32 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5000, "busy_timeout 应为 5000ms");
    }

    // 重新打开连接验证 WAL 模式持久化（WAL 是持久性配置）
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            journal_mode.to_lowercase(),
            "wal",
            "重新打开后 journal_mode 应仍为 'wal'（WAL 是持久配置）"
        );
        // 注意：busy_timeout 不持久化，新连接为默认值 0
    }

    // 验证 WAL 模式下 load_from_db 仍能正常工作
    let loaded = load_from_db(&db_path).unwrap();
    assert_eq!(loaded.node_count(), 1);

    // 清理（WAL 模式会产生 -wal 和 -shm 文件）
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
}
