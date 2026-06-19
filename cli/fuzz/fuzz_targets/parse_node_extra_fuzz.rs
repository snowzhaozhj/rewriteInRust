//! Fuzz target: 随机字节作为 JSON 字符串输入，通过 load_from_db 间接测试 parse_node_extra。
//!
//! parse_node_extra 是 persist 模块的私有函数，这里通过构造包含随机 extra 字段的
//! SQLite 数据库，用公开的 load_from_db 读取来间接触发。验证面对畸形 JSON 不会 panic，
//! 应正常返回默认值并记录 warning。
//!
//! 手动跑 24h 全量 fuzz：
//!   cd cli/fuzz
//!   cargo +nightly fuzz run parse_node_extra_fuzz -- -max_total_time=86400
//!
//! 快速冒烟（10 秒）：
//!   cargo +nightly fuzz run parse_node_extra_fuzz -- -max_total_time=10

#![no_main]

use libfuzzer_sys::fuzz_target;
use rusqlite::Connection;
use rustmigrate_core::graph::persist::load_from_db;

fuzz_target!(|data: &[u8]| {
    // 将随机字节解释为 UTF-8 字符串，用作 extra JSON 列
    let extra_json = String::from_utf8_lossy(data);

    // 创建临时数据库并写入包含随机 extra 的节点
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let db_path = dir.path().join("fuzz.db");

    {
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        // 建表（与生产代码使用相同的 schema）
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_versions (
                version    TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT OR IGNORE INTO schema_versions (version) VALUES ('0.1');

            CREATE TABLE IF NOT EXISTS nodes (
                id                 TEXT PRIMARY KEY,
                node_type          TEXT NOT NULL,
                name               TEXT NOT NULL,
                file_path          TEXT NOT NULL,
                start_line         INTEGER,
                end_line           INTEGER,
                is_exported        BOOLEAN DEFAULT FALSE,
                complexity         TEXT DEFAULT 'moderate',
                migration_status   TEXT,
                migration_priority INTEGER,
                extra              JSON
            );

            CREATE TABLE IF NOT EXISTS edges (
                source    TEXT NOT NULL REFERENCES nodes(id),
                target    TEXT NOT NULL REFERENCES nodes(id),
                edge_type TEXT NOT NULL,
                provenance TEXT NOT NULL DEFAULT 'tree_sitter',
                weight    REAL DEFAULT 1.0,
                sub_kind  TEXT,
                mapping_notes TEXT,
                PRIMARY KEY (source, target, edge_type)
            );

            CREATE TABLE IF NOT EXISTS file_fingerprints (
                file_path      TEXT PRIMARY KEY,
                content_hash   TEXT NOT NULL,
                structure_hash TEXT NOT NULL,
                analyzed_at    TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS metadata (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            INSERT OR IGNORE INTO metadata (key, value) VALUES ('graph_integrity', 'full');",
        )
        .expect("建表失败");

        // 插入一个节点，extra 列使用 fuzzer 生成的随机数据
        conn.execute(
            "INSERT INTO nodes (id, node_type, name, file_path, start_line, end_line, \
             is_exported, complexity, migration_status, migration_priority, extra) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                "function:fuzz.ts:test_fn",
                "Function",
                "test_fn",
                "fuzz.ts",
                1,
                10,
                false,
                "moderate",
                rusqlite::types::Null,
                rusqlite::types::Null,
                extra_json.as_ref(),
            ],
        )
        .expect("插入节点失败");
    }

    // 通过 load_from_db 间接触发 parse_node_extra，不应 panic
    let _ = load_from_db(&db_path);
});
