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

use serde::{Deserialize, Serialize, Serializer};

use crate::error::{MigrateError, Result};
use crate::process::{run_with_timeout, PROBE_TIMEOUT};

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

/// 工具探测的**状态枚举**——用类型把「命令不存在 / 探测失败 / 可用」三态收敛为
/// 互斥变体，杜绝旧 `available`+`detected_version`+`probe_error` 三个扁平字段可能拼出的
/// 非法组合（如 `available=false` 却带 `detected_version=Some`）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolProbe {
    /// 命令不存在（PATH 未找到）——真正的「未安装」。
    Missing,
    /// 命令存在但探测失败（权限/非零退出/无输出），附原因。
    ProbeFailed(String),
    /// 命令可用，附探测到的版本号（解析失败为 `None`）。
    Available(Option<String>),
}

/// 单个工具的检测结果。
///
/// 探测三态由私有 [`ToolProbe`] 枚举承载（消除非法状态），对外仍通过 getter 暴露
/// `available()`/`detected_version()`/`probe_error()`/`satisfies_min()`。
/// 自定义 `Serialize` 把状态摊平回历史扁平结构
/// （`available`/`detected_version`/`satisfies_min`/`probe_error`），保持 `tool_checks`
/// JSON 输出契约不变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    /// 工具可执行文件名。
    pub tool_id: String,
    /// 人类可读名称。
    pub display_name: String,
    /// 要求的最低版本（如有）。
    pub min_version: Option<String>,
    /// 安装提示。
    pub install_hint: Option<String>,
    /// 是否为必需工具。
    pub required: bool,
    /// 探测状态（命令不存在 / 探测失败 / 可用+版本）。
    probe: ToolProbe,
}

impl ToolStatus {
    /// 是否检测到（命令可执行且成功退出）。
    pub fn available(&self) -> bool {
        matches!(self.probe, ToolProbe::Available(_))
    }

    /// 探测到的版本号（仅 `Available` 且解析成功时为 `Some`）。
    pub fn detected_version(&self) -> Option<&str> {
        match &self.probe {
            ToolProbe::Available(v) => v.as_deref(),
            _ => None,
        }
    }

    /// 探测异常原因（命令存在但执行失败：权限/非零退出/环境异常等）。
    /// 区别于「命令不存在」(`available()==false` 且本方法为 `None`)，避免把
    /// 「无法确认」误报为「未安装」。
    pub fn probe_error(&self) -> Option<&str> {
        match &self.probe {
            ToolProbe::ProbeFailed(e) => Some(e),
            _ => None,
        }
    }

    /// 是否满足最低版本。
    ///
    /// - 不可用（Missing/ProbeFailed）→ false；
    /// - 可用且无 `min_version` 要求 → true；
    /// - 可用但有要求却解析不出版本 → false（保守，不误判满足）；
    /// - 可用且版本可解析 → 与 `min_version` 比较。
    pub fn satisfies_min(&self) -> bool {
        match (&self.probe, &self.min_version) {
            (ToolProbe::Available(_), None) => true,
            (ToolProbe::Available(Some(det)), Some(min)) => version_satisfies(det, min),
            _ => false,
        }
    }

    /// 是否构成「缺失」（必需且不可用或版本不足）。
    pub fn is_missing(&self) -> bool {
        // satisfies_min() 在不可用时已为 false，故无需再单独判 available。
        self.required && !self.satisfies_min()
    }
}

impl Serialize for ToolStatus {
    /// 摊平为历史扁平结构，保持 `tool_checks` JSON 输出契约不变。
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            tool_id: &'a str,
            display_name: &'a str,
            available: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            detected_version: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            min_version: Option<&'a str>,
            satisfies_min: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            install_hint: Option<&'a str>,
            required: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            probe_error: Option<&'a str>,
        }
        Wire {
            tool_id: &self.tool_id,
            display_name: &self.display_name,
            available: self.available(),
            detected_version: self.detected_version(),
            min_version: self.min_version.as_deref(),
            satisfies_min: self.satisfies_min(),
            install_hint: self.install_hint.as_deref(),
            required: self.required,
            probe_error: self.probe_error(),
        }
        .serialize(serializer)
    }
}

/// 工具版本探测结果（区分「未安装」与「探测失败」，避免归并成误导性结论）。
enum Probe {
    /// 命令成功执行，捕获到版本输出文本。
    Found(String),
    /// 命令不存在（PATH 未找到）——真正的「未安装」。
    NotFound,
    /// 命令存在但探测失败（权限不足/非零退出/环境异常），附原因。
    Failed(String),
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
    ToolStatus {
        tool_id: tool.tool_id.clone(),
        display_name: tool.display_name.clone(),
        min_version: tool.min_version.clone(),
        install_hint: tool.install_hint.clone(),
        required: tool.required,
        probe: interpret_probe(probe_version(&tool.tool_id, &["--version"])),
    }
}

