//! A call's coalesced record keeps when it started.
//!
//! `ReportAgentActivity` appends two rows per call: the `PreToolUse` row (`running`, its
//! `started_unix_ms` set, `completed_unix_ms` 0) and the `PostToolUse` row (`completed`/`error`,
//! `started_unix_ms` 0, `completed_unix_ms` set). `read_all_agent_activity` coalesces them by
//! `call_id`; on 2026-11-22 every finished tool call `ReadSessionActivity` answered carried
//! `started_unix_ms: 0` — the later row replaced the earlier one whole.
//!
//! Findings: docs/dev/todo/2026-11-22-coder-target-back-channel-and-activity-cursor.md § Defect 2b.

use pretty_assertions::assert_eq;
use tddy_changeset::agent_activity::{
    append_agent_activity, read_agent_activity, read_all_agent_activity, AgentActivityRecord,
    STATUS_COMPLETED, STATUS_RUNNING,
};

const STARTED: u64 = 1_795_326_833_114;
const COMPLETED: u64 = 1_795_326_833_347;

/// The `PreToolUse` row of `call_id`, as the hook report writes it.
fn the_running_row(call_id: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: "Bash".to_string(),
        input: serde_json::json!({ "command": "git branch --show-current" }),
        status: STATUS_RUNNING.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: STARTED,
        completed_unix_ms: 0,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}

/// The `PostToolUse` row of `call_id`, as the hook report writes it: no start time of its own.
fn the_completed_row(call_id: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::json!({ "stdout": "test/orchestrator-coder-hooks-probe" }),
        started_unix_ms: 0,
        completed_unix_ms: COMPLETED,
        ..the_running_row(call_id)
    }
}

fn a_session_with_one_finished_call() -> tempfile::TempDir {
    let session_dir = tempfile::tempdir().unwrap();
    append_agent_activity(session_dir.path(), &the_running_row("toolu_01")).unwrap();
    append_agent_activity(session_dir.path(), &the_completed_row("toolu_01")).unwrap();
    session_dir
}

fn times_of(records: &[AgentActivityRecord]) -> Vec<(String, u64, u64)> {
    records
        .iter()
        .map(|r| (r.status.clone(), r.started_unix_ms, r.completed_unix_ms))
        .collect()
}

#[test]
fn a_finished_call_read_whole_keeps_its_start_time() {
    // Given a call whose running and completed rows were both recorded
    let session_dir = a_session_with_one_finished_call();

    // When every call is read, coalesced
    let records = read_all_agent_activity(session_dir.path()).unwrap();

    // Then the call is completed, and still says when it started
    assert_eq!(
        times_of(&records),
        vec![(STATUS_COMPLETED.to_string(), STARTED, COMPLETED)]
    );
}

#[test]
fn a_finished_call_read_capped_keeps_its_start_time() {
    // Given a call whose running and completed rows were both recorded
    let session_dir = a_session_with_one_finished_call();

    // When the capped tail is read, as the Agent Activity pane reads it
    let records = read_agent_activity(session_dir.path()).unwrap();

    // Then the call still says when it started
    assert_eq!(
        times_of(&records),
        vec![(STATUS_COMPLETED.to_string(), STARTED, COMPLETED)]
    );
}
