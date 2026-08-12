//! 状态机管理模块（migration-state.json）。
//!
//! 负责迁移项目状态的加载、保存、转换。

pub mod machine;
pub mod review_gate;

pub use machine::{
    Approval, ClearedGhostRef, GhostRepairOutcome, InterruptedModule, MigrationStateMachine,
    RecordMetricsOutcome, RecoverOutcome, RecoverPolicy, RepairedModule, ResetOutcome, ResumePlan,
    ResumeProgress, SprintAdvanceResult, STATE_SCHEMA_VERSION, SUBSTATUS_AGENT_DONE,
};
pub use review_gate::{
    GateDecision, GateJudgement, MandatoryReason, PolicyEvaluation, ReviewGateReport,
    POLICY_BATCH_MECHANICAL, POLICY_HEADLESS_DEFAULT, SUBSTATUS_AWAITING_FINAL_REVIEW,
};
