-- source-graph.db SQLite schema
-- 参照 docs/design/04-toolchain.md § 5.7.1

-- 版本追踪（M2 schema 升级时使用）
CREATE TABLE IF NOT EXISTS schema_versions (
    version   TEXT    NOT NULL,
    applied_at TEXT   NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO schema_versions (version) VALUES ('0.1');

-- 源码图节点
CREATE TABLE IF NOT EXISTS nodes (
    id               TEXT PRIMARY KEY,
    node_type        TEXT NOT NULL,  -- File|Module|Package|Function|Class|Interface|Enum|TypeAlias|Variable
    name             TEXT NOT NULL,
    file_path        TEXT NOT NULL,
    start_line       INTEGER,
    end_line         INTEGER,
    is_exported      INTEGER NOT NULL DEFAULT 0,
    complexity       TEXT,           -- simple|moderate|complex
    is_async         INTEGER NOT NULL DEFAULT 0,
    visibility       TEXT,           -- public|crate|private
    is_abstract      INTEGER NOT NULL DEFAULT 0,
    decorators       TEXT,           -- JSON 数组
    migration_status TEXT,
    migration_priority INTEGER
);

CREATE INDEX IF NOT EXISTS idx_nodes_file_path ON nodes(file_path);
CREATE INDEX IF NOT EXISTS idx_nodes_node_type ON nodes(node_type);

-- 源码图边
CREATE TABLE IF NOT EXISTS edges (
    source       TEXT NOT NULL REFERENCES nodes(id),
    target       TEXT NOT NULL REFERENCES nodes(id),
    edge_type    TEXT NOT NULL,  -- contains|imports|calls|extends|uses_type|exports|maps_to|tested_by
    provenance   TEXT NOT NULL DEFAULT 'tree_sitter',
    weight       REAL NOT NULL DEFAULT 1.0,
    sub_kind     TEXT,
    mapping_notes TEXT,
    PRIMARY KEY (source, target, edge_type)
);

CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(edge_type);

-- 文件指纹（增量图更新检测）
CREATE TABLE IF NOT EXISTS metadata (
    file_path      TEXT PRIMARY KEY,
    content_hash   TEXT NOT NULL,
    structure_hash TEXT NOT NULL,
    analyzed_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
