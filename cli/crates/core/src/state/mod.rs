//! 状态机管理模块（migration-state.json）。
//!
//! 负责迁移项目状态的加载、保存、转换。

pub mod machine;

pub use machine::{MigrationStateMachine, STATE_SCHEMA_VERSION};
