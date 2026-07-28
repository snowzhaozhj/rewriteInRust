//! 译后签批门判定（MDR-019）。
//!
//! `reviewing → done` 是迁移的**最终签批门**（设计 `02 § 3.4` / `03 § 7.4`「不自动宣布成功」）。
//! 本模块提供该门的确定性判定：
//!
//! 1. **强制人工清单**（[`mandatory_reasons`]）——命中任一即 [`GateDecision::MandatoryManual`]，
//!    **任何策略都不得放行**，只能人签批。
//! 2. **自动放行窄策略**（[`evaluate_policies`]）——仅当用户在 `[review_gate]
//!    .auto_approve_policies` 预签、且该策略的 state 层条件全过时，模块才 *可能* 自动放行；
//!    实际放行还须编排器逐项 attest 产物级检查（CLI 判不了的部分，见
//!    [`PolicySpec::required_attestations`]）。
//! 3. **证据包索引**（[`collect_evidence`]）——扫描 `intermediate/` 下与本模块相关的**实际存在**
//!    文件，供编排器展示；不构造「预期路径」（命名约定属 plugin 层，CLI 猜路径只会假报缺失）。
//!
//! 判定与 IO 分离：[`judge`] 是纯函数（可单测），[`collect_evidence`] 做目录扫描。

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use strum::Display;

use crate::types::config::ReviewGateConfig;
use crate::types::state::{DangerProvenance, ModuleState, ModuleStatus};

/// 内置策略：全机械合批组（`composite_kind=batch`）的窄合取放行。
pub const POLICY_BATCH_MECHANICAL: &str = "batch_mechanical";
/// 内置策略：无人值守模式下的放行（测试全通过 + 结构门过 + 覆盖率达阈值）。
pub const POLICY_HEADLESS_DEFAULT: &str = "headless_default";

/// substatus 值：已判定需人签批，正在等人（MDR-019 译后签批门的停门标记）。
pub const SUBSTATUS_AWAITING_FINAL_REVIEW: &str = "awaiting_final_review";

/// 允许自动放行的「干净」substatus 白名单（`None` 亦视为干净）。
///
/// 语义：模块停在 `reviewing` 时，substatus 只应表达「agent 自检完成」或「已停门等签批」。
/// 任何其他值（`phase_b_optimization_in_progress` / `incomplete*` / `requires_manual_review` /
/// 自由文本诊断）都意味着流程未走完或需人介入 → 强制人工（fail-closed，见
/// [`mandatory_reasons`] 的 `substatus_not_clean`）。
pub const CLEAN_SUBSTATUSES: &[&str] = &[
    crate::state::machine::SUBSTATUS_AGENT_DONE,
    SUBSTATUS_AWAITING_FINAL_REVIEW,
];

/// 判定结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GateDecision {
    /// 命中强制人工清单——任何策略都不得放行，须人签批（`state approve` 不带 `--by-policy`）。
    MandatoryManual,
    /// 未命中强制清单，且至少一个**已启用**策略的 state 层条件全过——编排器补齐
    /// `required_attestations` 后可 `state approve --by-policy <id>`。
    PolicyEligible,
    /// 未命中强制清单，但无可用策略（未启用 / 条件不满足）——须人签批。
    ManualRequired,
}

/// 强制人工的一条原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandatoryReason {
    /// 稳定机读码（编排器可据此分支，勿解析 `detail` 自然语言）。
    pub code: String,
    /// 人读细节（含具体数值 / 命中类别）。
    pub detail: String,
}

impl MandatoryReason {
    fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            detail: detail.into(),
        }
    }
}

/// 一个自动放行策略的评估结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    /// 策略 id。
    pub id: String,
    /// 是否已在 `[review_gate].auto_approve_policies` 预签。
    pub enabled: bool,
    /// state 层条件是否全过（**未启用的策略恒 `false`**——未预签即不可用）。
    pub eligible: bool,
    /// 不合格原因（`eligible=false` 时非空，含未启用 / 逐条条件拒因）。
    pub rejections: Vec<String>,
    /// 该策略要求编排器逐项声明的产物级检查（`state approve --attest <k>`，缺一即拒）。
    pub required_attestations: Vec<String>,
}

/// 内置策略的静态定义。
struct PolicySpec {
    id: &'static str,
    /// 该策略**特有**的产物级声明项。完整 `required_attestations` = 本项 ∪
    /// [`ORCHESTRATOR_MUST_CHECK`]（见 [`required_attestations`]）——CLI 判不了的强制人工项
    /// 必须逐项声明，否则策略等于「只声明容易的、跳过难的」。
    specific_attestations: &'static [&'static str],
}

const POLICY_SPECS: &[PolicySpec] = &[
    PolicySpec {
        id: POLICY_BATCH_MECHANICAL,
        // 机械 batch 无行为测试，故其特有门压在「导出符号一致 + 源码未变」上。
        specific_attestations: &["exports_match", "content_hash_unchanged"],
    },
    PolicySpec {
        id: POLICY_HEADLESS_DEFAULT,
        specific_attestations: &["tests_passed"],
    },
];

