//! Cargo workspace 骨架生成 + FFI 降级桩 + 降级分析报告。

pub mod degrade_report;
pub mod ffi;
pub mod template;

pub use degrade_report::{
    generate_degrade_report, DegradeEstimate, DegradeOptions, DegradeReport, ErrorSnippet,
    FailureCategory,
};
pub use ffi::{count_exports, generate_ffi_binding, select_cycle_break_point, FfiInterface};
pub use template::{scaffold_project, scaffold_project_with_bin};
