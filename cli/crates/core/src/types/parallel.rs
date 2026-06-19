//! 并行翻译派发/回传协议类型。
//!
//! 编排器与 SubAgent 之间通过文件系统 + JSON 通信，
//! 本模块定义派发请求（`TranslationDispatch`）和回传结果（`TranslationResult`）的结构。
//!
//! 参照 `docs/decisions/003-m2-parallel-write-isolation.md` 和
//! `docs/PLAN-M2.md` §7 通信协议。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 编排器 → SubAgent 的派发请求。
///
/// 包含翻译目标模块、worktree 路径、依赖接口和 porting 规则约束。
/// 对应通信协议步骤 ①：编排器创建 worktree 后派发给 SubAgent。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TranslationDispatch {
    /// 要翻译的模块 key（如 `"file:src/utils.ts"`）。
    pub module_key: String,
    /// worktree 路径。
    pub worktree_path: PathBuf,
    /// 依赖接口（该模块的直接依赖模块的导出签名）。
    pub dependency_interfaces: Vec<DependencyInterface>,
    /// porting 规则（最小化共享写面的约束）。
    pub porting_rules: PortingRules,
}

/// 依赖模块的接口签名。
///
/// 翻译时 SubAgent 需要知道直接依赖模块的导出符号，
/// 以便生成正确的 `use` 引用和类型对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DependencyInterface {
    /// 依赖模块 key（如 `"file:src/types.ts"`）。
    pub module_key: String,
    /// 该模块的导出符号列表（函数签名、类型名等）。
    pub exports: Vec<String>,
}

/// porting 规则约束包。
///
/// 最小化共享写面的约束集合（D3 决策 §约束包 #2）：
/// 并行模块优先用既有共享 API + 逃生口，复杂共享扩展留串行 cleanup。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PortingRules {
    /// 优先用既有共享 API。
    pub prefer_existing_api: bool,
    /// 不够时用 `Error::Other`/`anyhow` 逃生口。
    pub allow_escape_hatch: bool,
    /// 禁删/改签名既有共享 API。
    pub no_break_shared_api: bool,
    /// 新增只 append。
    pub append_only: bool,
}

impl Default for PortingRules {
    /// 默认全部启用（保守策略）。
    fn default() -> Self {
        Self {
            prefer_existing_api: true,
            allow_escape_hatch: true,
            no_break_shared_api: true,
            append_only: true,
        }
    }
}

/// SubAgent → 编排器的回传结果。
///
/// 对应通信协议步骤 ③：只回传 touched-list，代码留盘（上下文经济）。
/// `agent_done` 是 substatus（非终态），只有合并后整组 check 过才升最终 `done`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TranslationResult {
    /// 模块 key。
    pub module_key: String,
    /// agent 自检状态。
    pub status: AgentStatus,
    /// 模块自身文件（在 worktree 内的路径）。
    pub own_files: Vec<PathBuf>,
    /// 触碰过的共享文件（仅文件清单，无内容）。
    pub shared_touched: Vec<PathBuf>,
    /// 自检结果（cargo check）。
    pub self_check: CheckStatus,
    /// 测试结果（cargo test）。
    pub test: CheckStatus,
}

/// Agent 自检状态。
///
/// `AgentDone` 是 substatus（沿用 `phase_*_complete` 那套模式），
/// 非终态——只有编排器合并后整组 check 过才升最终 `done`（两层 done）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// 自检通过，非终态。
    AgentDone,
    /// 自检未通过。
    Failed,
}

