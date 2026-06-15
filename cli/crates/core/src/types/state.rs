//! 迁移状态机类型定义。
//!
//! 参照 `docs/design/02-architecture.md § 3.4` 和
//! `docs/design/09-appendix-schemas.md § 附录 A`。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::{Display, EnumString};

use super::common::{RiskLevel, SourceLang, Timestamp};

/// 项目级状态机节点（编排器状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProjectState {
    /// 初始化阶段。
    Init,
    /// 项目画像分析。
    Profile,
    /// 迁移计划生成。
    Plan,
    /// Rust 工程脚手架。
    Scaffold,
    /// Sprint 循环迁移。
    SprintLoop,
    /// 毕业（迁移完成）。
    Graduate,
}

impl ProjectState {
    /// 检查是否允许从当前状态转换到目标状态。
    ///
    /// 合法转换路径：Init → Profile → Plan → Scaffold → SprintLoop → Graduate。
    pub fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Init, Self::Profile)
                | (Self::Profile, Self::Plan)
                | (Self::Plan, Self::Scaffold)
                | (Self::Scaffold, Self::SprintLoop)
                | (Self::SprintLoop, Self::Graduate)
        )
    }
}

/// 模块级状态（模块迁移生命周期）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ModuleStatus {
    Pending,
    Translating,
    CompileFixing,
    Testing,
    Reviewing,
    Done,
    DegradeFfi,
    DegradeManual,
    DegradeSkip,
    Paused,
    Blocked,
}

impl ModuleStatus {
    /// 是否为终态（done 或 degrade_*）。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::DegradeFfi | Self::DegradeManual | Self::DegradeSkip
        )
    }

    /// 是否为降级状态。
    pub fn is_degraded(self) -> bool {
        matches!(
            self,
            Self::DegradeFfi | Self::DegradeManual | Self::DegradeSkip
        )
    }

    /// 检查模块状态是否允许从当前状态转换到目标状态。
    ///
    /// 严格对齐 `docs/design/09-appendix-schemas.md` 的模块状态转换图：
    /// ```text
    /// pending → translating → compile_fixing → testing → reviewing → done
    ///               └（cargo check 首次通过）→ testing
    ///         compile_fixing（3 轮失败）/ testing（不可修复）→ paused
    ///         paused → translating | degrade_ffi | degrade_manual | degrade_skip
    ///         degrade_* →（/migrate run --force 恢复）→ translating
    /// ```
    /// 补充语义：
    /// - 任意可阻塞活跃态（pending/translating/compile_fixing/testing/reviewing/paused）
    ///   可因依赖未完成进入 `blocked`；`blocked` 恢复回这些活跃态之一
    ///   （实际恢复目标由 `pre_blocked_status` 决定，此处只校验"是可阻塞活跃态"）。
    /// - **`done` 是唯一真终态**；`degrade_*` 非终态，可经 `--force` 恢复到 `translating`
    ///   （设计 §0.3 Step / 状态图恢复边）。
    pub fn can_transition_to(self, target: Self) -> bool {
        use ModuleStatus::*;
        // 可被阻塞的活跃态：可进入 blocked，也是 blocked 恢复的合法目标。
        let blockable = |s: ModuleStatus| {
            matches!(
                s,
                Pending | Translating | CompileFixing | Testing | Reviewing | Paused
            )
        };
        match self {
            Pending => matches!(target, Translating | Blocked),
            Translating => matches!(target, CompileFixing | Testing | Blocked),
            CompileFixing => matches!(target, Testing | Paused | Blocked),
            Testing => matches!(target, Reviewing | Paused | Blocked),
            Reviewing => matches!(target, Done | Blocked),
            Paused => matches!(
                target,
                Translating | DegradeFfi | DegradeManual | DegradeSkip | Blocked
            ),
            // degrade_* 非真终态：可经 --force 恢复到 translating。
            DegradeFfi | DegradeManual | DegradeSkip => matches!(target, Translating),
            // done 是唯一真终态，不可再转出（保护断点续传不被非法回退覆盖）。
            Done => false,
            // blocked 恢复到原活跃态（目标由 pre_blocked_status 决定）。
            Blocked => blockable(target),
        }
    }
}

/// 翻译阶段（Phase A 忠实翻译 / Phase B 惯用化优化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TranslationPhase {
    A,
    B,
}

/// 状态历史条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub state: ProjectState,
    pub entered_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<Timestamp>,
}

/// 项目基本信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub source_language: SourceLang,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<String>,
    #[serde(default)]
    pub source_loc: u64,
    pub created_at: Timestamp,
}

