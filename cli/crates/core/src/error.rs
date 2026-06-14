//! 统一错误类型。

use std::path::PathBuf;

/// rustmigrate 全局错误类型。
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// 图构建/查询错误。
    #[error("图错误: {message} (文件: {file})")]
    Graph { message: String, file: String },

    /// tree-sitter 解析失败。
    #[error("解析失败: {path}")]
    Parse { path: PathBuf },

    /// 状态转换非法。
    #[error("非法状态转换: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    /// 前置条件不满足。
    #[error("前置条件不满足: {condition}")]
    PreconditionFailed { condition: String },

    /// 模块被阻塞。
    #[error("模块 {module} 被阻塞: {reason}")]
    Blocked { module: String, reason: String },

    /// 循环依赖检测。
    #[error("检测到循环依赖: {cycle}")]
    CyclicDependency { cycle: String },

    /// 配置错误。
    #[error("配置错误: {0}")]
    Config(String),

    /// 数据库操作失败。
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    /// JSON 序列化/反序列化失败。
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    /// IO 操作失败。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// TOML 解析失败。
    #[error("TOML 解析错误: {0}")]
    Toml(#[from] toml::de::Error),

    /// TOML 序列化失败。
    #[error("TOML 序列化错误: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// 文件不存在。
    #[error("文件不存在: {0}")]
    FileNotFound(PathBuf),

    /// Schema 校验失败。
    #[error("Schema 校验失败: {0}")]
    SchemaValidation(String),

    /// 并发锁冲突。
    #[error("迁移锁冲突: {0}")]
    LockConflict(String),
}

/// 便捷 Result 别名。
pub type Result<T> = std::result::Result<T, MigrateError>;
