//! CLI 统一 JSON 响应结构。
//!
//! 所有 CLI 命令输出格式：`{"status":"ok|error|warning", "data":{...}, "warnings":[...]}`

use serde::Serialize;

use crate::error::MigrateError;

/// 响应状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// 成功。
    Ok,
    /// 错误。
    Error,
    /// 警告。
    Warning,
}

/// 统一 JSON 响应结构。
#[derive(Debug, Clone, Serialize)]
pub struct Response<T: Serialize> {
    /// 响应状态码。
    pub status: Status,
    /// 响应数据。
    pub data: T,
    /// 警告信息列表。
    ///
    /// **始终序列化**（空时输出 `[]`），保证 `{status,data,warnings}` 三字段恒在，
    /// 满足「所有命令输出全字段可解析」契约（见 CLAUDE.md 编码约束）。
    pub warnings: Vec<String>,
}

impl<T: Serialize> Response<T> {
    /// 创建成功响应。
    pub fn ok(data: T) -> Self {
        Self {
            status: Status::Ok,
            data,
            warnings: Vec::new(),
        }
    }

    /// 创建带警告的成功响应。
    pub fn ok_with_warnings(data: T, warnings: Vec<String>) -> Self {
        Self {
            status: if warnings.is_empty() {
                Status::Ok
            } else {
                Status::Warning
            },
            data,
            warnings,
        }
    }
}

/// 错误响应的 data 字段。
#[derive(Debug, Clone, Serialize)]
pub struct ErrorData {
    /// 错误分类标识（如 `"graph"`, `"parse"`, `"config"`）。
    pub kind: String,
    /// 错误描述信息。
    pub message: String,
    /// 可选的上下文信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Response<ErrorData> {
    /// 创建错误响应。
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            data: ErrorData {
                kind: "unknown".to_owned(),
                message: message.into(),
                context: None,
            },
            warnings: Vec::new(),
        }
    }

    /// 创建带上下文的错误响应。
    pub fn error_with_context(message: impl Into<String>, context: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            data: ErrorData {
                kind: "unknown".to_owned(),
                message: message.into(),
                context: Some(context.into()),
            },
            warnings: Vec::new(),
        }
    }
}

impl From<MigrateError> for Response<ErrorData> {
    /// 将 [`MigrateError`] 转换为 JSON 错误响应。
    fn from(err: MigrateError) -> Self {
        let kind = match &err {
            MigrateError::Graph { .. } => "graph",
            MigrateError::Parse { .. } => "parse",
            MigrateError::InvalidTransition { .. } => "invalid_transition",
            MigrateError::PreconditionFailed { .. } => "precondition",
            MigrateError::Blocked { .. } => "blocked",
            MigrateError::CyclicDependency { .. } => "cyclic_dependency",
            MigrateError::Config(_) => "config",
            MigrateError::Database(_) => "database",
            MigrateError::Json(_) => "json",
            MigrateError::Io(_) => "io",
            MigrateError::Toml(_) => "toml",
            MigrateError::FileNotFound(_) => "file_not_found",
            MigrateError::SchemaValidation(_) => "schema_validation",
            MigrateError::LockConflict(_) => "lock_conflict",
            MigrateError::NotImplemented(_) => "not_implemented",
        };
        Self {
            status: Status::Error,
            data: ErrorData {
                kind: kind.to_owned(),
                message: err.to_string(),
                context: None,
            },
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_always_outputs_warnings_field() {
        let json = serde_json::to_value(Response::ok(serde_json::json!({"k": 1}))).unwrap();
        // {status,data,warnings} 三字段恒在，空 warnings 输出 []。
        assert_eq!(json["status"], "ok");
        assert!(json.get("warnings").is_some(), "warnings 字段必须存在");
        assert_eq!(json["warnings"], serde_json::json!([]));
    }

    #[test]
    fn test_error_outputs_warnings_field() {
        let resp: Response<ErrorData> = MigrateError::NotImplemented("x".into()).into();
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["kind"], "not_implemented");
        assert_eq!(json["warnings"], serde_json::json!([]));
    }

    #[test]
    fn test_nonempty_warnings_downgrades_to_warning_status() {
        let resp = Response::ok_with_warnings(serde_json::json!({}), vec!["w".into()]);
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["status"], "warning");
        assert_eq!(json["warnings"], serde_json::json!(["w"]));
    }
}