/// Sprint 历史条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprintEntry {
    pub id: u32,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(default)]
    pub target_modules: Vec<String>,
    #[serde(default)]
    pub completed_modules: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// 本 Sprint 使用的 PORTING.md 版本号。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub porting_md_version: Option<String>,
}

/// Sprint 状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprintState {
    pub current: u32,
    #[serde(default)]
    pub history: Vec<SprintEntry>,
}

/// 模块迁移尝试记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub timestamp: Timestamp,
    pub result: String,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

/// 单个模块的迁移状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleState {
    pub status: ModuleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substatus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<u32>,
    #[serde(default)]
    pub attempts: Vec<AttemptRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_pass_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<u32>,
    #[serde(default)]
    pub known_differences: u32,
    #[serde(default = "default_risk")]
    pub risk: RiskLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_a_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_a_audit_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_blocked_status: Option<ModuleStatus>,
}

fn default_risk() -> RiskLevel {
    RiskLevel::Low
}

/// 从内部 module key 派生「人类友好显示名」（纯函数，不改变内部 key）。
///
/// 内部 module key 形如 NodeId 原值（见 `types/common.rs`）：
/// `file:src/utils.ts`、`file:src/foo/bar.ts`，对人类不友好。本函数归一化为
/// 可辨识的短名，供 `--human` 标志在 CLI 输出中附带显示映射：
///
/// 归一化规则（保守、无歧义）：
/// 1. 去掉 NodeType 前缀（`file:` / `function:` 等，即第一个 `:` 之前的部分）；
///    无前缀时按原样处理路径。
/// 2. 去掉常见源码根目录前缀（`src/`），保留其余目录层级以保证可辨识。
/// 3. 去掉文件名扩展名（仅末段 basename 的最后一个 `.` 之后）。
/// 4. 统一路径分隔符为 `/`（兼容 Windows 风格 `\`）。
///
/// 该函数只做显示派生，**不保证唯一性**（不同 key 可能映射到同名），故调用方
/// 应将其作为附加显示字段，而非主键。
///
/// # 示例
/// ```
/// use rustmigrate_core::types::state::humanize_module_key;
/// assert_eq!(humanize_module_key("file:src/utils.ts"), "utils");
/// assert_eq!(humanize_module_key("file:src/foo/bar.ts"), "foo/bar");
/// assert_eq!(humanize_module_key("file:lib/index.ts"), "lib/index");
/// assert_eq!(humanize_module_key("utils.ts"), "utils");
/// ```
pub fn humanize_module_key(key: &str) -> String {
    // 1. 去掉 NodeType 前缀：NodeId 形如 `file:src/utils.ts`，类型前缀不含路径分隔符，
    //    故「第一个 `:` 之前不含 `/`、`\`」可安全判定为类型前缀。
    let after_prefix = match key.split_once(':') {
        Some((prefix, rest)) if !prefix.contains('/') && !prefix.contains('\\') => rest,
        _ => key,
    };

    // 4. 统一分隔符。
    let normalized = after_prefix.replace('\\', "/");

    // 2. 去掉常见源码根目录前缀。
    let without_root = normalized
        .strip_prefix("src/")
        .unwrap_or(normalized.as_str());

    // 3. 去掉末段 basename 的扩展名，保留目录层级。
    match without_root.rsplit_once('/') {
        Some((dir, base)) => format!("{dir}/{}", strip_extension(base)),
        None => strip_extension(without_root).to_owned(),
    }
}

/// 去掉文件名最后一个扩展名（无扩展名或隐藏文件如 `.gitignore` 则原样返回）。
fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// SubAgent 调用记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubAgentCall {
    pub step_index: u32,
    pub subagent_name: String,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Timestamp>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// 迁移元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationMetadata {
    #[serde(default)]
    pub graph_build_completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_build_completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_token: Option<String>,
}

/// 迁移状态文件 (migration-state.json) 的完整结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationStateFile {
    /// schema 版本号；JSON 键为 `schema_version`，对齐设计 06 §10.0.2 / §10.7。
    #[serde(rename = "schema_version")]
    pub version: String,
    pub state: ProjectState,
    #[serde(default)]
    pub state_history: Vec<StateHistoryEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<SprintState>,
    #[serde(default)]
    pub modules: HashMap<String, ModuleState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_ref: Option<String>,
    #[serde(default)]
    pub subagent_calls: Vec<SubAgentCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MigrationMetadata>,
}

#[cfg(test)]
mod tests {
    use super::ModuleStatus::*;
    use super::*;

