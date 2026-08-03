//! 统一错误类型。

use std::path::PathBuf;

use serde::Serialize;

/// 错误码枚举——覆盖 ~15 条高频错误，提供编号、重试建议和用户提示。
///
/// `EnumIter`：供 `design_error_codes.rs` 守卫从**真值域**取全部码（而非写死清单）
/// 断言「设计文档 06 § 10.7 错误码表声称由 CLI 返回的码必须真实存在」——M4 核实时
/// 该表 11 个语义码里有 3 个（`VALIDATION_TIMEOUT`/`OOM`/`SCHEMA_CORRUPTED`）在 CLI
/// 中从不存在，而 06 § 10.7 要求编排器按它们分流，照做恒为假。见 MDR-021。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 图构建失败。
    GraphBuildFailed,
    /// 循环依赖。
    CyclicDependency,
    /// 模块未找到。
    ModuleNotFound,
    /// 非法状态转换。
    InvalidTransition,
    /// 前置条件不满足。
    PreconditionFailed,
    /// 模块被阻塞。
    ModuleBlocked,
    /// 迁移锁冲突。
    LockConflict,
    /// Schema 校验失败。
    SchemaValidation,
    /// 文件不存在。
    FileNotFound,
    /// 解析失败。
    ParseFailed,
    /// 数据库错误。
    DatabaseError,
    /// 配置错误。
    ConfigError,
    /// 超时。
    Timeout,
    /// IO 错误。
    IoError,
    /// 命令尚未实现。
    NotImplemented,
}

impl ErrorCode {
    /// 错误编号（如 `"E001"`）。
    pub fn code(self) -> &'static str {
        match self {
            Self::GraphBuildFailed => "E001",
            Self::CyclicDependency => "E002",
            Self::ModuleNotFound => "E003",
            Self::InvalidTransition => "E004",
            Self::PreconditionFailed => "E005",
            Self::ModuleBlocked => "E006",
            Self::LockConflict => "E007",
            Self::SchemaValidation => "E008",
            Self::FileNotFound => "E009",
            Self::ParseFailed => "E010",
            Self::DatabaseError => "E011",
            Self::ConfigError => "E012",
            Self::Timeout => "E013",
            Self::IoError => "E014",
            Self::NotImplemented => "E015",
        }
    }

    /// CI 是否可重试（Timeout / IoError / DatabaseError 为 true）。
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::IoError | Self::DatabaseError)
    }

    /// 用户提示信息。
    pub fn suggestion(self) -> &'static str {
        match self {
            Self::GraphBuildFailed => "请检查源码目录结构后重试 graph build",
            Self::CyclicDependency => "请打破循环依赖后重试",
            Self::ModuleNotFound => "请确认模块路径是否正确",
            Self::InvalidTransition => "当前状态不允许此操作，请检查迁移状态",
            Self::PreconditionFailed => "前置条件不满足，请先完成依赖步骤",
            Self::ModuleBlocked => "模块被上游依赖阻塞，请先完成上游模块迁移",
            Self::LockConflict => "迁移锁冲突，请确认无其他迁移进程运行后重试",
            Self::SchemaValidation => "Schema 校验失败，请检查配置文件格式",
            Self::FileNotFound => "文件不存在，请确认路径是否正确",
            // E010 的**两条真实路径**（主审视角逐条实证，见 MDR-021 § 可达性核查）：
            // ① `migration-state.json` 损坏且 `.backup` 也不可用；② `validate rules
            // --registry` 指向的 `rule-registry.json` 损坏。
            //
            // 三处**看似是来源、实则不是**的映射，故文案不提它们：
            // ⒜ `MigrateError::Parse`（源码语法）虽映射到本码，但其全部三个调用点
            //    （`graph/build.rs` 的 analyze_file 处）都把它降级成 `warnings` 并跳过该
            //    文件，绝不上抛——源码语法错误实测不产出本码，写进建议会让用户拿到的第一
            //    条指引永远指错方向。
            // ⒝ `Toml`/`TomlSer` 亦映射到本码，但三处 `toml::from_str` 全部显式包成
            //    `MigrateError::Config`——配置文件损坏实测返 E012。
            // ⒞ 不提「从备份恢复」：state 主文件 JSON 损坏时 `MigrationStateMachine::load`
            //    已自动回退 `.migration-state.json.backup` 并降级 warning；能走到本建议说明
            //    备份不存在或同样不可用，叫用户去恢复是把他引向死路。
            //
            // `init` 兜底只对 state 文件成立（它不生成 `rule-registry.json`，那个文件在
            // `plugin/skills/migrate/references/` 下），故该建议收在 state 分句内部。
            Self::ParseFailed => {
                "JSON 解析失败：若为 .rust-migration/migration-state.json 损坏，按 message 中的\
                 行列号修正（主文件与 .backup 双双不可用时才会到此），无法修复则重新执行 init；\
                 若为 validate rules 的 --registry 指向的 rule-registry.json 损坏，按行列号修正该文件"
            }
            Self::DatabaseError => "数据库操作失败，可重试；若持续失败请检查 .rust-migration/ 目录",
            Self::ConfigError => "配置错误，请检查配置文件",
            Self::Timeout => "子进程超时，可重试或增加超时时间",
            Self::IoError => "IO 操作失败，可重试；若持续失败请检查文件权限",
            Self::NotImplemented => "此命令尚未实现，将在后续版本支持",
        }
    }
}

impl From<&MigrateError> for ErrorCode {
    /// 从 [`MigrateError`] 映射到 [`ErrorCode`]。
    fn from(err: &MigrateError) -> Self {
        match err {
            MigrateError::Graph { .. } => Self::GraphBuildFailed,
            MigrateError::Parse { .. } => Self::ParseFailed,
            MigrateError::InvalidTransition { .. } => Self::InvalidTransition,
            MigrateError::PreconditionFailed { .. } => Self::PreconditionFailed,
            MigrateError::Blocked { .. } => Self::ModuleBlocked,
            MigrateError::CyclicDependency { .. } => Self::CyclicDependency,
            MigrateError::Config(_) => Self::ConfigError,
            MigrateError::Database(_) => Self::DatabaseError,
            MigrateError::Json(_) | MigrateError::Toml(_) | MigrateError::TomlSer(_) => {
                Self::ParseFailed
            }
            MigrateError::Io(_) => Self::IoError,
            MigrateError::FileNotFound(_) => Self::FileNotFound,
            MigrateError::SchemaValidation(_) => Self::SchemaValidation,
            MigrateError::LockConflict(_) => Self::LockConflict,
            MigrateError::NotImplemented(_) => Self::NotImplemented,
            MigrateError::Timeout { .. } => Self::Timeout,
        }
    }
}

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

    /// 命令尚未实现（占位，待后续阶段接线）。
    #[error("命令尚未实现: {0}")]
    NotImplemented(String),

    /// 子进程执行超时。
    #[error("子进程超时: {command} (超时 {timeout_secs}s)")]
    Timeout { command: String, timeout_secs: u64 },
}

/// 便捷 Result 别名。
pub type Result<T> = std::result::Result<T, MigrateError>;
