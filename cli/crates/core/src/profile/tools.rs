//! 外部工具可用性检测（`rustmigrate profile`）。
//!
//! 设计（`06-plugin-structure.md` § CLI / 错误码 `ADAPTER_TOOL_MISSING`、`RUST_TOOL_MISSING`）：
//! - 按适配器的 `analysis-tools.json` 逐项验证「已安装 + 满足最低版本」；
//! - 检测 Tier 0 Rust 外部二进制 `cargo-nextest`。
//!
//! 工具探测通过运行 `<binary> --version` 实现：命令不存在或非零退出视为「未安装」，
//! 退出成功则从输出中提取版本号与 `min_version` 比较。版本解析/比较为纯函数，可单测；
//! 命令执行单独隔离，便于复用与降低测试不确定性。

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{MigrateError, Result};

/// `analysis-tools.json` 中的单个工具条目。
///
/// 格式见 `06-plugin-structure.md` § adapter.json 契约：
/// JSON 数组，每项 `{tool_id, display_name, min_version, install_hint, required}`。
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisTool {
    /// 工具的可执行文件名（探测时运行 `<tool_id> --version`）。
    pub tool_id: String,
    /// 人类可读名称（用于警告文案）。
    pub display_name: String,
    /// 最低版本要求（`major.minor.patch`，可选；省略则仅检测是否安装）。
    #[serde(default)]
    pub min_version: Option<String>,
    /// 安装提示（写入警告 `install_hint`，可选）。
    #[serde(default)]
    pub install_hint: Option<String>,
    /// 是否为必需工具（仅必需工具缺失才产出警告）。
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_required() -> bool {
    true
}

/// 单个工具的检测结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolStatus {
    /// 工具可执行文件名。
    pub tool_id: String,
    /// 人类可读名称。
    pub display_name: String,
    /// 是否检测到（命令可执行且成功退出）。
    pub available: bool,
    /// 探测到的版本号（解析成功时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_version: Option<String>,
    /// 要求的最低版本（如有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// 是否满足最低版本（无 `min_version` 时只要 available 即 true）。
    pub satisfies_min: bool,
    /// 安装提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    /// 是否为必需工具。
    pub required: bool,
}

impl ToolStatus {
    /// 是否构成「缺失」（必需且不可用或版本不足）。
    pub fn is_missing(&self) -> bool {
        self.required && (!self.available || !self.satisfies_min)
    }
}

/// 读取并解析适配器的 `analysis-tools.json`。
///
/// 文件不存在返回 `MigrateError::FileNotFound`，JSON 非法返回 `MigrateError::Json`。
pub fn load_analysis_tools(path: &Path) -> Result<Vec<AnalysisTool>> {
    if !path.exists() {
        return Err(MigrateError::FileNotFound(path.to_path_buf()));
    }
    let content = std::fs::read_to_string(path)?;
    let tools: Vec<AnalysisTool> = serde_json::from_str(&content)?;
    Ok(tools)
}

/// 检测一组适配器工具。
pub fn check_adapter_tools(tools: &[AnalysisTool]) -> Vec<ToolStatus> {
    tools.iter().map(check_tool).collect()
}

/// 检测单个工具：运行 `<tool_id> --version` 并解析版本。
pub fn check_tool(tool: &AnalysisTool) -> ToolStatus {
    let output = probe_version(&tool.tool_id, &["--version"]);
    let detected_version = output.as_deref().and_then(parse_version_str);
    let available = output.is_some();
    let satisfies_min = match (&tool.min_version, &detected_version) {
        // 无最低版本要求：安装即满足。
        (None, _) => available,
        // 有要求但探测不到版本号：保守判定为不满足。
        (Some(_), None) => false,
        (Some(min), Some(det)) => version_satisfies(det, min),
    };
    ToolStatus {
        tool_id: tool.tool_id.clone(),
        display_name: tool.display_name.clone(),
        available,
        detected_version,
        min_version: tool.min_version.clone(),
        satisfies_min,
        install_hint: tool.install_hint.clone(),
        required: tool.required,
    }
}

/// 检测 Tier 0 Rust 外部二进制 `cargo-nextest`（运行 `cargo nextest --version`）。
pub fn check_cargo_nextest() -> ToolStatus {
    let output = probe_version("cargo", &["nextest", "--version"]);
    let detected_version = output.as_deref().and_then(parse_version_str);
    ToolStatus {
        tool_id: "cargo-nextest".to_owned(),
        display_name: "cargo-nextest".to_owned(),
        available: output.is_some(),
        detected_version,
        min_version: None,
        satisfies_min: output.is_some(),
        install_hint: Some("cargo install cargo-nextest --locked".to_owned()),
        required: true,
    }
}

