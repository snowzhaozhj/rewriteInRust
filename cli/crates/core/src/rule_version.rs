//! 迁移规则版本一致性校验（M4-GOV-01，Sprint F）。
//!
//! **权威规则版本清单**（`rule-registry.json`）是核心迁移规则（RULE-N）**当前版本**的单一真相源。
//! 各语言适配器 `porting-template.md` 的 frontmatter `rule_version` 声明该模板生成/更新时依据的
//! 核心规则版本（如 `RULE-3:v2.0.0`）。核心规则发生破坏性升级后模板未同步 bump，即产生「陈旧」
//! 漂移——新旧规则版本混用会打破 `05 §6.2`「项目专有规则优先」约束。本模块提供**确定性比对**：
//! 加载权威清单 + 解析各模板 frontmatter → 报告缺失 / 版本不符 / 未知规则。
//!
//! 设计权威：
//! - `docs/design/06-plugin-structure.md § 11.1`（`[rules].enforce_rule_version_consistency` 开关 +
//!   R3-D7-03「适配器与核心规则的版本同步」）
//! - `docs/design/05-documentation-system.md § 6.2`（规则版本管理与代码一致性）
//! - `docs/decisions/014-rule-version-registry.md`（权威清单选型 + CLI 命令面）
//!
//! 命令面复用 `profile/tools.rs` 的 serde JSON 加载框架（`load_analysis_tools` 同构）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MigrateError, Result};

/// 适配器模板文件名（各适配器目录下唯一，MDR-009 两文件契约之一）。
const PORTING_TEMPLATE: &str = "porting-template.md";

/// 权威规则版本清单（`rule-registry.json`）：核心 `RULE-N` → 当前版本（如 `v1.0.0`）。
///
/// 手工维护的**小型**治理清单（当前 9 条核心规则），是规则版本的单一真相源——与已砍的
/// `index.json`（自动聚合模块数据、YAGNI）不同：本清单直接支撑 GOV-01 一致性校验，非投机数据模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRegistry {
    /// 清单格式版本（与被追踪的规则版本区分，用于清单结构自身演进）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    /// 核心规则当前版本表：`"RULE-2" -> "v1.0.0"`。
    pub rules: BTreeMap<String, String>,
}

/// 从 JSON 文件加载权威规则版本清单。
///
/// 文件不存在返回 [`MigrateError::FileNotFound`]；JSON 非法经 `#[from]` 转 [`MigrateError::Json`]。
pub fn load_rule_registry(path: &Path) -> Result<RuleRegistry> {
    if !path.exists() {
        return Err(MigrateError::FileNotFound(path.to_path_buf()));
    }
    let content = std::fs::read_to_string(path)?;
    let registry: RuleRegistry = serde_json::from_str(&content)?;
    if registry.rules.is_empty() {
        return Err(MigrateError::Config(format!(
            "规则清单 {} 的 rules 为空，无法校验一致性",
            path.display()
        )));
    }
    Ok(registry)
}

/// 解析 `porting-template.md` frontmatter 中的 `rule_version` 字段。
///
/// frontmatter 为文件**首个** `---` 与下一个 `---` 之间的 YAML 块；`rule_version` 值形如
/// `RULE-2:v1.0.0, RULE-3:v1.0.0`（逗号分隔，每项 `RULE-N:版本`）。返回 `RULE-N -> 版本` 映射。
///
/// 缺 frontmatter / 缺 `rule_version` / 项格式非法 / 空值均返回 [`MigrateError::Config`]（显式失败，
/// 不静默——陈旧检测的前提是模板确实声明了版本）。
pub fn parse_template_rule_version(md_content: &str) -> Result<BTreeMap<String, String>> {
    let frontmatter = extract_frontmatter(md_content).ok_or_else(|| {
        MigrateError::Config(
            "porting-template.md 缺少 YAML frontmatter（首行须为 `---`，闭合行 `---`）".to_owned(),
        )
    })?;
    let raw = frontmatter
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("rule_version:"))
        .ok_or_else(|| {
            MigrateError::Config(
                "porting-template.md frontmatter 缺少 rule_version 字段".to_owned(),
            )
        })?;

    let mut map = BTreeMap::new();
    for seg in raw.split(',') {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let (rule, version) = seg.split_once(':').ok_or_else(|| {
            MigrateError::Config(format!(
                "rule_version 项格式非法（应为 `RULE-N:版本`）: `{seg}`"
            ))
        })?;
        let (rule, version) = (rule.trim(), version.trim());
        if rule.is_empty() || version.is_empty() {
            return Err(MigrateError::Config(format!(
                "rule_version 项 rule 名或版本为空: `{seg}`"
            )));
        }
        map.insert(rule.to_owned(), version.to_owned());
    }
    if map.is_empty() {
        return Err(MigrateError::Config(
            "rule_version 字段为空，未声明任何规则版本".to_owned(),
        ));
    }
    Ok(map)
}