    /// 全量校验 `ModuleStatus::can_transition_to` 的转换矩阵：
    /// 白名单严格对齐 `docs/design/09-appendix-schemas.md` 状态转换图，
    /// 对 11×11 笛卡尔积取反验证"白名单外皆非法"。
    #[test]
    fn test_module_transition_matrix() {
        let all = [
            Pending,
            Translating,
            CompileFixing,
            Testing,
            Reviewing,
            Done,
            DegradeFfi,
            DegradeManual,
            DegradeSkip,
            Paused,
            Blocked,
        ];
        // (from, to) 合法白名单（依据设计状态转换图）。
        let legal: &[(ModuleStatus, ModuleStatus)] = &[
            // 主链
            (Pending, Translating),
            (Translating, CompileFixing),
            (Translating, Testing),
            (CompileFixing, Testing),
            (Testing, Reviewing),
            (Reviewing, Done),
            // 失败 → paused
            (CompileFixing, Paused),
            (Testing, Paused),
            // paused 出边
            (Paused, Translating),
            (Paused, DegradeFfi),
            (Paused, DegradeManual),
            (Paused, DegradeSkip),
            // degrade_* --force 恢复
            (DegradeFfi, Translating),
            (DegradeManual, Translating),
            (DegradeSkip, Translating),
            // 任意可阻塞活跃态 → blocked
            (Pending, Blocked),
            (Translating, Blocked),
            (CompileFixing, Blocked),
            (Testing, Blocked),
            (Reviewing, Blocked),
            (Paused, Blocked),
            // blocked 恢复到原活跃态
            (Blocked, Pending),
            (Blocked, Translating),
            (Blocked, CompileFixing),
            (Blocked, Testing),
            (Blocked, Reviewing),
            (Blocked, Paused),
        ];
        for &from in &all {
            for &to in &all {
                let want = legal.contains(&(from, to));
                assert_eq!(
                    from.can_transition_to(to),
                    want,
                    "{from} -> {to} 期望 {want}"
                );
            }
        }
    }

    #[test]
    fn test_degrade_force_recovery_to_translating() {
        // 设计：degrade_* 可经 --force 恢复到 translating（非真终态）。
        for st in [DegradeFfi, DegradeManual, DegradeSkip] {
            assert!(
                st.can_transition_to(Translating),
                "{st} 应允许 --force 恢复到 translating"
            );
            // 但不能直达其他状态。
            assert!(!st.can_transition_to(Done));
            assert!(!st.can_transition_to(Testing));
        }
    }

    #[test]
    fn test_done_is_only_true_terminal() {
        // done 不可转出任何状态。
        for to in [Translating, Testing, Reviewing, Pending, Blocked, Paused] {
            assert!(!Done.can_transition_to(to), "done 不应可转出到 {to}");
        }
    }

    #[test]
    fn test_humanize_module_key() {
        // 典型 NodeId（file 前缀 + src 根 + .ts 扩展）。
        assert_eq!(humanize_module_key("file:src/utils.ts"), "utils");
        // 保留中间目录层级。
        assert_eq!(humanize_module_key("file:src/foo/bar.ts"), "foo/bar");
        // 非 src 根目录前缀保留。
        assert_eq!(humanize_module_key("file:lib/index.ts"), "lib/index");
        // 无 file 前缀（裸路径）。
        assert_eq!(humanize_module_key("utils.ts"), "utils");
        assert_eq!(humanize_module_key("src/a/b.ts"), "a/b");
        // 其他 NodeType 前缀同样剥离（module key 实际只用 file 型，此处仅验证健壮性）：
        // `function:` 前缀剥离后剩 `src/utils.ts:clamp`，去 src/ 根 → `utils.ts:clamp`，
        // basename 无 `/`，扩展名按最后一个 `.` 切分（`utils` | `ts:clamp`）→ `utils`。
        assert_eq!(humanize_module_key("function:src/utils.ts:clamp"), "utils");
        // 无扩展名。
        assert_eq!(humanize_module_key("file:src/mod"), "mod");
        // Windows 风格分隔符归一。
        assert_eq!(humanize_module_key("file:src\\foo\\bar.ts"), "foo/bar");
        // 隐藏文件（前导点）不被误删。
        assert_eq!(humanize_module_key("file:.gitignore"), ".gitignore");
        // 多重扩展只去末段。
        assert_eq!(humanize_module_key("file:src/types.d.ts"), "types.d");
        // 空字符串安全。
        assert_eq!(humanize_module_key(""), "");
    }

    #[test]
    fn test_no_bypass_review_to_done() {
        // 只有 reviewing 能到 done，其余活跃态直达 done 均非法（防越权标完成）。
        for from in [Pending, Translating, CompileFixing, Testing] {
            assert!(
                !from.can_transition_to(Done),
                "{from} 不应越过 reviewing 直达 done"
            );
        }
        assert!(Reviewing.can_transition_to(Done));
    }
}
