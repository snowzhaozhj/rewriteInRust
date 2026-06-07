//! 状态机管理模块（migration-state.json）。
//!
//! 负责迁移项目状态的加载、保存、转换。

pub mod machine;

pub use machine::MigrationStateMachine;

/// 获取当前状态（CLI 占位接口，后续由 CLI 层集成参数）。
pub fn get() {
    todo!("M1: state get — 请通过 MigrationStateMachine::load 使用")
}

/// 执行状态转换（CLI 占位接口，后续由 CLI 层集成参数）。
pub fn transition() {
    todo!("M1: state transition — 请通过 MigrationStateMachine::transition 使用")
}
