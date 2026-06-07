/// CLI 统一 JSON 响应结构。
///
/// 所有 CLI 命令输出格式：`{"status":"ok|error|warning", "data":{...}, "warnings":[...]}`
use serde::Serialize;

/// 响应状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Error,
    Warning,
}

/// 统一 JSON 响应结构。
#[derive(Debug, Clone, Serialize)]
pub struct Response<T: Serialize> {
    pub status: Status,
    pub data: T,
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
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Response<ErrorData> {
    /// 创建错误响应。
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            data: ErrorData {
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
                message: message.into(),
                context: Some(context.into()),
            },
            warnings: Vec::new(),
        }
    }
}