/// 抽取 Markdown 文件的 YAML frontmatter 文本块（不含首尾 `---` 界定行）。
///
/// 仅当文件以 `---\n`（或 `---\r\n`）开头且后续存在闭合 `---` 行时返回 `Some`，否则 `None`。
fn extract_frontmatter(md: &str) -> Option<&str> {
    let rest = md.strip_prefix("---")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    // 闭合界定符须是独占一行的 `---`（行首），故匹配换行后紧跟 `---`。
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

/// 规则版本不一致的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleVersionIssueKind {
    /// 权威清单中的规则未在模板 `rule_version` 声明（模板覆盖缺失）。
    MissingInTemplate,
    /// 模板声明的版本与权威清单不符（陈旧或超前）。
    VersionMismatch,
    /// 模板声明了权威清单中不存在的规则（typo 或清单遗漏）。
    UnknownRule,
}

/// 单个模板的一条规则版本不一致项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleVersionIssue {
    /// 规则 ID（如 `RULE-3`）。
    pub rule: String,
    /// 不一致类型。
    pub kind: RuleVersionIssueKind,
    /// 权威清单版本（`UnknownRule` 时为 `None`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// 模板声明版本（`MissingInTemplate` 时为 `None`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

/// 校验单个模板 `rule_version` 与权威清单一致性，返回全部不一致项（空 = 一致）。
///
/// 校验规则（严格双向）：
/// 1. 清单每条规则须在模板声明且版本一致——缺失 → `MissingInTemplate`，版本不符 → `VersionMismatch`；
/// 2. 模板声明的规则须在清单存在——否则 → `UnknownRule`。
///
/// 遍历顺序按 `BTreeMap` 字典序，输出确定性（便于测试与 diff）。
pub fn check_template_consistency(
    registry: &RuleRegistry,
    template_versions: &BTreeMap<String, String>,
) -> Vec<RuleVersionIssue> {
    let mut issues = Vec::new();
    // 权威清单侧：缺失 / 版本不符。
    for (rule, expected) in &registry.rules {
        match template_versions.get(rule) {
            None => issues.push(RuleVersionIssue {
                rule: rule.clone(),
                kind: RuleVersionIssueKind::MissingInTemplate,
                expected: Some(expected.clone()),
                actual: None,
            }),
            Some(actual) if actual != expected => issues.push(RuleVersionIssue {
                rule: rule.clone(),
                kind: RuleVersionIssueKind::VersionMismatch,
                expected: Some(expected.clone()),
                actual: Some(actual.clone()),
            }),
            Some(_) => {}
        }
    }
    // 模板侧：未知规则。
    for (rule, actual) in template_versions {
        if !registry.rules.contains_key(rule) {
            issues.push(RuleVersionIssue {
                rule: rule.clone(),
                kind: RuleVersionIssueKind::UnknownRule,
                expected: None,
                actual: Some(actual.clone()),
            });
        }
    }
    issues
}

/// 单个适配器模板的校验结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TemplateRuleCheck {
    /// 适配器语言目录名（如 `typescript`）。
    pub adapter: String,
    /// 模板文件路径（取调用方传入的 adapters_dir 拼接，保留原样便于定位）。
    pub template_path: String,
    /// 不一致项（空 = 一致）。
    pub issues: Vec<RuleVersionIssue>,
}