/// 检查状态（自检/测试通用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// 通过。
    Pass,
    /// 失败（附带错误信息）。
    Fail {
        /// 失败原因。
        message: String,
    },
    /// 跳过（如 trivial 模块无需测试）。
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_porting_rules_default() {
        let rules = PortingRules::default();
        assert!(rules.prefer_existing_api);
        assert!(rules.allow_escape_hatch);
        assert!(rules.no_break_shared_api);
        assert!(rules.append_only);
    }

    #[test]
    fn test_translation_dispatch_roundtrip() {
        let dispatch = TranslationDispatch {
            module_key: "file:src/utils.ts".to_string(),
            worktree_path: PathBuf::from(".wt/utils"),
            dependency_interfaces: vec![DependencyInterface {
                module_key: "file:src/types.ts".to_string(),
                exports: vec![
                    "pub struct Config".to_string(),
                    "pub fn validate".to_string(),
                ],
            }],
            porting_rules: PortingRules::default(),
        };

        let json = serde_json::to_string_pretty(&dispatch).unwrap();
        let parsed: TranslationDispatch = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.module_key, dispatch.module_key);
        assert_eq!(parsed.worktree_path, dispatch.worktree_path);
        assert_eq!(parsed.dependency_interfaces.len(), 1);
        assert_eq!(
            parsed.dependency_interfaces[0].module_key,
            "file:src/types.ts"
        );
        assert_eq!(parsed.dependency_interfaces[0].exports.len(), 2);
        assert!(parsed.porting_rules.prefer_existing_api);
        assert!(parsed.porting_rules.no_break_shared_api);
    }

    #[test]
    fn test_translation_result_roundtrip() {
        let result = TranslationResult {
            module_key: "file:src/parser.ts".to_string(),
            status: AgentStatus::AgentDone,
            own_files: vec![PathBuf::from("src/parser.rs")],
            shared_touched: vec![PathBuf::from("src/error.rs"), PathBuf::from("Cargo.toml")],
            self_check: CheckStatus::Pass,
            test: CheckStatus::Pass,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        let parsed: TranslationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.module_key, result.module_key);
        assert_eq!(parsed.status, AgentStatus::AgentDone);
        assert_eq!(parsed.own_files, vec![PathBuf::from("src/parser.rs")]);
        assert_eq!(parsed.shared_touched.len(), 2);
        assert_eq!(parsed.self_check, CheckStatus::Pass);
        assert_eq!(parsed.test, CheckStatus::Pass);
    }

    #[test]
    fn test_translation_result_with_failure() {
        let result = TranslationResult {
            module_key: "file:src/broken.ts".to_string(),
            status: AgentStatus::Failed,
            own_files: vec![PathBuf::from("src/broken.rs")],
            shared_touched: vec![],
            self_check: CheckStatus::Fail {
                message: "error[E0308]: mismatched types".to_string(),
            },
            test: CheckStatus::Skipped,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        let parsed: TranslationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, AgentStatus::Failed);
        assert_eq!(
            parsed.self_check,
            CheckStatus::Fail {
                message: "error[E0308]: mismatched types".to_string(),
            }
        );
        assert_eq!(parsed.test, CheckStatus::Skipped);
    }

    #[test]
    fn test_json_snake_case_keys() {
        let dispatch = TranslationDispatch {
            module_key: "file:src/a.ts".to_string(),
            worktree_path: PathBuf::from(".wt/a"),
            dependency_interfaces: vec![],
            porting_rules: PortingRules::default(),
        };
        let json = serde_json::to_string(&dispatch).unwrap();
        // 验证 JSON key 是 snake_case。
        assert!(json.contains("\"module_key\""));
        assert!(json.contains("\"worktree_path\""));
        assert!(json.contains("\"dependency_interfaces\""));
        assert!(json.contains("\"porting_rules\""));
        assert!(json.contains("\"prefer_existing_api\""));
        assert!(json.contains("\"allow_escape_hatch\""));
        assert!(json.contains("\"no_break_shared_api\""));
        assert!(json.contains("\"append_only\""));

        let result = TranslationResult {
            module_key: "file:src/a.ts".to_string(),
            status: AgentStatus::AgentDone,
            own_files: vec![],
            shared_touched: vec![],
            self_check: CheckStatus::Pass,
            test: CheckStatus::Skipped,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"module_key\""));
        assert!(json.contains("\"own_files\""));
        assert!(json.contains("\"shared_touched\""));
        assert!(json.contains("\"self_check\""));
        assert!(json.contains("\"agent_done\""));
    }

    #[test]
    fn test_empty_dependency_interfaces() {
        // 叶模块无依赖的场景。
        let dispatch = TranslationDispatch {
            module_key: "file:src/constants.ts".to_string(),
            worktree_path: PathBuf::from(".wt/constants"),
            dependency_interfaces: vec![],
            porting_rules: PortingRules::default(),
        };
        let json = serde_json::to_string(&dispatch).unwrap();
        let parsed: TranslationDispatch = serde_json::from_str(&json).unwrap();
        assert!(parsed.dependency_interfaces.is_empty());
    }
}
