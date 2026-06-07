//! 验证规则定义。
//!
//! 参照设计文档中的三层验证体系（Tier0 / Tier1 / Tier2）。

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::Result;

/// 验证规则检查函数类型别名。
type CheckFn = Box<dyn Fn(&Path) -> Result<(bool, Option<String>)> + Send + Sync>;

/// 验证层级。
///
/// - Tier0: 基本编译检查（cargo check, clippy）— 必须通过
/// - Tier1: 覆盖率达标 — 应该通过
/// - Tier2: 高级验证（proptest, fuzz）— 可选
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValidationTier {
    /// 基本编译检查。
    Tier0,
    /// 覆盖率达标。
    Tier1,
    /// 高级验证（proptest/fuzz）。
    Tier2,
}

impl std::fmt::Display for ValidationTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tier0 => write!(f, "Tier0"),
            Self::Tier1 => write!(f, "Tier1"),
            Self::Tier2 => write!(f, "Tier2"),
        }
    }
}

/// 单条验证规则的检查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleResult {
    /// 规则名称。
    pub name: String,
    /// 所属层级。
    pub tier: ValidationTier,
    /// 是否通过。
    pub passed: bool,
    /// 失败时的消息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 验证规则。
///
/// 包含规则名称、层级和检查函数。
/// 检查函数接受项目根路径，返回 `(passed, message)` 元组。
pub struct ValidationRule {
    /// 规则名称。
    pub name: String,
    /// 所属层级。
    pub tier: ValidationTier,
    /// 检查函数：接受项目根路径，返回 (是否通过, 失败消息)。
    pub check: CheckFn,
}

impl std::fmt::Debug for ValidationRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidationRule")
            .field("name", &self.name)
            .field("tier", &self.tier)
            .finish()
    }
}

impl ValidationRule {
    /// 创建新的验证规则。
    pub fn new(
        name: impl Into<String>,
        tier: ValidationTier,
        check: impl Fn(&Path) -> Result<(bool, Option<String>)> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            tier,
            check: Box::new(check),
        }
    }

    /// 执行检查并返回结果。
    pub fn run(&self, project_root: &Path) -> Result<RuleResult> {
        let (passed, message) = (self.check)(project_root)?;
        Ok(RuleResult {
            name: self.name.clone(),
            tier: self.tier,
            passed,
            message,
        })
    }
}

/// 获取默认规则集。
///
/// 包含 Tier0 / Tier1 / Tier2 基本规则。
/// 注：实际执行 cargo 命令需要在 CLI 层调用，这里定义规则结构和检查逻辑。
pub fn default_rules() -> Vec<ValidationRule> {
    vec![
        // Tier0: cargo check 通过
        ValidationRule::new(
            "cargo_check",
            ValidationTier::Tier0,
            |project_root: &Path| {
                let output = std::process::Command::new("cargo")
                    .arg("check")
                    .current_dir(project_root)
                    .output()?;
                if output.status.success() {
                    Ok((true, None))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Ok((
                        false,
                        Some(format!(
                            "cargo check 失败: {}",
                            stderr.chars().take(500).collect::<String>()
                        )),
                    ))
                }
            },
        ),
        // Tier0: cargo clippy 通过
        ValidationRule::new(
            "cargo_clippy",
            ValidationTier::Tier0,
            |project_root: &Path| {
                let output = std::process::Command::new("cargo")
                    .args(["clippy", "--", "-D", "warnings"])
                    .current_dir(project_root)
                    .output()?;
                if output.status.success() {
                    Ok((true, None))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Ok((
                        false,
                        Some(format!(
                            "cargo clippy 失败: {}",
                            stderr.chars().take(500).collect::<String>()
                        )),
                    ))
                }
            },
        ),
        // Tier1: 覆盖率达标（占位检查 — 需要 cargo-tarpaulin 或类似工具）
        ValidationRule::new(
            "coverage_threshold",
            ValidationTier::Tier1,
            |_project_root: &Path| {
                // M1 阶段暂不实际计算覆盖率，标记为跳过
                Ok((true, Some("覆盖率检查暂未实现，默认通过".to_owned())))
            },
        ),
        // Tier2: proptest/fuzz（占位检查）
        ValidationRule::new(
            "proptest_fuzz",
            ValidationTier::Tier2,
            |_project_root: &Path| {
                // M1 阶段暂不实际运行 proptest/fuzz
                Ok((
                    true,
                    Some("proptest/fuzz 检查暂未实现，默认通过".to_owned()),
                ))
            },
        ),
    ]
}

