//! Check runs as `GET repos/{o}/{n}/commits/{sha}/check-runs` reports them (`#workqueue` 2/14).

use crate::pr_api::CheckRun;

/// Parse a check-runs response body (`{"total_count": …, "check_runs": [...]}`).
pub fn parse_check_runs(body: &str) -> Result<Vec<CheckRun>, String> {
    // TODO(n2 github-event-inbox): implement — id, name, status, conclusion, started_at,
    // completed_at, details_url for every run.
    let _ = body;
    todo!("TODO(n2): implement parse_check_runs")
}