impl TemplateRuleCheck {
    /// 该模板 `rule_version` 是否与权威清单完全一致。
    pub fn is_consistent(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 扫描适配器根目录（`<adapters_dir>/<lang>/porting-template.md`），逐个校验 `rule_version`。
///
/// 返回按适配器名字典序排列的结果。不含 `porting-template.md` 的子目录（如 `references/`）跳过。
/// 目录不存在返回 [`MigrateError::FileNotFound`]；无任何模板返回 [`MigrateError::Config`]（避免
/// 「零模板」被误判为「全一致」的静默通过）。模板 frontmatter 解析失败直接上抛（无法表达为 issue）。
pub fn check_adapters_dir(
    registry: &RuleRegistry,
    adapters_dir: &Path,
) -> Result<Vec<TemplateRuleCheck>> {
    if !adapters_dir.is_dir() {
        return Err(MigrateError::FileNotFound(adapters_dir.to_path_buf()));
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(adapters_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut checks = Vec::new();
    for dir in dirs {
        let template = dir.join(PORTING_TEMPLATE);
        if !template.exists() {
            continue; // 非适配器目录（无模板）跳过
        }
        let adapter = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let content = std::fs::read_to_string(&template)?;
        let versions = parse_template_rule_version(&content)
            .map_err(|e| MigrateError::Config(format!("解析 {} 失败: {e}", template.display())))?;
        checks.push(TemplateRuleCheck {
            adapter,
            template_path: template.to_string_lossy().into_owned(),
            issues: check_template_consistency(registry, &versions),
        });
    }
    if checks.is_empty() {
        return Err(MigrateError::Config(format!(
            "适配器目录 {} 下未找到任何 */{PORTING_TEMPLATE}",
            adapters_dir.display()
        )));
    }
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> RuleRegistry {
        RuleRegistry {
            schema_version: Some("1.0".to_owned()),
            rules: [("RULE-2", "v1.0.0"), ("RULE-3", "v1.0.0")]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        }
    }

    fn versions(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn parses_rule_version_line() {
        let md = "---\nlanguage_id: go\nrule_version: RULE-2:v1.0.0, RULE-3:v2.0.0\n---\n# body\n";
        let got = parse_template_rule_version(md).unwrap();
        assert_eq!(got, versions(&[("RULE-2", "v1.0.0"), ("RULE-3", "v2.0.0")]));
    }

    #[test]
    fn parse_missing_frontmatter_errors() {
        assert!(parse_template_rule_version("# 无 frontmatter\n正文").is_err());
    }

    #[test]
    fn parse_missing_rule_version_errors() {
        let md = "---\nlanguage_id: go\n---\n正文";
        assert!(parse_template_rule_version(md).is_err());
    }

    #[test]
    fn parse_malformed_item_errors() {
        let md = "---\nrule_version: RULE-2:v1.0.0, BADITEM\n---\n";
        assert!(parse_template_rule_version(md).is_err());
    }

    #[test]
    fn consistent_template_has_no_issues() {
        let v = versions(&[("RULE-2", "v1.0.0"), ("RULE-3", "v1.0.0")]);
        assert!(check_template_consistency(&registry(), &v).is_empty());
    }

    #[test]
    fn detects_version_mismatch() {
        let v = versions(&[("RULE-2", "v1.0.0"), ("RULE-3", "v0.9.0")]);
        let issues = check_template_consistency(&registry(), &v);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].rule, "RULE-3");
        assert_eq!(issues[0].kind, RuleVersionIssueKind::VersionMismatch);
        assert_eq!(issues[0].expected.as_deref(), Some("v1.0.0"));
        assert_eq!(issues[0].actual.as_deref(), Some("v0.9.0"));
    }

    #[test]
    fn detects_missing_in_template() {
        let v = versions(&[("RULE-2", "v1.0.0")]);
        let issues = check_template_consistency(&registry(), &v);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, RuleVersionIssueKind::MissingInTemplate);
        assert_eq!(issues[0].rule, "RULE-3");
    }

    #[test]
    fn detects_unknown_rule() {
        let v = versions(&[
            ("RULE-2", "v1.0.0"),
            ("RULE-3", "v1.0.0"),
            ("RULE-99", "v1.0.0"),
        ]);
        let issues = check_template_consistency(&registry(), &v);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, RuleVersionIssueKind::UnknownRule);
        assert_eq!(issues[0].rule, "RULE-99");
        assert_eq!(issues[0].expected, None);
    }

    #[test]
    fn issue_order_is_deterministic_by_rule() {
        // 同时缺失 RULE-3 与出现未知 RULE-99：清单侧先出（字典序），模板侧未知后出。
        let v = versions(&[("RULE-2", "v1.0.0"), ("RULE-99", "v1.0.0")]);
        let issues = check_template_consistency(&registry(), &v);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].rule, "RULE-3"); // MissingInTemplate（清单侧）
        assert_eq!(issues[1].rule, "RULE-99"); // UnknownRule（模板侧）
    }
}
