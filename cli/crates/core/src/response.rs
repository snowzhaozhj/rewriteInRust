//! CLI 统一 JSON 响应结构。
//!
//! 所有 CLI 命令输出格式：`{"status":"ok|error|warning", "data":{...}, "warnings":[...]}`

use serde::Serialize;

use crate::error::{ErrorCode, MigrateError};

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
    /// 警告信息列表。空时省略序列化——warnings 仅在确有内容时出现，
    /// 避免每条响应都挂个空 `[]` 噪声/误导。消费方按"可能缺失"处理（如 `jq '.warnings[]'`）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
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
    /// 错误编号（如 `"E001"`）。空时省略。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error_code: String,
    /// 错误描述信息。
    pub message: String,
    /// CI 是否可重试。
    pub retryable: bool,
    /// 用户可操作的建议。空时省略。
    #[serde(skip_serializing_if = "String::is_empty")]
    pub suggestion: String,
    /// 可选的上下文信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// 结构化补充上下文（如循环依赖的 `cycle_path` 数组）。
    ///
    /// `flatten`：`Some(obj)` 时把 obj 的键**提升到 `data` 顶层**（而非嵌套到
    /// `data.details`），从而让 `data.cycle_path` 等命名上下文保持稳定，对齐
    /// `09-appendix § Step 2.8` + plugin `analyze.md`（SKILL 直接读 `data.cycle_path`）；
    /// `None` 时不输出任何键。值须为 JSON object（其键被展开），不应为标量/数组。
    #[serde(flatten)]
    pub details: Option<serde_json::Value>,
}

impl ErrorData {
    /// 构造一条带结构化上下文的错误 data（`details` 的键将展开到 `data` 顶层）。
    pub fn new(
        kind: impl Into<String>,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            kind: kind.into(),
            error_code: String::new(),
            message: message.into(),
            retryable: false,
            suggestion: String::new(),
            context: None,
            details,
        }
    }

    /// 构造一条带 `ErrorCode` 的错误 data。
    pub fn with_error_code(
        error_code: ErrorCode,
        kind: impl Into<String>,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            kind: kind.into(),
            error_code: error_code.code().to_owned(),
            message: message.into(),
            retryable: error_code.is_retryable(),
            suggestion: error_code.suggestion().to_owned(),
            context: None,
            details,
        }
    }
}

impl Response<ErrorData> {
    /// 创建错误响应。
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            data: ErrorData {
                kind: "unknown".to_owned(),
                error_code: String::new(),
                message: message.into(),
                retryable: false,
                suggestion: String::new(),
                context: None,
                details: None,
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
                error_code: String::new(),
                message: message.into(),
                retryable: false,
                suggestion: String::new(),
                context: Some(context.into()),
                details: None,
            },
            warnings: Vec::new(),
        }
    }
}

impl From<MigrateError> for Response<ErrorData> {
    /// 将 [`MigrateError`] 转换为 JSON 错误响应。
    fn from(err: MigrateError) -> Self {
        let error_code = ErrorCode::from(&err);
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
            MigrateError::TomlSer(_) => "toml",
            MigrateError::FileNotFound(_) => "file_not_found",
            MigrateError::SchemaValidation(_) => "schema_validation",
            MigrateError::LockConflict(_) => "lock_conflict",
            MigrateError::NotImplemented(_) => "not_implemented",
            MigrateError::Timeout { .. } => "timeout",
        };
        Self {
            status: Status::Error,
            data: ErrorData {
                kind: kind.to_owned(),
                error_code: error_code.code().to_owned(),
                message: err.to_string(),
                retryable: error_code.is_retryable(),
                suggestion: error_code.suggestion().to_owned(),
                context: None,
                details: None,
            },
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_warnings_omitted() {
        // 空 warnings 不序列化——仅在确有内容时出现。
        let json = serde_json::to_value(Response::ok(serde_json::json!({"k": 1}))).unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json.get("warnings").is_none(), "空 warnings 应省略");
    }

