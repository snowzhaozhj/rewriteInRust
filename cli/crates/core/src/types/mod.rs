//! 公共类型定义（Phase 0 冻结合约）。
//!
//! Phase 1 并行开发的各 Worker 依赖此模块中的类型，
//! 冻结后变更需走合约变更协议（见 PLAN.md §3）。
pub mod common;
pub mod config;
pub mod graph;
pub mod state;

pub use common::*;
pub use config::{BudgetCheckMode, DegradeStrategy, MigrateConfig};
pub use graph::*;
pub use state::*;
