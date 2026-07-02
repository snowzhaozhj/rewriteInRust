//! 迁移进度统计与代码行数统计。

pub mod community;
pub mod compare;
pub mod coverage;
pub mod loc;
pub mod quality;

pub use community::{detect_community_deviation, CommunityReport};
pub use compare::{compare_structure, CompareReport, Ratio, StructureMetrics};
pub use coverage::{compute_stats, MigrationStats};
pub use loc::{count_loc, LocLang, LocReport};
pub use quality::{compute_quality, QualityReport};