/// 检测 Tier 0 Rust 外部二进制 `cargo-nextest`（运行 `cargo nextest --version`）。
pub fn check_cargo_nextest() -> ToolStatus {
    ToolStatus {
        tool_id: "cargo-nextest".to_owned(),
        display_name: "cargo-nextest".to_owned(),
        min_version: None,
        install_hint: Some("cargo install cargo-nextest --locked".to_owned()),
        required: true,
        probe: interpret_probe(probe_version("cargo", &["nextest", "--version"])),
    }
}

/// 把原始 [`Probe`] 折叠为状态枚举 [`ToolProbe`]。
fn interpret_probe(probe: Probe) -> ToolProbe {
    match probe {
        Probe::Found(out) => ToolProbe::Available(parse_version_str(&out)),
        Probe::NotFound => ToolProbe::Missing,
        Probe::Failed(e) => ToolProbe::ProbeFailed(e),
    }
}

/// 运行 `<bin> <args...>` 探测版本，区分「未安装」与「探测失败」：
/// - spawn 失败且为 `NotFound` → [`Probe::NotFound`]（命令不在 PATH）；
/// - spawn 失败的其他 IO 错误（权限等）或非零退出 → [`Probe::Failed`]（附原因）；
/// - 超时 → [`Probe::Failed`]（附超时信息）；
/// - 成功 → [`Probe::Found`]，取 stdout（为空回退 stderr）去空白。
///
/// `stdin` 置 null，避免工具误等交互输入而挂起。
fn probe_version(bin: &str, args: &[&str]) -> Probe {
    let label = format!("{bin} {}", args.join(" "));
    let output = match run_with_timeout(Command::new(bin).args(args), PROBE_TIMEOUT, &label) {
        Ok(o) => o,
        Err(MigrateError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Probe::NotFound
        }
        Err(MigrateError::Io(e)) => return Probe::Failed(format!("无法执行 {bin}: {e}")),
        Err(MigrateError::Timeout { timeout_secs, .. }) => {
            return Probe::Failed(format!("{bin} 探测超时 ({timeout_secs}s)"))
        }
        Err(e) => return Probe::Failed(format!("无法执行 {bin}: {e}")),
    };
    if !output.status.success() {
        return Probe::Failed(format!("{bin} --version 退出码非零: {}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        return Probe::Found(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Probe::Failed(format!("{bin} --version 无输出"))
    } else {
        Probe::Found(stderr)
    }
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

    /// 构造一个指定探测状态的 ToolStatus（测试辅助，绕过命令探测）。
    fn status_with(min_version: Option<&str>, probe: ToolProbe) -> ToolStatus {
        ToolStatus {
            tool_id: "t".to_owned(),
            display_name: "T".to_owned(),
            min_version: min_version.map(str::to_owned),
            install_hint: None,
            required: true,
            probe,
        }
    }

    #[test]
    fn test_serialize_keeps_flat_contract() {
        // 枚举化后 JSON 仍是历史扁平结构（available/detected_version/satisfies_min/...），
        // 保持 tool_checks 输出契约不变。
        let status = status_with(
            Some("16.0.0"),
            ToolProbe::Available(Some("16.3.0".to_owned())),
        );
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["available"], true);
        assert_eq!(json["detected_version"], "16.3.0");
        assert_eq!(json["min_version"], "16.0.0");
        assert_eq!(json["satisfies_min"], true);
        // 无 probe_error 时该键省略（非 null）。
        assert!(json.get("probe_error").is_none());
        assert!(json.get("install_hint").is_none());
    }

    #[test]
    fn test_serialize_probe_failed_and_missing() {
        // ProbeFailed：available=false、probe_error 出现、detected_version 省略。
        let failed = status_with(None, ToolProbe::ProbeFailed("权限不足".to_owned()));
        let j1 = serde_json::to_value(&failed).unwrap();
        assert_eq!(j1["available"], false);
        assert_eq!(j1["probe_error"], "权限不足");
        assert!(j1.get("detected_version").is_none());

        // Missing：available=false 且无 probe_error（区分「未安装」与「探测失败」）。
        let missing = status_with(None, ToolProbe::Missing);
        let j2 = serde_json::to_value(&missing).unwrap();
        assert_eq!(j2["available"], false);
        assert!(j2.get("probe_error").is_none());
    }

    #[test]
    fn test_satisfies_min_derivation() {
        // 无要求 + 可用 → 满足。
        assert!(status_with(None, ToolProbe::Available(None)).satisfies_min());
        // 有要求但版本解析不出 → 保守不满足。
        assert!(!status_with(Some("1.0.0"), ToolProbe::Available(None)).satisfies_min());
        // 版本达标 / 不达标。
        assert!(status_with(
            Some("1.0.0"),
            ToolProbe::Available(Some("1.2.0".to_owned()))
        )
        .satisfies_min());
        assert!(!status_with(
            Some("2.0.0"),
            ToolProbe::Available(Some("1.2.0".to_owned()))
        )
        .satisfies_min());
        // 不可用一律不满足。
        assert!(!status_with(None, ToolProbe::Missing).satisfies_min());
        assert!(!status_with(None, ToolProbe::ProbeFailed("x".to_owned())).satisfies_min());
    }

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
        assert!(!status.available());
        assert!(!status.satisfies_min());
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
        assert!(!status.available());
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