/// 编排器**必须**自查并逐项声明的强制项（CLI 无从判定：需读产物 / 跑命令 / 读 MDR）。
///
/// 对应 MDR-019 § 决策 2 强制人工清单中 CLI 判不了的部分。**这些项一并进入每个策略的
/// `required_attestations`**——只把它们输出成「提醒清单」而不要求声明，等于让策略绕过
/// 强制人工清单里最难判的那半（异构审查 I4）。任一项不成立时编排器**不得**声明，
/// 应停 `reviewing + awaiting_final_review` 等人签批。
///
/// `coverage_not_below_source` 单列的理由：MDR-019 要求「覆盖率低于**源**即强制人工」，
/// 但 CLI 不掌握源侧覆盖率（`state` 只有 Rust 侧 `coverage`），故绝对阈值由红线判、
/// 与源比较归声明。
pub const ORCHESTRATOR_MUST_CHECK: &[&str] = &[
    "l2_l3_differential_executable",
    "phase_b_new_paths_have_mdr",
    "public_api_unchanged",
    "error_semantics_unchanged",
    "concurrency_model_unchanged",
    "numeric_boundary_unchanged",
    "io_side_effects_unchanged",
    "coverage_not_below_source",
    "bug_replica_confirmed",
    "todo_port_zero",
];

/// 某策略的完整必需声明项 = 特有项 ∪ [`ORCHESTRATOR_MUST_CHECK`]（去重，稳定序：特有项在前）。
fn required_attestations(spec: &PolicySpec) -> Vec<String> {
    let mut out: Vec<String> = spec
        .specific_attestations
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for m in ORCHESTRATOR_MUST_CHECK {
        let owned = (*m).to_owned();
        if !out.contains(&owned) {
            out.push(owned);
        }
    }
    out
}

/// 判定所需的 state 层事实（原样回显，供编排器展示与排查）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateFacts {
    pub status: ModuleStatus,
    pub substatus: Option<String>,
    pub danger: Vec<String>,
    pub danger_provenance: DangerProvenance,
    pub known_differences: u32,
    pub test_pass_rate: Option<String>,
    pub coverage: Option<u32>,
    pub tier: Option<String>,
    pub composite_kind: Option<String>,
    pub member_files: Option<Vec<String>>,
    pub phase_a_audit_passed: Option<bool>,
}

impl StateFacts {
    pub fn from_module(m: &ModuleState) -> Self {
        Self {
            status: m.status,
            substatus: m.substatus.clone(),
            danger: m.danger.iter().map(|d| d.as_str().to_owned()).collect(),
            danger_provenance: m.danger_provenance,
            known_differences: m.known_differences,
            test_pass_rate: m.test_pass_rate.clone(),
            coverage: m.coverage,
            tier: m.tier.map(|t| t.to_string()),
            composite_kind: m.composite_kind.map(|k| k.to_string()),
            member_files: m.member_files.clone(),
            phase_a_audit_passed: m.phase_a_audit_passed,
        }
    }
}

/// 一条证据文件（实际存在于磁盘，非「预期路径」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// 证据类型（按文件名模式归类，无法归类时为 `other`）。
    pub kind: String,
    /// 相对 state 文件所在 `.rust-migration/` 目录的路径。
    pub path: String,
}

/// 纯判定结果（不含证据扫描，可在无文件系统的场景单测）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateJudgement {
    pub decision: GateDecision,
    pub mandatory_reasons: Vec<MandatoryReason>,
    pub policies: Vec<PolicyEvaluation>,
}

/// `state review-gate` 的完整报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewGateReport {
    /// 归一后的组代表 module key。
    pub module: String,
    pub decision: GateDecision,
    pub mandatory_reasons: Vec<MandatoryReason>,
    pub policies: Vec<PolicyEvaluation>,
    pub state_facts: StateFacts,
    /// 本次判定实际生效的覆盖率阈值（`[testing].coverage_threshold`）。
    /// **原样回显**：配置键拼错会静默回落默认 80（`serde` 忽略未知键），回显让它在人眼前暴露
    /// （静默失败审查 MEDIUM-4）。
    pub coverage_threshold: u32,
    /// 本次判定实际生效的预签策略清单（`[review_gate].auto_approve_policies` 原样回显）。
    /// 空 = 全停门；配置解析降级时也会是空，与 warnings 对照即可定位。
    pub enabled_policies: Vec<String>,
    /// 磁盘上实际存在的证据文件。
    pub evidence: Vec<EvidenceItem>,
    /// 编排器展示证据包时应补跑的命令（产出不落固定文件，故给命令而非路径）。
    pub evidence_commands: Vec<String>,
    /// 编排器必须自查的强制项（任一命中则不得自动放行）。
    pub orchestrator_must_check: Vec<String>,
}

