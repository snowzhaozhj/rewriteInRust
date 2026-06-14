//! 迁移进度统计与代码行数统计。

pub mod coverage;
pub mod loc;

pub use coverage::{compute_stats, MigrationStats};
pub use loc::{count_loc, LocLang, LocReport};
