//! 项目画像分析（tokei 语言检测 + 复杂度评估）。

pub mod detect;

pub use detect::{detect_language, profile_project, LangStats, ProjectProfile};