/// 强制人工清单（CLI 可确定性判定的红线）——命中任一即任何策略都不得放行。
///
/// 逐条对应 MDR-019 § 决策 2：
/// - `danger_non_empty`：命中危险信号（并发 / 数值 / 反射 / IO / FFI / 全局可变）。
/// - `danger_provenance_untrusted`：分类来源不可信（未分类 / 分类不完整）——`danger=[]`
///   在此不代表安全（消解空值语义重载）。
/// - `known_differences_present`：已知行为差异非零（等价性有缺口）。
/// - `substatus_not_clean`：substatus 不在 [`CLEAN_SUBSTATUSES`] 白名单内（进行中 / 需人工 /
///   未知标记一律停门）。
/// - `phase_a_audit_failed`：Phase A 结构门未过（1:1 忠实性不成立）。
/// - `coverage_below_threshold`：覆盖率缺失或低于 `[testing].coverage_threshold`。
///   **机械合批组（`composite_kind=batch`）豁免此条**——它无运行时行为、覆盖率无语义
///   （run.md「Batch 组轻量路径」：机械文件无行为可测、编译即门禁），其门压在导出符号一致 +
///   content-hash 未变上。与源覆盖率的比较 CLI 判不了，归 `coverage_not_below_source` 声明。
pub fn mandatory_reasons(m: &ModuleState, coverage_threshold: u32) -> Vec<MandatoryReason> {
    let mut out = Vec::new();
    if !m.danger.is_empty() {
        let cats: Vec<&str> = m.danger.iter().map(|d| d.as_str()).collect();
        out.push(MandatoryReason::new(
            "danger_non_empty",
            format!("命中危险信号：{}", cats.join(", ")),
        ));
    }
    match m.danger_provenance {
        DangerProvenance::Classified => {}
        DangerProvenance::Unclassified => out.push(MandatoryReason::new(
            "danger_provenance_untrusted",
            "danger 未经分类器判定（--no-decompose 路径或旧版 state），空值不代表安全",
        )),
        DangerProvenance::PartiallyClassified => out.push(MandatoryReason::new(
            "danger_provenance_untrusted",
            "部分成员源文件读取失败、分类不完整，danger 可能漏项",
        )),
    }
    if m.known_differences > 0 {
        out.push(MandatoryReason::new(
            "known_differences_present",
            format!("存在 {} 项已知行为差异", m.known_differences),
        ));
    }
    // substatus 白名单（fail-closed）：只有「干净」的 substatus 才允许自动放行。
    // 不用子串启发式（`contains("incomplete")` 等）——静默失败审查 MEDIUM-8 实证它拦不住真实值：
    // `phase_b_optimization_in_progress` 是设计的正式值、会被 translating→testing→reviewing 一路带进
    // reviewing，既不含 `incomplete` 也不含 `requires_manual_review`，于是「Phase B 还在进行中」的模块
    // 反而无红线。改为白名单后：未知/进行中/需人工的 substatus 一律停门。
    if let Some(sub) = m.substatus.as_deref() {
        if !CLEAN_SUBSTATUSES.contains(&sub) {
            out.push(MandatoryReason::new(
                "substatus_not_clean",
                format!(
                    "substatus={sub} 不在允许自动放行的白名单内（{}）——进行中/需人工/未知标记一律停门",
                    CLEAN_SUBSTATUSES.join(", ")
                ),
            ));
        }
    }
    if m.phase_a_audit_passed == Some(false) {
        out.push(MandatoryReason::new(
            "phase_a_audit_failed",
            "Phase A 结构门未通过（stats compare 越界）",
        ));
    }
    // 覆盖率红线（MDR-019 § 决策 2「覆盖率低于源或低于阈值」的绝对阈值部分）。
    // 机械合批组豁免：无运行时行为 → 覆盖率无语义（见方法级 doc）。
    if m.composite_kind != Some(crate::types::state::CompositeKind::Batch) {
        match m.coverage {
            None => out.push(MandatoryReason::new(
                "coverage_below_threshold",
                "coverage 缺失（未测量覆盖率，不能据此认定行为已被验证）",
            )),
            Some(c) if c < coverage_threshold => out.push(MandatoryReason::new(
                "coverage_below_threshold",
                format!("coverage={c} 低于阈值 {coverage_threshold}"),
            )),
            Some(_) => {}
        }
    }
    out
}

/// 评估各内置策略：是否预签启用 + state 层条件是否全过。
///
/// **未启用的策略恒 `eligible=false`**（预签是前提，不是提示）。已启用但 id 未知（拼写错误）
/// 通过返回的 `unknown_enabled_policies` 报出，避免静默失效。
pub fn evaluate_policies(
    m: &ModuleState,
    cfg: &ReviewGateConfig,
    coverage_threshold: u32,
) -> (Vec<PolicyEvaluation>, Vec<String>) {
    let enabled: BTreeSet<&str> = cfg
        .auto_approve_policies
        .iter()
        .map(|s| s.as_str())
        .collect();
    let known: BTreeSet<&str> = POLICY_SPECS.iter().map(|s| s.id).collect();
    let unknown_enabled: Vec<String> = enabled
        .difference(&known)
        .map(|s| (*s).to_owned())
        .collect();

    let evaluations = POLICY_SPECS
        .iter()
        .map(|spec| {
            let is_enabled = enabled.contains(spec.id);
            let mut rejections = Vec::new();
            if !is_enabled {
                rejections.push(format!(
                    "策略未在 [review_gate].auto_approve_policies 预签启用：{}",
                    spec.id
                ));
            }
            rejections.extend(policy_state_rejections(spec.id, m, coverage_threshold));
            PolicyEvaluation {
                id: spec.id.to_owned(),
                enabled: is_enabled,
                eligible: rejections.is_empty(),
                rejections,
                required_attestations: required_attestations(spec),
            }
        })
        .collect();
    (evaluations, unknown_enabled)
}