    #[test]
    fn test_error_empty_warnings_omitted() {
        let resp: Response<ErrorData> = MigrateError::NotImplemented("x".into()).into();
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["status"], "error");
        assert_eq!(json["data"]["kind"], "not_implemented");
        assert!(json.get("warnings").is_none(), "空 warnings 应省略");
    }

    #[test]
    fn test_error_details_flatten_to_data_top_level() {
        // REFAC-14：details 的键应展开到 data 顶层（保持 `data.cycle_path` 契约），
        // 而非嵌套到 `data.details`。
        let resp = Response {
            status: Status::Error,
            data: ErrorData::new(
                "cyclic_dependency",
                "存在循环依赖",
                Some(serde_json::json!({ "cycle_path": [["a", "b", "a"]] })),
            ),
            warnings: Vec::new(),
        };
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["data"]["kind"], "cyclic_dependency");
        // 顶层可直接取到 cycle_path，且不存在嵌套的 data.details。
        assert_eq!(
            json["data"]["cycle_path"],
            serde_json::json!([["a", "b", "a"]])
        );
        assert!(
            json["data"].get("details").is_none(),
            "details 应被 flatten 展开，不应出现嵌套 details 键"
        );
    }

    #[test]
    fn test_error_details_none_omits_keys() {
        // details=None 时不应产生任何多余键（flatten 空）。
        let resp: Response<ErrorData> = MigrateError::NotImplemented("x".into()).into();
        let json = serde_json::to_value(resp).unwrap();
        let obj = json["data"].as_object().unwrap();
        // kind/error_code/message/retryable/suggestion 始终存在。
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("error_code"));
        assert!(obj.contains_key("message"));
        assert!(obj.contains_key("retryable"));
        assert!(obj.contains_key("suggestion"));
        // context/details 为 None 时省略。
        assert!(!obj.contains_key("context"));
        assert!(!obj.contains_key("details"));
    }

    #[test]
    fn test_nonempty_warnings_present_and_downgrades_status() {
        // warnings 有内容时才出现，且 status 降级为 warning。
        let resp = Response::ok_with_warnings(serde_json::json!({}), vec!["w".into()]);
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["status"], "warning");
        assert_eq!(json["warnings"], serde_json::json!(["w"]));
    }

    #[test]
    fn test_error_code_fields_in_json() {
        // 验证 MigrateError 转换后的 JSON 包含 error_code、retryable、suggestion 字段。
        let resp: Response<ErrorData> = MigrateError::CyclicDependency {
            cycle: "a→b→a".into(),
        }
        .into();
        let json = serde_json::to_value(resp).unwrap();
        assert_eq!(json["data"]["kind"], "cyclic_dependency");
        assert_eq!(json["data"]["error_code"], "E002");
        assert_eq!(json["data"]["retryable"], false);
        assert!(
            json["data"]["suggestion"].as_str().unwrap().len() > 0,
            "应有非空建议"
        );
    }

    #[test]
    fn test_retryable_errors() {
        // Timeout / IoError / DatabaseError 应标记为 retryable。
        let timeout_resp: Response<ErrorData> = MigrateError::Timeout {
            command: "test".into(),
            timeout_secs: 30,
        }
        .into();
        let json = serde_json::to_value(timeout_resp).unwrap();
        assert_eq!(json["data"]["retryable"], true);
        assert_eq!(json["data"]["error_code"], "E013");

        let io_resp: Response<ErrorData> =
            MigrateError::Io(std::io::Error::new(std::io::ErrorKind::Other, "fail")).into();
        let json = serde_json::to_value(io_resp).unwrap();
        assert_eq!(json["data"]["retryable"], true);
        assert_eq!(json["data"]["error_code"], "E014");
    }

    #[test]
    fn test_non_retryable_errors() {
        // 图、解析、配置等错误不可重试。
        let cases: Vec<(MigrateError, &str, &str)> = vec![
            (
                MigrateError::Graph {
                    message: "test".into(),
                    file: "a.ts".into(),
                },
                "graph",
                "E001",
            ),
            (MigrateError::Config("bad".into()), "config", "E012"),
            (
                MigrateError::NotImplemented("x".into()),
                "not_implemented",
                "E015",
            ),
        ];
        for (err, expected_kind, expected_code) in cases {
            let resp: Response<ErrorData> = err.into();
            let json = serde_json::to_value(resp).unwrap();
            assert_eq!(json["data"]["kind"], expected_kind);
            assert_eq!(json["data"]["error_code"], expected_code);
            assert_eq!(json["data"]["retryable"], false);
        }
    }

    #[test]
    fn test_all_error_codes_unique() {
        // 所有 ErrorCode 的编号应唯一。
        use std::collections::HashSet;
        let codes = [
            ErrorCode::GraphBuildFailed,
            ErrorCode::CyclicDependency,
            ErrorCode::ModuleNotFound,
            ErrorCode::InvalidTransition,
            ErrorCode::PreconditionFailed,
            ErrorCode::ModuleBlocked,
            ErrorCode::LockConflict,
            ErrorCode::SchemaValidation,
            ErrorCode::FileNotFound,
            ErrorCode::ParseFailed,
            ErrorCode::DatabaseError,
            ErrorCode::ConfigError,
            ErrorCode::Timeout,
            ErrorCode::IoError,
            ErrorCode::NotImplemented,
        ];
        let mut seen = HashSet::new();
        for ec in &codes {
            assert!(seen.insert(ec.code()), "错误编号 {} 重复", ec.code());
        }
        assert_eq!(codes.len(), 15, "应有 15 个错误码");
    }

    #[test]
    fn test_error_code_suggestion_non_empty() {
        // 每个 ErrorCode 的 suggestion 应非空。
        let codes = [
            ErrorCode::GraphBuildFailed,
            ErrorCode::CyclicDependency,
            ErrorCode::ModuleNotFound,
            ErrorCode::InvalidTransition,
            ErrorCode::PreconditionFailed,
            ErrorCode::ModuleBlocked,
            ErrorCode::LockConflict,
            ErrorCode::SchemaValidation,
            ErrorCode::FileNotFound,
            ErrorCode::ParseFailed,
            ErrorCode::DatabaseError,
            ErrorCode::ConfigError,
            ErrorCode::Timeout,
            ErrorCode::IoError,
            ErrorCode::NotImplemented,
        ];
        for ec in &codes {
            assert!(
                !ec.suggestion().is_empty(),
                "{:?} 的 suggestion 不应为空",
                ec
            );
        }
    }

    #[test]
    fn test_with_error_code_constructor() {
        // with_error_code 构造器应正确填充 error_code/retryable/suggestion。
        let data = ErrorData::with_error_code(
            ErrorCode::CyclicDependency,
            "cyclic_dependency",
            "检测到循环依赖",
            Some(serde_json::json!({ "cycle_path": [["a", "b", "a"]] })),
        );
        assert_eq!(data.error_code, "E002");
        assert!(!data.retryable);
        assert!(!data.suggestion.is_empty());
        assert_eq!(data.kind, "cyclic_dependency");
    }
}
