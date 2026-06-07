-- source-graph.db SQLite schema
-- 参照 docs/design/04-toolchain.md § 5.7.3

-- 版本追踪（M2 schema 升级时使用）
CREATE TABLE IF NOT EXISTS schema_versions (
    version    TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO schema_versions (version) VALUES ('0.1');

-- 源码图节点
CREATE TABLE IF NOT EXISTS nodes (
    id                 TEXT PRIMARY KEY,
    node_type          TEXT NOT NULL,  -- File|Module|Package|Function|Class|Interface|Enum|RustTarget|TestFixture|TypeAlias|Variable
    name               TEXT NOT NULL,
    file_path          TEXT NOT NULL,
    start_line         INTEGER,
    end_line           INTEGER,
    is_exported        BOOLEAN DEFAULT FALSE,
    complexity         TEXT DEFAULT 'moderate',  -- simple|moderate|complex
    migration_status   TEXT,
    migration_priority INTEGER,
    extra              JSON   -- 类型特有的可扩展属性
);

CREATE INDEX IF NOT EXISTS idx_nodes_file ON nodes(file_path);
CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);

-- 源码图边
CREATE TABLE IF NOT EXISTS edges (
    source    TEXT NOT NULL REFERENCES nodes(id),
    target    TEXT NOT NULL REFERENCES nodes(id),
    edge_type TEXT NOT NULL,  -- contains|imports|calls|extends|uses_type|exports|maps_to|tested_by
    provenance TEXT NOT NULL DEFAULT 'tree-sitter',
    weight    REAL DEFAULT 1.0,
    sub_kind  TEXT,           -- 边的子类型（如 implements / constructor）
    mapping_notes TEXT,       -- 迁移映射备注
    PRIMARY KEY (source, target, edge_type)
);

CREATE INDEX IF NOT EXISTS idx_edges_source_type ON edges(source, edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_target_type ON edges(target, edge_type);

-- 文件指纹表（增量更新用）
CREATE TABLE IF NOT EXISTS file_fingerprints (
    file_path      TEXT PRIMARY KEY,
    content_hash   TEXT NOT NULL,
    structure_hash TEXT NOT NULL,  -- AST 签名哈希
    analyzed_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 元数据键值表（熔断机制 & topo-sort 前置检查依赖）
CREATE TABLE IF NOT EXISTS metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- MVP 预置：全量 graph build 完成后 UPSERT 重置为 'full'
INSERT OR IGNORE INTO metadata (key, value) VALUES ('graph_integrity', 'full');