/// 运行 `<bin> <args...>` 探测版本。命令不存在或非零退出返回 `None`，
/// 成功则返回 stdout（为空时回退 stderr）去空白后的字符串。
fn probe_version(bin: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(bin).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        return Some(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    (!stderr.is_empty()).then_some(stderr)
}

/// 从工具版本输出中提取首个形如 `\d+\.\d+(\.\d+)?` 的版本号。
///
/// 兼容 `depcruise 16.3.0`、`cargo-nextest 0.9.70`、`tsc Version 5.4.5` 等格式。
fn parse_version_str(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dots = 0;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    // 末尾点（如 "1.2."）不并入版本，避免误判。
                    if i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_digit() {
                        break;
                    }
                    dots += 1;
                }
                i += 1;
            }
            if dots >= 1 {
                return Some(s[start..i].to_owned());
            }
            // 仅有整数无点：跳过，继续找带点的版本号。
        }
        i += 1;
    }
    None
}

/// 解析 `major.minor.patch`（缺失部分补 0）为 `(u64, u64, u64)`。
///
/// 用 u64 避免 CalVer/超长版本段（如 `20240131`）或异常输出溢出 u32 导致解析失败、
/// 进而被 [`version_satisfies`] 保守误判为「版本不足」。
fn parse_triple(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(0);
    Some((major, minor, patch))
}

/// `detected >= min` 的语义化版本比较。任一无法解析时保守返回 `false`。
fn version_satisfies(detected: &str, min: &str) -> bool {
    match (parse_triple(detected), parse_triple(min)) {
        (Some(d), Some(m)) => d >= m,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_str_variants() {
        assert_eq!(
            parse_version_str("depcruise 16.3.0").as_deref(),
            Some("16.3.0")
        );
        assert_eq!(
            parse_version_str("cargo-nextest 0.9.70").as_deref(),
            Some("0.9.70")
        );
        assert_eq!(parse_version_str("Version 5.4").as_deref(), Some("5.4"));
        assert_eq!(parse_version_str("v2.0.1-beta").as_deref(), Some("2.0.1"));
        // 纯整数无点：不视为版本。
        assert_eq!(parse_version_str("node 20"), None);
        assert_eq!(parse_version_str("no version here"), None);
        // 末尾点不并入。
        assert_eq!(parse_version_str("ends 1.2.").as_deref(), Some("1.2"));
    }

    #[test]
    fn test_version_satisfies() {
        assert!(version_satisfies("16.3.0", "16.0.0"));
        assert!(version_satisfies("16.0.0", "16.0.0"));
        assert!(version_satisfies("17.0.0", "16.9.9"));
        assert!(!version_satisfies("15.9.9", "16.0.0"));
        assert!(version_satisfies("5.4", "5.0.0")); // 缺失部分补 0
        assert!(!version_satisfies("5.0", "5.0.1"));
        // 无法解析保守 false。
        assert!(!version_satisfies("abc", "1.0.0"));
    }

    #[test]
    fn test_check_tool_no_min_version_presence_only() {
        // 不存在的二进制：available=false，is_missing=true（required 默认 true）。
        let tool = AnalysisTool {
            tool_id: "definitely-not-a-real-binary-xyz".to_owned(),
            display_name: "Ghost Tool".to_owned(),
            min_version: None,
            install_hint: Some("install it".to_owned()),
            required: true,
        };
        let status = check_tool(&tool);
        assert!(!status.available);
        assert!(!status.satisfies_min);
        assert!(status.is_missing());
    }

    #[test]
    fn test_optional_tool_not_missing_when_absent() {
        let tool = AnalysisTool {
            tool_id: "definitely-not-a-real-binary-xyz".to_owned(),
            display_name: "Optional Tool".to_owned(),
            min_version: None,
            install_hint: None,
            required: false,
        };
        let status = check_tool(&tool);
        assert!(!status.available);
        assert!(!status.is_missing()); // 非必需，不算缺失
    }

    #[test]
    fn test_load_analysis_tools_missing_file() {
        let err = load_analysis_tools(Path::new("/tmp/不存在/analysis-tools.json")).unwrap_err();
        assert!(matches!(err, MigrateError::FileNotFound(_)));
    }

    #[test]
    fn test_load_analysis_tools_parses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis-tools.json");
        std::fs::write(
            &path,
            r#"[{"tool_id":"depcruise","display_name":"dependency-cruiser","min_version":"11.0.0","install_hint":"npm i -g dependency-cruiser","required":true}]"#,
        )
        .unwrap();
        let tools = load_analysis_tools(&path).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].tool_id, "depcruise");
        assert_eq!(tools[0].min_version.as_deref(), Some("11.0.0"));
        assert!(tools[0].required);
    }
}
