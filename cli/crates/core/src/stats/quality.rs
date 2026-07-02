//! 迁移质量度量框架。
//!
//! 对齐 `docs/design/03-execution-model.md § 7.5` 质量评估分层评分卡。

use crate::types::state::{MigrationStateFile, ModuleStatus};
use serde::{Deserialize, Serialize};

/// 项目级质量报告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityReport {
    pub degrade_rate: f64,
    pub total_modules: usize,
    pub done_modules: usize,
    pub degraded_modules: usize,
    pub avg_final_score: Option<f64>,
    pub revision_rate: Option<f64>,
    pub data_completeness: f64,
    pub modules: Vec<ModuleQuality>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub below_threshold: Vec<String>,
}

/// 质量度量阈值配置。
pub struct QualityThresholds {
    pub done_threshold: f64,
    pub degrade_ffi_threshold: f64,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        Self {
            done_threshold: 80.0,
            degrade_ffi_threshold: 60.0,
        }
    }
}

/// 单模块质量度量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleQuality {
    pub module: String,
    pub status: ModuleStatus,
    pub behavior_coverage: Option<f64>,
    pub known_differences: u32,
    pub deterministic: DeterministicIndicators,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_indicators: Option<AiIndicators>,
    pub final_score: Option<f64>,
}

/// 确定性指标（70% 权重）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeterministicIndicators {
    pub compile_pass: Option<bool>,
    pub test_pass_rate: Option<f64>,
    pub loc_ratio: Option<f64>,
    pub function_ratio: Option<f64>,
    pub clippy_warnings: Option<u32>,
    pub unsafe_blocks: Option<u32>,
    pub cyclomatic_ratio: Option<f64>,
}

/// AI 辅助指标（30% 权重，各项 0-100）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiIndicators {
    pub idiom: f64,
    pub fidelity: f64,
    pub maintainability: f64,
}

pub fn compute_quality(state: &MigrationStateFile) -> QualityReport {
    compute_quality_with_thresholds(state, &QualityThresholds::default())
}

pub fn compute_quality_with_thresholds(
    state: &MigrationStateFile,
    thresholds: &QualityThresholds,
) -> QualityReport {
    let total_modules = state.modules.len();
    let mut done_modules = 0usize;
    let mut degraded_modules = 0usize;

    let mut modules: Vec<ModuleQuality> = state
        .modules
        .iter()
        .map(|(key, ms)| {
            if ms.status == ModuleStatus::Done {
                done_modules += 1;
            }
            if ms.status.is_degraded() {
                degraded_modules += 1;
            }

            let test_pass = ms.test_pass_rate.as_deref().and_then(parse_test_pass_rate);
            let behavior_coverage = compute_behavior_coverage(test_pass, ms.known_differences);
            let compile_pass = infer_compile_pass(ms.status);

            let deterministic = DeterministicIndicators {
                compile_pass,
                test_pass_rate: test_pass,
                loc_ratio: None,
                function_ratio: None,
                clippy_warnings: None,
                unsafe_blocks: None,
                cyclomatic_ratio: None,
            };

            let final_score = compute_final_score(&deterministic, None);

            ModuleQuality {
                module: key.clone(),
                status: ms.status,
                behavior_coverage,
                known_differences: ms.known_differences,
                deterministic,
                ai_indicators: None,
                final_score,
            }
        })
        .collect();

    modules.sort_by(|a, b| a.module.cmp(&b.module));

    let degrade_rate = if total_modules == 0 {
        0.0
    } else {
        degraded_modules as f64 / total_modules as f64
    };

    let scores: Vec<f64> = modules.iter().filter_map(|m| m.final_score).collect();
    let avg_final_score = if scores.is_empty() {
        None
    } else {
        Some(scores.iter().sum::<f64>() / scores.len() as f64)
    };

    let data_completeness = if modules.is_empty() {
        0.0
    } else {
        let with_score = scores.len() as f64;
        with_score / modules.len() as f64
    };

    let below_threshold: Vec<String> = modules
        .iter()
        .filter(|m| {
            if let Some(score) = m.final_score {
                let threshold = if m.status == ModuleStatus::DegradeFfi {
                    thresholds.degrade_ffi_threshold
                } else {
                    thresholds.done_threshold
                };
                score < threshold
            } else {
                false
            }
        })
        .map(|m| m.module.clone())
        .collect();

    QualityReport {
        degrade_rate,
        total_modules,
        done_modules,
        degraded_modules,
        avg_final_score,
        revision_rate: None,
        data_completeness,
        modules,
        below_threshold,
    }
}

