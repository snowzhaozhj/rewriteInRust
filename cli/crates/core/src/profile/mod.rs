//! 项目画像分析（tokei 语言检测 + 复杂度评估）。

pub mod detect;
pub mod tools;

pub use detect::{
    detect_language, detect_source_root, profile_project, LangStats, ProjectProfile,
    SourceRootDetection,
};
pub use tools::{
    check_adapter_tools, check_cargo_nextest, check_tool, load_analysis_tools, AnalysisTool,
    ToolStatus,
};
