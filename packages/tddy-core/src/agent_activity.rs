//! Durable per-session **agent activity** log — the agent's own tool calls.
//!
//! Distinct from `tool-calls.jsonl` (which records only web-triggered `ExecuteTool`
//! invocations): this log captures the tool calls the *agent* makes autonomously during a
//! session (Read, Shell/Bash, Edit, `tddy-tools` verbs, …), with the full input and full
//! output. It is written by whichever host owns a session's tool execution (the daemon for
//! claude-cli / sandbox sessions, the coder participant for tool / cursor-cli sessions) and
//! read back — coalesced — to render the web Agent Activity pane and to seed its live stream.
//!
//! # Format
//! One JSON-serialised [`AgentActivityRecord`] per line in
//! `~/.tddy/sessions/{session_id}/agent-activity.jsonl` (`"\n"` terminator). A tool call
//! appends a `running` row when it starts and a terminal (`completed` / `error`) row when it
//! finishes — append-only keeps each write atomic (POSIX write of ≤ PIPE_BUF bytes). The two
//! rows share a [`AgentActivityRecord::call_id`]; [`read_agent_activity`] **coalesces by
//! `call_id`** (a later row supersedes an earlier one) into the latest state per call,
//! preserving first-seen order, then applies the [`AGENT_ACTIVITY_READ_CAP`] tail cap. A crash
//! mid-call leaves a stuck `running` row (the UI shows it as in-progress).
//!
//! Malformed lines are skipped on read with a `log::warn!`; valid lines after them are still
//! returned.

use std::collections::HashMap;
use std::io;
use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Filename within the session directory.
pub const AGENT_ACTIVITY_FILENAME: &str = "agent-activity.jsonl";

/// Maximum number of *coalesced* calls returned by [`read_agent_activity`]. The newest calls
/// are kept when the log holds more than this.
pub const AGENT_ACTIVITY_READ_CAP: usize = 500;

/// Wire string: a tool call that has started but not yet finished.
pub const STATUS_RUNNING: &str = "running";
/// Wire string: a tool call that finished successfully.
pub const STATUS_COMPLETED: &str = "completed";
/// Wire string: a tool call that finished with an error.
pub const STATUS_ERROR: &str = "error";

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// One recorded state of an agent tool call, persisted as a JSONL row.
///
/// The same `call_id` appears on the `running` row and the terminal row; the read side
/// coalesces them into the call's latest state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentActivityRecord {
    /// Stable id correlating the `running` and terminal rows of one call.
    pub call_id: String,
    /// Tool name, e.g. `"Read"`, `"Bash"`, `"Edit"`.
    pub tool_name: String,
    /// Full tool input as a JSON string.
    pub input_json: String,
    /// One of [`STATUS_RUNNING`], [`STATUS_COMPLETED`], [`STATUS_ERROR`].
    pub status: String,
    /// Full tool output as a JSON string; empty on the `running` row.
    pub result_json: String,
    /// Human-readable error message when `status == "error"`; otherwise empty.
    pub error_message: String,
    /// Unix timestamp (ms since epoch) when the call started.
    pub started_unix_ms: u64,
    /// Unix timestamp (ms since epoch) when the call finished; `0` until terminal.
    pub completed_unix_ms: u64,
    /// Provenance of the record: `"coder"` | `"cursor-cli"` | `"claude-cli"` | `"sandbox"`.
    pub source: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Append one agent-activity row to the session's `agent-activity.jsonl`.
///
/// Creates the session directory and log file if absent. The write is a single append so
/// partial rows are not possible. Callers **must not** treat a failure here as fatal — log it
/// and continue.
pub fn append_agent_activity(session_dir: &Path, record: &AgentActivityRecord) -> io::Result<()> {
    std::fs::create_dir_all(session_dir)?;
    let path = session_dir.join(AGENT_ACTIVITY_FILENAME);
    let mut line = serde_json::to_string(record).map_err(io::Error::other)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())
}

