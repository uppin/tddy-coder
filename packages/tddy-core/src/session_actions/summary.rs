//! Optional structured summaries (for example cargo-style test totals).

use std::sync::OnceLock;

use log::debug;
use regex::Regex;
use serde_json::Value;

use super::error::SessionActionsError;

/// Parsed quantitative test summary aligned with cargo’s “test result: …” line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestSummary {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
}

fn cargo_test_totals_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)(\d+)\s+passed;\s+(\d+)\s+failed;\s+(\d+)\s+ignored")
            .expect("cargo test totals regex")
    })
}

/// Extract `[passed, failed, skipped]` from combined stdout/stderr (cargo-style `test result:` block).
pub fn parse_test_summary_from_process_output(stdout_stderr: &str) -> Result<TestSummary, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::summary",
        "parse_test_summary_from_process_output: input_len={}",
        stdout_stderr.len()
    );

    let caps = cargo_test_totals_re()
        .captures(stdout_stderr)
        .ok_or(SessionActionsError::TestSummaryParseFailed)?;

    let passed: u64 = caps
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .ok_or(SessionActionsError::TestSummaryParseFailed)?;
    let failed: u64 = caps
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .ok_or(SessionActionsError::TestSummaryParseFailed)?;
    let skipped: u64 = caps
        .get(3)
        .and_then(|m| m.as_str().parse().ok())
        .ok_or(SessionActionsError::TestSummaryParseFailed)?;

    Ok(TestSummary {
        passed,
        failed,
        skipped,
    })
}

/// Merge parsed summary into the JSON invocation record (`summary` field).
pub fn invocation_record_summary_value(summary: &TestSummary) -> Value {
    serde_json::json!({
        "passed": summary.passed,
        "failed": summary.failed,
        "skipped": summary.skipped,
    })
}
