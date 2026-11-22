//! Check runs as `GET repos/{o}/{n}/commits/{sha}/check-runs` reports them (`#workqueue` 2/14).

use serde_json::Value;

use crate::pr_api::{CheckRun, CheckStatus};

/// Parse a check-runs response body (`{"total_count": …, "check_runs": [...]}`).
pub fn parse_check_runs(body: &str) -> Result<Vec<CheckRun>, String> {
    let response: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    let runs = response
        .get("check_runs")
        .and_then(Value::as_array)
        .ok_or_else(|| "the check-runs answer has no check_runs array".to_string())?;
    Ok(runs.iter().map(parse_check_run).collect())
}

fn parse_check_run(run: &Value) -> CheckRun {
    let text = |field: &str| run.get(field).and_then(Value::as_str).map(str::to_string);
    CheckRun {
        id: run.get("id").and_then(Value::as_u64).unwrap_or(0),
        name: text("name").unwrap_or_default(),
        // Any status short of `completed` (`queued`, `in_progress`, `waiting`, `pending`, …) is
        // unfinished; only `in_progress` is told apart from queued.
        status: match text("status").as_deref() {
            Some("completed") => CheckStatus::Completed,
            Some("in_progress") => CheckStatus::InProgress,
            _ => CheckStatus::Queued,
        },
        // `null` while the run is going: empty, never a failure.
        conclusion: text("conclusion").unwrap_or_default(),
        started_at: text("started_at"),
        completed_at: text("completed_at"),
        details_url: text("details_url").unwrap_or_default(),
    }
}