/// Read the session's agent activity, **coalesced by `call_id`** into one record per call
/// (latest row wins, first-seen order preserved), capped to the newest
/// [`AGENT_ACTIVITY_READ_CAP`] calls.
///
/// Returns an empty `Vec` when the file does not exist (no activity recorded yet). Malformed
/// lines are skipped with a warning; valid lines after them are still returned.
pub fn read_agent_activity(session_dir: &Path) -> io::Result<Vec<AgentActivityRecord>> {
    let path = session_dir.join(AGENT_ACTIVITY_FILENAME);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path)?;

    // Coalesce by `call_id`: a later row supersedes the earlier one, while first-seen order of
    // call_ids is preserved.
    let mut order: Vec<String> = Vec::new();
    let mut by_call: HashMap<String, AgentActivityRecord> = HashMap::new();
    for (i, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<AgentActivityRecord>(trimmed) {
            Ok(record) => {
                if !by_call.contains_key(&record.call_id) {
                    order.push(record.call_id.clone());
                }
                by_call.insert(record.call_id.clone(), record);
            }
            Err(e) => {
                log::warn!(
                    "agent_activity: skipping malformed line {} in {}: {}",
                    i + 1,
                    path.display(),
                    e
                );
            }
        }
    }

    // Tail cap: keep only the newest AGENT_ACTIVITY_READ_CAP calls by first-seen order.
    let skip = order.len().saturating_sub(AGENT_ACTIVITY_READ_CAP);
    let records = order
        .into_iter()
        .skip(skip)
        .filter_map(|call_id| by_call.remove(&call_id))
        .collect();
    Ok(records)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod agent_activity_unit_tests {
    use super::*;

    fn a_completed_record(call_id: &str, tool_name: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            input_json: r#"{"path":"src/main.rs"}"#.to_string(),
            status: STATUS_COMPLETED.to_string(),
            result_json: r#"{"content":"fn main() {}"}"#.to_string(),
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 1_700_000_000_500,
            source: "coder".to_string(),
        }
    }

    /// Appending a call and reading it back returns an identical record, with the full input
    /// and full output preserved.
    #[test]
    fn append_then_read_round_trips_a_completed_call() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("s1");
        let record = AgentActivityRecord {
            call_id: "call-1".to_string(),
            tool_name: "Bash".to_string(),
            input_json: r#"{"command":"cargo test --workspace"}"#.to_string(),
            status: STATUS_COMPLETED.to_string(),
            result_json: r#"{"stdout":"test result: ok. 412 passed","exit_code":0}"#.to_string(),
            error_message: String::new(),
            started_unix_ms: 1_700_000_001_000,
            completed_unix_ms: 1_700_000_001_800,
            source: "sandbox".to_string(),
        };

        // When
        append_agent_activity(&session_dir, &record).unwrap();
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert_eq!(records.len(), 1, "must return exactly one call");
        assert_eq!(
            records[0], record,
            "round-tripped record must equal the original"
        );
    }

    /// Reading a session directory with no `agent-activity.jsonl` returns an empty vec without
    /// error — no activity has been recorded yet.
    #[test]
    fn read_missing_file_returns_empty_vec() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("no-activity");
        std::fs::create_dir_all(&session_dir).unwrap();

        // When
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert!(
            records.is_empty(),
            "missing file must return empty vec, not an error"
        );
    }

    /// A malformed line in the middle of the log is skipped; valid calls before and after it
    /// are still returned.
    #[test]
    fn malformed_line_is_skipped_valid_calls_returned() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("partial");
        std::fs::create_dir_all(&session_dir).unwrap();
        let good_a = a_completed_record("call-a", "Read");
        let good_b = a_completed_record("call-b", "Bash");
        let log_path = session_dir.join(AGENT_ACTIVITY_FILENAME);
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&log_path).unwrap();
            writeln!(f, "{}", serde_json::to_string(&good_a).unwrap()).unwrap();
            writeln!(f, "{{not valid json}}").unwrap();
            writeln!(f, "{}", serde_json::to_string(&good_b).unwrap()).unwrap();
        }

        // When
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert_eq!(
            records.len(),
            2,
            "only the two valid calls must be returned"
        );
        assert_eq!(records[0].tool_name, "Read");
        assert_eq!(records[1].tool_name, "Bash");
    }

    /// A `running` row followed by a terminal row for the same `call_id` coalesces into a
    /// single record carrying the call's latest (completed) state.
    #[test]
    fn running_then_completed_rows_coalesce_into_one_completed_call() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("coalesce");
        let running = AgentActivityRecord {
            call_id: "call-1".to_string(),
            tool_name: "Bash".to_string(),
            input_json: r#"{"command":"cargo build"}"#.to_string(),
            status: STATUS_RUNNING.to_string(),
            result_json: String::new(),
            error_message: String::new(),
            started_unix_ms: 1_700_000_002_000,
            completed_unix_ms: 0,
            source: "coder".to_string(),
        };
        let completed = AgentActivityRecord {
            status: STATUS_COMPLETED.to_string(),
            result_json: r#"{"stdout":"Compiling","exit_code":0}"#.to_string(),
            completed_unix_ms: 1_700_000_002_900,
            ..running.clone()
        };

        // When
        append_agent_activity(&session_dir, &running).unwrap();
        append_agent_activity(&session_dir, &completed).unwrap();
        let records = read_agent_activity(&session_dir).unwrap();

        // Then — the two rows collapse to one record in its terminal state
        assert_eq!(
            records.len(),
            1,
            "the two rows for call-1 must coalesce into one call"
        );
        assert_eq!(records[0].status, STATUS_COMPLETED);
        assert_eq!(
            records[0].result_json,
            r#"{"stdout":"Compiling","exit_code":0}"#
        );
        assert_eq!(records[0].completed_unix_ms, 1_700_000_002_900);
    }

    /// When more than `AGENT_ACTIVITY_READ_CAP` calls are recorded, only the newest
    /// `AGENT_ACTIVITY_READ_CAP` are returned (oldest calls dropped).
    #[test]
    fn tail_cap_returns_newest_calls_when_exceeded() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("capped");
        let total = AGENT_ACTIVITY_READ_CAP + 10;
        for i in 0..total {
            let mut record = a_completed_record(&format!("call-{}", i), "Grep");
            record.started_unix_ms = 1_700_000_000_000 + i as u64;
            append_agent_activity(&session_dir, &record).unwrap();
        }

        // When
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert_eq!(
            records.len(),
            AGENT_ACTIVITY_READ_CAP,
            "read must not return more than AGENT_ACTIVITY_READ_CAP calls"
        );
        assert_eq!(
            records.last().unwrap().call_id,
            format!("call-{}", total - 1),
            "the newest call must be present"
        );
        assert_eq!(
            records.first().unwrap().call_id,
            format!("call-{}", total - AGENT_ACTIVITY_READ_CAP),
            "the oldest calls must be dropped when the cap is exceeded"
        );
    }
}