/// 从 ModuleStatus 推断编译是否通过。
/// 到达 CompileFixing 及之后的状态意味着至少尝试过编译。
/// Done/Testing/Reviewing 意味着编译曾通过。
fn infer_compile_pass(status: ModuleStatus) -> Option<bool> {
    match status {
        ModuleStatus::Done | ModuleStatus::Testing | ModuleStatus::Reviewing => Some(true),
        ModuleStatus::CompileFixing => Some(false),
        ModuleStatus::DegradeFfi
        | ModuleStatus::DegradeManual
        | ModuleStatus::DegradeSkip
        | ModuleStatus::Paused => None,
        ModuleStatus::Pending | ModuleStatus::Translating | ModuleStatus::Blocked => None,
    }
}

/// 解析 test_pass_rate 字符串为 0.0-1.0 浮点数。
/// 支持格式："85%"、"0.85"、"85"、"85/100"。
fn parse_test_pass_rate(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let result = if let Some(stripped) = s.strip_suffix('%') {
        stripped.trim().parse::<f64>().ok().map(|v| v / 100.0)
    } else if s.contains('/') {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() == 2 {
            let num = parts[0].trim().parse::<f64>().ok()?;
            let den = parts[1].trim().parse::<f64>().ok()?;
            if den == 0.0 {
                return None;
            }
            Some(num / den)
        } else {
            None
        }
    } else {
        let v = s.parse::<f64>().ok()?;
        if v > 1.0 {
            Some(v / 100.0)
        } else {
            Some(v)
        }
    };
    result.filter(|v| v.is_finite())
}

/// 计算行为覆盖率。
/// 基于 test_pass_rate 和 known_differences 调整。
fn compute_behavior_coverage(test_pass: Option<f64>, known_differences: u32) -> Option<f64> {
    let base = test_pass?;
    if known_differences == 0 {
        return Some(base);
    }
    let penalty = known_differences as f64 / (known_differences as f64 + 10.0);
    Some((base * (1.0 - penalty)).max(0.0))
}

/// 计算 AI 指标均值（min*0.34 + median*0.33 + max*0.33）。
fn compute_ai_avg(ai: &AiIndicators) -> f64 {
    let mut vals = [ai.idiom, ai.fidelity, ai.maintainability];
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    vals[0] * 0.34 + vals[1] * 0.33 + vals[2] * 0.33
}

/// 将单项确定性指标归一化到 0-100。
fn normalize_compile(pass: bool) -> f64 {
    if pass {
        100.0
    } else {
        0.0
    }
}

fn normalize_test_pass(rate: f64) -> f64 {
    (rate * 100.0).clamp(0.0, 100.0)
}

fn normalize_ratio(value: f64, healthy_high: f64, alert: f64) -> f64 {
    if value <= healthy_high {
        100.0
    } else if value >= alert {
        0.0
    } else {
        ((alert - value) / (alert - healthy_high) * 100.0).clamp(0.0, 100.0)
    }
}

fn normalize_count(count: u32, penalty_per: f64) -> f64 {
    (100.0 - count as f64 * penalty_per).max(0.0)
}

/// 计算确定性指标均值（仅含有值的项）。
fn compute_deterministic_avg(det: &DeterministicIndicators) -> Option<f64> {
    let mut scores = Vec::new();

    if let Some(pass) = det.compile_pass {
        scores.push(normalize_compile(pass));
    }
    if let Some(rate) = det.test_pass_rate {
        scores.push(normalize_test_pass(rate));
    }
    if let Some(r) = det.loc_ratio {
        scores.push(normalize_ratio(r, 2.0, 3.0));
    }
    if let Some(r) = det.function_ratio {
        scores.push(normalize_ratio(r, 1.3, 2.0));
    }
    if let Some(c) = det.clippy_warnings {
        scores.push(normalize_count(c, 10.0));
    }
    if let Some(u) = det.unsafe_blocks {
        scores.push(normalize_count(u, 15.0));
    }
    if let Some(r) = det.cyclomatic_ratio {
        scores.push(normalize_ratio(r, 1.2, 1.5));
    }

    if scores.len() < 2 {
        return None;
    }
    Some(scores.iter().sum::<f64>() / scores.len() as f64)
}

