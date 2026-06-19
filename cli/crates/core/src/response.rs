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
    /// 错误描述信息。
    pub message: String,
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
            message: message.into(),
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
                message: message.into(),
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
                message: message.into(),
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
                message: err.to_string(),
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
        // 仅 kind/message（context/details 均 None 省略）。
        assert!(obj.contains_key("kind"));
        assert!(obj.contains_key("message"));
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
}