/// 单个策略的 state 层条件拒因（不含「未启用」，由调用方拼接）。
fn policy_state_rejections(id: &str, m: &ModuleState, _coverage_threshold: u32) -> Vec<String> {
    let mut r = Vec::new();
    match id {
        POLICY_BATCH_MECHANICAL => {
            // 仅全机械合批组——它无运行时行为、编译 + 导出符号即门禁（run.md「Batch 组轻量路径」）。
            if m.composite_kind != Some(crate::types::state::CompositeKind::Batch) {
                r.push(format!(
                    "composite_kind={} 非 batch（仅全机械合批组适用本策略）",
                    m.composite_kind
                        .map(|k| k.to_string())
                        .unwrap_or_else(|| "null".to_owned())
                ));
            }
        }
        POLICY_HEADLESS_DEFAULT => {
            // 无人值守放行要求「有真实 L2 结果且全过」——缺失通过率即视为未验证。
            match m.test_pass_rate.as_deref() {
                None => r.push("test_pass_rate 缺失（无 L2 行为结果，未验证等价性）".to_owned()),
                Some(raw) => match crate::stats::quality::parse_test_pass_rate(raw) {
                    None => r.push(format!("test_pass_rate 无法解析：{raw}")),
                    Some(rate) if rate < 1.0 => {
                        r.push(format!("test_pass_rate={raw} 未达 100%"));
                    }
                    Some(_) => {}
                },
            }
            if m.phase_a_audit_passed != Some(true) {
                r.push("phase_a_audit_passed 非 true（Phase A 结构门未记录通过）".to_owned());
            }
            // coverage 阈值已提为**全局红线**（见 `mandatory_reasons`），此处不重复判
            // ——红线命中时 decision 恒为 MandatoryManual，任何策略都到不了放行。
        }
        _ => r.push(format!("未知策略 id：{id}")),
    }
    r
}

/// 纯判定：强制清单 + 策略评估 → [`GateDecision`]。
pub fn judge(m: &ModuleState, cfg: &ReviewGateConfig, coverage_threshold: u32) -> GateJudgement {
    let mandatory = mandatory_reasons(m, coverage_threshold);
    let (mut policies, _unknown) = evaluate_policies(m, cfg, coverage_threshold);
    // 已停门待人签批的模块：任何策略都放行不了（approve 会拒），故报告里也不得广告
    // `policy_eligible`——否则编排器照 decision 分支去调 `--by-policy` 必撞墙。
    if m.substatus.as_deref() == Some(SUBSTATUS_AWAITING_FINAL_REVIEW) {
        for p in &mut policies {
            p.eligible = false;
            p.rejections.push(
                "模块已停门待人签批（substatus=awaiting_final_review），策略不得放行".to_owned(),
            );
        }
    }
    let decision = if !mandatory.is_empty() {
        // 红线优先：命中即任何策略都不得放行（即使某策略条件恰好全过）。
        GateDecision::MandatoryManual
    } else if policies.iter().any(|p| p.eligible) {
        GateDecision::PolicyEligible
    } else {
        GateDecision::ManualRequired
    };
    GateJudgement {
        decision,
        mandatory_reasons: mandatory,
        policies,
    }
}

/// 校验一个策略放行请求：策略须已启用、判定须非红线、该策略 state 条件须全过、
/// attestation 须覆盖 `required_attestations` 全项。
///
/// 返回 `Err(拒绝原因)`。**分工要如实理解**（静默失败审查 MEDIUM-6）：
/// - CLI **确定性复核**的是：策略已预签、未命中强制人工红线、该策略的 state 层条件全过、
///   `required_attestations` **声明齐全**；
/// - attestation 的**真实性由编排器负责**——CLI 判不了 `no_bug_replica`/`exports_match` 之类
///   （要读产物、跑命令、读 MDR）。故机制不是「CLI 验过了」，而是「逐项声明 + 强制写入审计
///   `attest=[…]`」，让声明可追溯、事后可追责（MDR-019 § 7.4 Approval Token 精神）。
///
/// TODO(port): `content_hash_unchanged` 其实可机验（`{module}-source-hashes.json` 就在磁盘上），
/// 后续可把这一项从「声明」升级为「CLI 复核」。
pub fn check_policy_approval(
    m: &ModuleState,
    cfg: &ReviewGateConfig,
    coverage_threshold: u32,
    policy_id: &str,
    attestations: &[String],
) -> std::result::Result<(), String> {
    if !POLICY_SPECS.iter().any(|s| s.id == policy_id) {
        return Err(format!(
            "unknown_policy: 未知策略 id `{policy_id}`（内置策略：{}）",
            POLICY_SPECS
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let judgement = judge(m, cfg, coverage_threshold);
    let eval = judgement
        .policies
        .iter()
        .find(|p| p.id == policy_id)
        .expect("内置策略必在评估结果中");
    if !eval.enabled {
        return Err(format!(
            "policy_not_enabled: 策略 `{policy_id}` 未在 [review_gate].auto_approve_policies 预签启用"
        ));
    }
    if judgement.decision == GateDecision::MandatoryManual {
        let codes: Vec<&str> = judgement
            .mandatory_reasons
            .iter()
            .map(|r| r.code.as_str())
            .collect();
        return Err(format!(
            "mandatory_manual: 命中强制人工清单（{}），任何策略均不得放行，须人签批",
            codes.join(", ")
        ));
    }
    if !eval.eligible {
        return Err(format!(
            "policy_conditions_unmet: 策略 `{policy_id}` 条件不满足（{}）",
            eval.rejections.join("; ")
        ));
    }
    let given: BTreeSet<&str> = attestations.iter().map(|s| s.as_str()).collect();
    let missing: Vec<&str> = eval
        .required_attestations
        .iter()
        .map(|s| s.as_str())
        .filter(|a| !given.contains(a))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "missing_attestations: 策略 `{policy_id}` 要求编排器逐项声明产物级自查，缺少 --attest {}",
            missing.join(" --attest ")
        ));
    }
    Ok(())
}

