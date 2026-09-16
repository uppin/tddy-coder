//! Rust code analysis: cyclomatic complexity, CRAP scoring, coverage capture, reports.

pub mod analyze_cli;
pub mod cancellation;
pub mod complexity;
pub mod complexity_cache;
pub mod coverage;
pub mod crap;
pub mod duplicate_tests;
pub mod error;
pub mod report;

pub use cancellation::never_cancelled;
pub use error::{AnalysisError, Result};