/// 计算 final_score（§7.5 公式）。
/// 无 AI 指标时确定性权重为 100%。
fn compute_final_score(det: &DeterministicIndicators, ai: Option<&AiIndicators>) -> Option<f64> {
    let det_avg = compute_deterministic_avg(det)?;

    match ai {
        Some(ai_ind) => {
            let ai_avg = compute_ai_avg(ai_ind);
            Some(det_avg * 0.7 + ai_avg * 0.3)
        }
        None => Some(det_avg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::state::{ModuleState, ProjectState};
    use std::collections::HashMap;

    fn module(status: ModuleStatus) -> ModuleState {
        ModuleState {
            status,
            substatus: None,
            sprint: None,
            attempts: vec![],
            test_pass_rate: None,
            coverage: None,
            known_differences: 0,
            tier: None,
            phase_a_version: None,
            phase_a_audit_passed: None,
            blocked_by: None,
            pre_blocked_status: None,
            member_files: None,
            composite_kind: None,
            decomposition_snapshot: None,
            decomposition_frozen: false,
            danger: Vec::new(),
        }
    }

    fn empty_state() -> MigrationStateFile {
        MigrationStateFile {
            version: "1.0.0".to_string(),
            state: ProjectState::SprintLoop,
            state_history: vec![],
            project: None,
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: vec![],
            metadata: None,
        }
    }

    #[test]
    fn test_compute_quality_empty_state() {
        let report = compute_quality(&empty_state());
        assert_eq!(report.total_modules, 0);
        assert_eq!(report.done_modules, 0);
        assert_eq!(report.degraded_modules, 0);
        assert!((report.degrade_rate - 0.0).abs() < f64::EPSILON);
        assert!(report.avg_final_score.is_none());
        assert!(report.modules.is_empty());
        assert!(report.below_threshold.is_empty());
    }

    #[test]
    fn test_compute_quality_mixed_statuses() {
        let mut state = empty_state();
        state.modules.insert("a".into(), module(ModuleStatus::Done));
        state
            .modules
            .insert("b".into(), module(ModuleStatus::DegradeSkip));
        state
            .modules
            .insert("c".into(), module(ModuleStatus::Pending));
        state
            .modules
            .insert("d".into(), module(ModuleStatus::DegradeFfi));

        let report = compute_quality(&state);
        assert_eq!(report.total_modules, 4);
        assert_eq!(report.done_modules, 1);
        assert_eq!(report.degraded_modules, 2);
        assert!((report.degrade_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_test_pass_rate_percent() {
        assert!((parse_test_pass_rate("85%").unwrap() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_parse_test_pass_rate_decimal() {
        assert!((parse_test_pass_rate("0.85").unwrap() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_parse_test_pass_rate_integer() {
        assert!((parse_test_pass_rate("85").unwrap() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_parse_test_pass_rate_fraction() {
        assert!((parse_test_pass_rate("85/100").unwrap() - 0.85).abs() < 1e-9);
    }

    #[test]
    fn test_parse_test_pass_rate_empty() {
        assert!(parse_test_pass_rate("").is_none());
    }

    #[test]
    fn test_parse_test_pass_rate_one() {
        assert!((parse_test_pass_rate("1.0").unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_test_pass_rate_nan_rejected() {
        assert!(parse_test_pass_rate("NaN").is_none());
    }

    #[test]
    fn test_parse_test_pass_rate_infinity_rejected() {
        assert!(parse_test_pass_rate("inf").is_none());
        assert!(parse_test_pass_rate("-inf").is_none());
    }

    #[test]
    fn test_infer_compile_pass_done() {
        assert_eq!(infer_compile_pass(ModuleStatus::Done), Some(true));
    }

    #[test]
    fn test_infer_compile_pass_testing() {
        assert_eq!(infer_compile_pass(ModuleStatus::Testing), Some(true));
    }

    #[test]
    fn test_infer_compile_pass_compile_fixing() {
        assert_eq!(infer_compile_pass(ModuleStatus::CompileFixing), Some(false));
    }

    #[test]
    fn test_infer_compile_pass_pending() {
        assert_eq!(infer_compile_pass(ModuleStatus::Pending), None);
    }

    #[test]
    fn test_infer_compile_pass_translating() {
        assert_eq!(infer_compile_pass(ModuleStatus::Translating), None);
    }

    #[test]
    fn test_compute_ai_avg() {
        let ai = AiIndicators {
            idiom: 90.0,
            fidelity: 80.0,
            maintainability: 70.0,
        };
        let avg = compute_ai_avg(&ai);
        // sorted: [70, 80, 90] → 70*0.34 + 80*0.33 + 90*0.33 = 23.8 + 26.4 + 29.7 = 79.9
        assert!((avg - 79.9).abs() < 1e-9);
    }

    #[test]
    fn test_compute_final_score_with_ai() {
        let det = DeterministicIndicators {
            compile_pass: Some(true),
            test_pass_rate: Some(1.0),
            loc_ratio: None,
            function_ratio: None,
            clippy_warnings: None,
            unsafe_blocks: None,
            cyclomatic_ratio: None,
        };
        let ai = AiIndicators {
            idiom: 90.0,
            fidelity: 80.0,
            maintainability: 70.0,
        };
        let score = compute_final_score(&det, Some(&ai)).unwrap();
        // det_avg = (100 + 100) / 2 = 100
        // ai_avg = 79.9
        // final = 100 * 0.7 + 79.9 * 0.3 = 70 + 23.97 = 93.97
        assert!((score - 93.97).abs() < 0.01);
    }

    #[test]
    fn test_compute_final_score_without_ai() {
        let det = DeterministicIndicators {
            compile_pass: Some(true),
            test_pass_rate: Some(0.95),
            loc_ratio: None,
            function_ratio: None,
            clippy_warnings: None,
            unsafe_blocks: None,
            cyclomatic_ratio: None,
        };
        let score = compute_final_score(&det, None).unwrap();
        // det_avg = (100 + 95) / 2 = 97.5；无 AI 时 100% 权重
        assert!((score - 97.5).abs() < 0.01);
    }

    #[test]
    fn test_compute_final_score_insufficient_data() {
        let det = DeterministicIndicators {
            compile_pass: Some(true),
            test_pass_rate: None,
            loc_ratio: None,
            function_ratio: None,
            clippy_warnings: None,
            unsafe_blocks: None,
            cyclomatic_ratio: None,
        };
        assert!(compute_final_score(&det, None).is_none());
    }

    #[test]
    fn test_below_threshold_detection() {
        let mut state = empty_state();
        // compile=true(100), test=30%(30) → det_avg=65 → below 80
        let mut m = module(ModuleStatus::Done);
        m.test_pass_rate = Some("30%".into());
        state.modules.insert("low_score".into(), m);

        // compile=true(100), test=100%(100) → det_avg=100 → above 80
        let mut m2 = module(ModuleStatus::Done);
        m2.test_pass_rate = Some("100%".into());
        state.modules.insert("high_score".into(), m2);

        let report = compute_quality(&state);
        assert!(
            report.below_threshold.contains(&"low_score".to_string()),
            "det_avg=65 应低于 done 阈值 80"
        );
        assert!(
            !report.below_threshold.contains(&"high_score".to_string()),
            "det_avg=100 应不低于 done 阈值 80"
        );
    }

    #[test]
    fn test_below_threshold_with_failing_compile() {
        let mut state = empty_state();
        let mut m = module(ModuleStatus::CompileFixing);
        m.test_pass_rate = Some("50%".into());
        state.modules.insert("failing".into(), m);

        let report = compute_quality(&state);
        let mq = report
            .modules
            .iter()
            .find(|m| m.module == "failing")
            .unwrap();
        // compile=false(0), test=50(50) → det_avg=25 → below 80
        assert_eq!(mq.final_score, Some(25.0));
        assert!(report.below_threshold.contains(&"failing".to_string()));
    }

    #[test]
    fn test_behavior_coverage_no_differences() {
        assert!((compute_behavior_coverage(Some(0.95), 0).unwrap() - 0.95).abs() < 1e-9);
    }

    #[test]
    fn test_behavior_coverage_with_differences() {
        let cov = compute_behavior_coverage(Some(1.0), 5).unwrap();
        // penalty = 5 / (5+10) = 1/3 ≈ 0.333; cov = 1.0 * (1 - 0.333) ≈ 0.667
        assert!((cov - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_behavior_coverage_no_test_rate() {
        assert!(compute_behavior_coverage(None, 5).is_none());
    }

    #[test]
    fn test_normalize_ratio_in_healthy_range() {
        assert!((normalize_ratio(1.5, 2.0, 3.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_normalize_ratio_at_alert() {
        assert!((normalize_ratio(3.0, 2.0, 3.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_normalize_ratio_between() {
        let v = normalize_ratio(2.5, 2.0, 3.0);
        assert!((v - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_normalize_count() {
        assert!((normalize_count(0, 10.0) - 100.0).abs() < 1e-9);
        assert!((normalize_count(5, 10.0) - 50.0).abs() < 1e-9);
        assert!((normalize_count(15, 10.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_data_completeness() {
        let mut state = empty_state();
        let mut m = module(ModuleStatus::Done);
        m.test_pass_rate = Some("100%".into());
        state.modules.insert("a".into(), m);
        state
            .modules
            .insert("b".into(), module(ModuleStatus::Pending));

        let report = compute_quality(&state);
        // a 有 2 个指标(compile+test) → final_score=Some；b 没有 → None
        assert!((report.data_completeness - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_modules_sorted_by_key() {
        let mut state = empty_state();
        state
            .modules
            .insert("z_mod".into(), module(ModuleStatus::Pending));
        state
            .modules
            .insert("a_mod".into(), module(ModuleStatus::Done));
        state
            .modules
            .insert("m_mod".into(), module(ModuleStatus::Translating));

        let report = compute_quality(&state);
        let keys: Vec<&str> = report.modules.iter().map(|m| m.module.as_str()).collect();
        assert_eq!(keys, vec!["a_mod", "m_mod", "z_mod"]);
    }
}
