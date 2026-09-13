//! Rust code analysis: cyclomatic complexity, CRAP scoring, coverage capture, reports.

pub mod analyze_cli;
pub mod complexity;
pub mod coverage;
pub mod crap;
pub mod duplicate_tests;
pub mod error;
pub mod report;

pub use error::{AnalysisError, Result};