/// 按层级筛选规则。
pub fn rules_for_tier(rules: &[ValidationRule], tier: ValidationTier) -> Vec<&ValidationRule> {
    rules.iter().filter(|r| r.tier == tier).collect()
}

/// 执行指定层级及以下的所有规则。
pub fn run_rules_up_to_tier(
    rules: &[ValidationRule],
    tier: ValidationTier,
    project_root: &Path,
) -> Result<Vec<RuleResult>> {
    let tier_ord = match tier {
        ValidationTier::Tier0 => 0,
        ValidationTier::Tier1 => 1,
        ValidationTier::Tier2 => 2,
    };

    let mut results = Vec::new();
    for rule in rules {
        let rule_ord = match rule.tier {
            ValidationTier::Tier0 => 0,
            ValidationTier::Tier1 => 1,
            ValidationTier::Tier2 => 2,
        };
        if rule_ord <= tier_ord {
            results.push(rule.run(project_root)?);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_validation_tier_display() {
        assert_eq!(ValidationTier::Tier0.to_string(), "Tier0");
        assert_eq!(ValidationTier::Tier1.to_string(), "Tier1");
        assert_eq!(ValidationTier::Tier2.to_string(), "Tier2");
    }

    #[test]
    fn test_validation_rule_create_and_run() {
        let rule =
            ValidationRule::new("test_rule", ValidationTier::Tier0, |_path| Ok((true, None)));
        assert_eq!(rule.name, "test_rule");
        assert_eq!(rule.tier, ValidationTier::Tier0);

        let result = rule.run(Path::new("/tmp")).unwrap();
        assert!(result.passed);
        assert!(result.message.is_none());
    }

    #[test]
    fn test_validation_rule_failure_returns_message() {
        let rule = ValidationRule::new("fail_rule", ValidationTier::Tier0, |_path| {
            Ok((false, Some("编译失败".to_owned())))
        });
        let result = rule.run(Path::new("/tmp")).unwrap();
        assert!(!result.passed);
        assert_eq!(result.message.as_deref(), Some("编译失败"));
    }

    #[test]
    fn test_default_rules_all_tiers() {
        let rules = default_rules();
        assert!(rules.len() >= 4);

        let tier0: Vec<_> = rules
            .iter()
            .filter(|r| r.tier == ValidationTier::Tier0)
            .collect();
        let tier1: Vec<_> = rules
            .iter()
            .filter(|r| r.tier == ValidationTier::Tier1)
            .collect();
        let tier2: Vec<_> = rules
            .iter()
            .filter(|r| r.tier == ValidationTier::Tier2)
            .collect();

        assert!(
            tier0.len() >= 2,
            "Tier0 应至少有 cargo_check 和 cargo_clippy"
        );
        assert!(!tier1.is_empty(), "Tier1 应至少有覆盖率规则");
        assert!(!tier2.is_empty(), "Tier2 应至少有 proptest/fuzz 规则");
    }

    #[test]
    fn test_rules_for_tier_filter() {
        let rules = default_rules();
        let tier0 = rules_for_tier(&rules, ValidationTier::Tier0);
        assert!(tier0.iter().all(|r| r.tier == ValidationTier::Tier0));
    }

    #[test]
    fn test_run_rules_up_to_tier_filter() {
        let rules = vec![
            ValidationRule::new("t0", ValidationTier::Tier0, |_| Ok((true, None))),
            ValidationRule::new("t1", ValidationTier::Tier1, |_| Ok((true, None))),
            ValidationRule::new("t2", ValidationTier::Tier2, |_| Ok((true, None))),
        ];

        // 只执行 Tier0
        let results =
            run_rules_up_to_tier(&rules, ValidationTier::Tier0, Path::new("/tmp")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tier, ValidationTier::Tier0);

        // 执行到 Tier1
        let results =
            run_rules_up_to_tier(&rules, ValidationTier::Tier1, Path::new("/tmp")).unwrap();
        assert_eq!(results.len(), 2);

        // 执行到 Tier2（全部）
        let results =
            run_rules_up_to_tier(&rules, ValidationTier::Tier2, Path::new("/tmp")).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_rule_result_serialize() {
        let result = RuleResult {
            name: "cargo_check".to_owned(),
            tier: ValidationTier::Tier0,
            passed: true,
            message: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("cargo_check"));
        assert!(json.contains("Tier0"));

        // 反序列化
        let parsed: RuleResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, result);
    }

    #[test]
    fn test_rule_result_failure_serialize() {
        let result = RuleResult {
            name: "clippy".to_owned(),
            tier: ValidationTier::Tier0,
            passed: false,
            message: Some("warning found".to_owned()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: RuleResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message.as_deref(), Some("warning found"));
    }
}