/// 扫描 `intermediate/`（含 `attempts/` 子目录）下与本模块相关的**实际存在**文件。
///
/// 匹配规则（两级收紧，防证据串包误导签批——异构 + 测试审查共识）：
/// 1. 文件名须以「模块显示名末段 + `-`」开头（显示名由
///    [`humanize_module_key`](crate::types::state::humanize_module_key) 派生）；多个前缀同时命中时
///    取**最长**前缀（否则模块 `a` 会抢走 `a-b-intent.md`）。
/// 2. 前缀之后的剩余部分须命中 [`classify_evidence`] 的**已知产物白名单**，否则不收
///    ——这样 `a-b-intent.md` 对前缀 `a` 的剩余是 `b-intent.md`（非白名单）而被排除。
///    **新增产物类型须同步白名单**，否则不会出现在证据包里。
///
/// **不构造预期路径**——产物命名约定属 plugin 层（`{module}-intent.md` 等），CLI 猜路径只会
/// 产出一片假的 `exists:false`；这里只报「磁盘上确实有这些相关文件」。composite 组同时用
/// 各成员显示名匹配（组产物可能以任一成员命名）。
///
/// **已知近似**：显示名只取路径末段，故 `src/a/utils.ts` 与 `src/b/utils.ts` 的产物无法区分
/// （产物文件名本身不含目录）。调用方（`state review-gate`）在检出同名末段的其他模块时会告警，
/// 提示人工核对证据归属。
pub fn collect_evidence(
    migration_dir: &Path,
    module_key: &str,
    member_files: &[String],
) -> (Vec<EvidenceItem>, Vec<String>) {
    use crate::types::state::humanize_module_key;

    // 待匹配前缀集合：组代表 + 各成员的显示名末段。
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    for key in std::iter::once(module_key).chain(member_files.iter().map(|s| s.as_str())) {
        let display = humanize_module_key(key);
        let last = display.rsplit('/').next().unwrap_or(&display).to_owned();
        if !last.is_empty() {
            prefixes.insert(last);
        }
    }

    let intermediate = migration_dir.join("intermediate");
    let mut out: Vec<EvidenceItem> = Vec::new();
    // 扫描诊断：目录不存在是正常场景（迁移早期还没产物），但**权限/IO 错误**必须告警——
    // 否则「读不到证据」和「没有证据」在输出上完全一样，而两者的正确处置相反
    // （前者修权限再签，后者质疑迁移是否真做了）。静默失败审查 MEDIUM-3 实测。
    let mut scan_warnings: Vec<String> = Vec::new();
    for sub in ["", "attempts"] {
        let dir = if sub.is_empty() {
            intermediate.clone()
        } else {
            intermediate.join(sub)
        };
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                scan_warnings.push(format!(
                    "证据目录 {} 读取失败，证据包可能不完整（非「无证据」）：{e}",
                    dir.display()
                ));
                continue;
            }
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 最长前缀优先：`a` 与 `a-b` 同时存在时，`a-b-intent.md` 归 `a-b` 而非 `a`。
            let Some(prefix) = prefixes
                .iter()
                .filter(|p| name.starts_with(&format!("{p}-")))
                .max_by_key(|p| p.len())
            else {
                continue;
            };
            let rest = &name[prefix.len() + 1..];
            // 白名单外一律不收：剩余部分不是已知产物名，说明这个文件属于另一个（名字以本模块名
            // 为前缀的）模块，收进来会污染签批证据包。
            let Some(kind) = classify_evidence(rest) else {
                continue;
            };
            let rel = if sub.is_empty() {
                format!("intermediate/{name}")
            } else {
                format!("intermediate/{sub}/{name}")
            };
            out.push(EvidenceItem {
                kind: kind.to_owned(),
                path: rel,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    (out, scan_warnings)
}

/// 按「模块名之后的剩余文件名」归类证据类型；**非已知产物返回 `None`**（调用方据此排除）。
///
/// 白名单而非兜底 `other`：兜底会把「名字以本模块名为前缀的**别的**模块」的产物收进来
/// （`a-b-intent.md` 之于模块 `a`），在人签批时展示错模块的证据。新增产物类型须在此登记。
fn classify_evidence(rest: &str) -> Option<&'static str> {
    Some(match rest {
        "intent.md" => "intent",
        "review.md" => "review",
        "selection.md" => "selection",
        "contract.md" => "contract",
        "degrade-report.json" => "degrade_report",
        "source-hashes.json" => "source_hashes",
        "progress.json" => "progress",
        r if r.starts_with("phase-a") => "phase_a_attempt",
        r if r.starts_with("phase-b") => "phase_b_attempt",
        r if r.ends_with("manifest.json") => "porting_manifest",
        _ => return None,
    })
}

/// 编排器展示证据包时应补跑的命令（产出不落固定文件）。
pub fn evidence_commands(module_key: &str) -> Vec<String> {
    vec![
        format!("rustmigrate state get {module_key}"),
        "rustmigrate stats compare --source <source_root> --rust <rust_root>".to_owned(),
        "rustmigrate stats quality --source <source_root> --rust <rust_root>".to_owned(),
        "git diff -- <rust_root>".to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::DangerCategory;
    use crate::types::state::{CompositeKind, ModuleTier};

    /// 构造一个「测试通过、结构门过、无危险信号、分类可信」的干净待签批模块。
    fn clean_module() -> ModuleState {
        ModuleState {
            status: ModuleStatus::Reviewing,
            substatus: None,
            sprint: Some(1),
            attempts: Vec::new(),
            test_pass_rate: Some("1.0".to_owned()),
            coverage: Some(90),
            known_differences: 0,
            tier: Some(ModuleTier::Standard),
            phase_a_version: Some("h".to_owned()),
            phase_a_audit_passed: Some(true),
            blocked_by: None,
            pre_blocked_status: None,
            member_files: None,
            composite_kind: None,
            decomposition_snapshot: None,
            decomposition_frozen: false,
            danger: Vec::new(),
            danger_provenance: DangerProvenance::Classified,
        }
    }

    fn cfg(policies: &[&str]) -> ReviewGateConfig {
        ReviewGateConfig {
            auto_approve_policies: policies.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn clean_module_without_policy_is_manual_required() {
        // 默认无预签策略 → 干净模块也须人签批（默认全停门）。
        let j = judge(&clean_module(), &ReviewGateConfig::default(), 80);
        assert_eq!(j.decision, GateDecision::ManualRequired);
        assert!(j.mandatory_reasons.is_empty());
        assert!(
            j.policies.iter().all(|p| !p.enabled && !p.eligible),
            "未预签的策略不得 eligible"
        );
    }

    #[test]
    fn danger_non_empty_is_mandatory_manual() {
        let mut m = clean_module();
        m.danger = vec![DangerCategory::Concurrency];
        let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
        assert_eq!(j.decision, GateDecision::MandatoryManual);
        assert!(j
            .mandatory_reasons
            .iter()
            .any(|r| r.code == "danger_non_empty"));
    }

    #[test]
    fn untrusted_provenance_is_mandatory_manual() {
        // 空 danger + 未分类 → 红线（消解空值语义重载：不可据空推断安全）。
        for p in [
            DangerProvenance::Unclassified,
            DangerProvenance::PartiallyClassified,
        ] {
            let mut m = clean_module();
            m.danger_provenance = p;
            let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
            assert_eq!(
                j.decision,
                GateDecision::MandatoryManual,
                "provenance={p} 应红线"
            );
            assert!(j
                .mandatory_reasons
                .iter()
                .any(|r| r.code == "danger_provenance_untrusted"));
        }
    }

    #[test]
    fn known_differences_and_substatus_and_audit_are_mandatory() {
        let cases: Vec<(ModuleState, &str)> = vec![
            (
                {
                    let mut m = clean_module();
                    m.known_differences = 2;
                    m
                },
                "known_differences_present",
            ),
            (
                {
                    let mut m = clean_module();
                    m.substatus = Some("requires_manual_review".to_owned());
                    m
                },
                "substatus_not_clean",
            ),
            (
                {
                    let mut m = clean_module();
                    m.substatus = Some("incomplete".to_owned());
                    m
                },
                "substatus_not_clean",
            ),
            (
                {
                    // 设计的正式值：Phase B 还在进行中的模块过去无红线（子串启发式拦不到）。
                    let mut m = clean_module();
                    m.substatus = Some("phase_b_optimization_in_progress".to_owned());
                    m
                },
                "substatus_not_clean",
            ),
            (
                {
                    // coverage 缺失 → 未验证行为，红线（机械 batch 才豁免）。
                    let mut m = clean_module();
                    m.coverage = None;
                    m
                },
                "coverage_below_threshold",
            ),
            (
                {
                    let mut m = clean_module();
                    m.coverage = Some(70);
                    m
                },
                "coverage_below_threshold",
            ),
            (
                {
                    let mut m = clean_module();
                    m.phase_a_audit_passed = Some(false);
                    m
                },
                "phase_a_audit_failed",
            ),
        ];
        for (m, code) in cases {
            let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
            assert_eq!(j.decision, GateDecision::MandatoryManual, "{code} 应红线");
            assert!(
                j.mandatory_reasons.iter().any(|r| r.code == code),
                "缺 {code}"
            );
        }
    }

    #[test]
    fn headless_policy_eligible_when_enabled_and_conditions_met() {
        let j = judge(&clean_module(), &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
        assert_eq!(j.decision, GateDecision::PolicyEligible);
        let e = j
            .policies
            .iter()
            .find(|p| p.id == POLICY_HEADLESS_DEFAULT)
            .unwrap();
        assert!(e.enabled && e.eligible, "{:?}", e.rejections);
        assert!(e.required_attestations.contains(&"tests_passed".to_owned()));
        // CLI 判不了的强制项也必须逐项声明（异构审查 I4）。
        assert!(
            e.required_attestations
                .contains(&"public_api_unchanged".to_owned()),
            "{:?}",
            e.required_attestations
        );
    }

    #[test]
    fn headless_policy_rejects_missing_test_pass_rate() {
        // 通过率缺失 = 无 L2 行为结果 → 该策略条件不满足（coverage 缺失走的是全局红线）。
        let mut m = clean_module();
        m.test_pass_rate = None;
        let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
        assert_eq!(j.decision, GateDecision::ManualRequired);
        let e = j
            .policies
            .iter()
            .find(|p| p.id == POLICY_HEADLESS_DEFAULT)
            .unwrap();
        assert!(
            e.rejections
                .iter()
                .any(|r| r.contains("test_pass_rate 缺失")),
            "{:?}",
            e.rejections
        );
    }

    /// headless 策略三个谓词的「有值但不合格」分支（测试审查 I2：此前只测了 None）。
    #[test]
    fn headless_policy_rejects_each_unmet_condition() {
        let cases: Vec<(ModuleState, &str)> = vec![
            (
                {
                    let mut m = clean_module();
                    m.test_pass_rate = Some("50%".to_owned());
                    m
                },
                "未达 100%",
            ),
            (
                {
                    let mut m = clean_module();
                    m.test_pass_rate = Some("不是数字".to_owned());
                    m
                },
                "无法解析",
            ),
            (
                {
                    let mut m = clean_module();
                    m.phase_a_audit_passed = None;
                    m
                },
                "phase_a_audit_passed 非 true",
            ),
        ];
        for (m, needle) in cases {
            let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
            assert_eq!(
                j.decision,
                GateDecision::ManualRequired,
                "应拒（{needle}）：{:?}",
                j.mandatory_reasons
            );
            let e = j
                .policies
                .iter()
                .find(|p| p.id == POLICY_HEADLESS_DEFAULT)
                .unwrap();
            assert!(
                e.rejections.iter().any(|r| r.contains(needle)),
                "拒因应含 `{needle}`: {:?}",
                e.rejections
            );
        }
    }

    #[test]
    fn coverage_exactly_at_threshold_is_allowed() {
        // 边界：coverage == 阈值 应放行（`<` 而非 `<=`），防回归成更严/更松。
        let mut m = clean_module();
        m.coverage = Some(80);
        let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
        assert_eq!(j.decision, GateDecision::PolicyEligible, "{j:?}");
    }

    /// 测试审查 I1：`policy_conditions_unmet` 分支此前零覆盖——删掉那 5 行测试仍全绿，
    /// 策略会退化成「预签 + attest 齐 ⇒ 放行」，`batch_mechanical` 的「窄」完全失效。
    #[test]
    fn check_policy_approval_rejects_unmet_state_conditions() {
        // 逻辑单文件模块（composite_kind=None）不符合 batch_mechanical 的形态条件。
        let m = clean_module();
        let c = cfg(&[POLICY_BATCH_MECHANICAL]);
        let all: Vec<String> = required_attestations(
            POLICY_SPECS
                .iter()
                .find(|s| s.id == POLICY_BATCH_MECHANICAL)
                .unwrap(),
        );
        let err = check_policy_approval(&m, &c, 80, POLICY_BATCH_MECHANICAL, &all).unwrap_err();
        assert!(err.starts_with("policy_conditions_unmet:"), "{err}");
        assert!(err.contains("非 batch"), "拒因须点明是哪条条件: {err}");
    }

    #[test]
    fn batch_mechanical_requires_batch_composite_kind() {
        // 单文件模块不适用 batch_mechanical。
        let j = judge(&clean_module(), &cfg(&[POLICY_BATCH_MECHANICAL]), 80);
        assert_eq!(j.decision, GateDecision::ManualRequired);

        let mut m = clean_module();
        m.composite_kind = Some(CompositeKind::Batch);
        // 机械 batch 无行为测试：通过率/覆盖率缺失也不影响本策略。
        m.test_pass_rate = None;
        m.coverage = None;
        let j = judge(&m, &cfg(&[POLICY_BATCH_MECHANICAL]), 80);
        assert_eq!(j.decision, GateDecision::PolicyEligible);

        // coupled_batch 不放行（含逻辑行为，须人签批）。
        let mut m2 = clean_module();
        m2.composite_kind = Some(CompositeKind::CoupledBatch);
        let j2 = judge(&m2, &cfg(&[POLICY_BATCH_MECHANICAL]), 80);
        assert_eq!(j2.decision, GateDecision::ManualRequired);
    }

    #[test]
    fn awaiting_module_does_not_advertise_policy_eligible() {
        // 报告与执行须一致：approve --by-policy 对停门模块必拒，故 judge 不得报 policy_eligible
        // （否则编排器照 decision 分支必撞墙）。
        let mut m = clean_module();
        m.substatus = Some(SUBSTATUS_AWAITING_FINAL_REVIEW.to_owned());
        let j = judge(&m, &cfg(&[POLICY_HEADLESS_DEFAULT]), 80);
        assert_eq!(j.decision, GateDecision::ManualRequired, "{j:?}");
        assert!(j
            .policies
            .iter()
            .all(|p| !p.eligible && p.rejections.iter().any(|r| r.contains("已停门待人签批"))));
    }

    #[test]
    fn evidence_prefix_does_not_leak_across_similar_module_names() {
        // `a` 不得抢走 `a-b` 的产物（最长前缀 + 白名单双重收紧）。
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let inter = root.join("intermediate");
        std::fs::create_dir_all(&inter).unwrap();
        std::fs::write(inter.join("a-intent.md"), "x").unwrap();
        std::fs::write(inter.join("a-b-intent.md"), "x").unwrap();
        // 非白名单剩余（他模块产物的中间段）不得被收成 `other`。
        std::fs::write(inter.join("a-something-else.txt"), "x").unwrap();

        let (ev, _) = collect_evidence(root, "file:src/a.ts", &[]);
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!(ev[0].path, "intermediate/a-intent.md");

        let (ev2, _) = collect_evidence(root, "file:src/a-b.ts", &[]);
        assert_eq!(ev2.len(), 1, "{ev2:?}");
        assert_eq!(ev2[0].path, "intermediate/a-b-intent.md");
    }

    #[test]
    fn unknown_enabled_policy_is_reported() {
        let (_evals, unknown) = evaluate_policies(&clean_module(), &cfg(&["typo_policy"]), 80);
        assert_eq!(unknown, vec!["typo_policy".to_owned()]);
    }

    #[test]
    fn check_policy_approval_full_path() {
        let m = clean_module();
        let c = cfg(&[POLICY_HEADLESS_DEFAULT]);
        // attestation 全齐 → 放行（required = 策略特有 ∪ ORCHESTRATOR_MUST_CHECK）。
        let all: Vec<String> = required_attestations(
            POLICY_SPECS
                .iter()
                .find(|s| s.id == POLICY_HEADLESS_DEFAULT)
                .unwrap(),
        );
        assert!(check_policy_approval(&m, &c, 80, POLICY_HEADLESS_DEFAULT, &all).is_ok());

        // 缺一项 → 拒（去掉最后一项）。
        let short = &all[..all.len() - 1];
        let err = check_policy_approval(&m, &c, 80, POLICY_HEADLESS_DEFAULT, short).unwrap_err();
        assert!(err.starts_with("missing_attestations:"), "{err}");
        assert!(err.contains(all.last().unwrap()), "{err}");

        // 未启用 → 拒。
        let err = check_policy_approval(
            &m,
            &ReviewGateConfig::default(),
            80,
            POLICY_HEADLESS_DEFAULT,
            &all,
        )
        .unwrap_err();
        assert!(err.starts_with("policy_not_enabled:"), "{err}");

        // 未知 id → 拒。
        let err = check_policy_approval(&m, &c, 80, "nope", &all).unwrap_err();
        assert!(err.starts_with("unknown_policy:"), "{err}");

        // 红线命中 → 拒（即使 attestation 齐、策略启用）。
        let mut dangerous = clean_module();
        dangerous.danger = vec![DangerCategory::NumericPrecision];
        let err =
            check_policy_approval(&dangerous, &c, 80, POLICY_HEADLESS_DEFAULT, &all).unwrap_err();
        assert!(err.starts_with("mandatory_manual:"), "{err}");
    }

    #[test]
    fn collect_evidence_reports_existing_files_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let inter = root.join("intermediate");
        std::fs::create_dir_all(inter.join("attempts")).unwrap();
        std::fs::write(inter.join("utils-intent.md"), "x").unwrap();
        std::fs::write(inter.join("utils-review.md"), "x").unwrap();
        std::fs::write(inter.join("utils-degrade-report.json"), "{}").unwrap();
        std::fs::write(inter.join("attempts/utils-phase-a.rs"), "fn a(){}").unwrap();
        // 无关模块的产物不得混入。
        std::fs::write(inter.join("other-intent.md"), "x").unwrap();

        let (ev, scan_warnings) = collect_evidence(root, "file:src/utils.ts", &[]);
        assert!(
            scan_warnings.is_empty(),
            "可读目录不应告警: {scan_warnings:?}"
        );
        let kinds: Vec<&str> = ev.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"intent"), "{ev:?}");
        assert!(kinds.contains(&"review"), "{ev:?}");
        assert!(kinds.contains(&"degrade_report"), "{ev:?}");
        assert!(kinds.contains(&"phase_a_attempt"), "{ev:?}");
        assert_eq!(ev.len(), 4, "不得混入其他模块产物: {ev:?}");
        assert!(ev.iter().all(|e| !e.path.contains("other-")));
    }

    #[test]
    fn collect_evidence_matches_composite_members() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let inter = root.join("intermediate");
        std::fs::create_dir_all(&inter).unwrap();
        // 组产物以成员名命名（组代表是 a.ts，但 progress 以 b 命名）。
        std::fs::write(inter.join("b-progress.json"), "{}").unwrap();

        let (ev, _) = collect_evidence(
            root,
            "file:src/a.ts",
            &["file:src/a.ts".to_owned(), "file:src/b.ts".to_owned()],
        );
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!(ev[0].kind, "progress");
    }

    #[test]
    fn collect_evidence_tolerates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (ev, warns) = collect_evidence(tmp.path(), "file:a.ts", &[]);
        assert!(ev.is_empty());
        assert!(warns.is_empty(), "目录不存在是正常场景，不应告警");
    }
}
