//! Durable per-session tool-call log.
//!
//! Each `ExecuteTool` invocation is appended as a JSON line to
//! `~/.tddy/sessions/{session_id}/tool-calls.jsonl`. The log is independent of
//! the in-memory `TaskRegistry` (which has a 5-min / 200-entry eviction policy)
//! and survives daemon restarts.
//!
//! # Format
//! One JSON-serialised [`ToolCallRecord`] per line (`"\n"` terminator, no trailing
//! comma). Malformed lines are skipped on read with a `log::warn!`; valid lines
//! after them are still returned.

use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Filename within the session directory.
pub const TOOL_CALLS_FILENAME: &str = "tool-calls.jsonl";

/// Maximum number of records returned by [`read_tool_calls`]. Newest entries
/// are kept when the file exceeds this cap.
pub const TOOL_CALLS_READ_CAP: usize = 500;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// One recorded tool-call execution, persisted as a JSONL row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    /// Task ID from the `TaskRegistry` (= `job_id` in `ToolOutcome`). May be
    /// empty for sync tools that don't produce a background job.
    pub task_id: String,
    /// Tool name, e.g. `"Read"`, `"Shell"`.
    pub tool_name: String,
    /// Raw JSON string passed as `args_json` in the `ExecuteToolRequest`.
    pub args_json: String,
    /// Raw JSON string from `ToolOutcome.result_json`.
    pub result_json: String,
    /// `true` when the tool returned an error-level result.
    pub is_error: bool,
    /// Human-readable error message when `is_error` is true.
    pub error_message: String,
    /// `true` when the tool spawned a background job (`job_running` from
    /// `ToolOutcome`). Stdio for background jobs is non-durable; the `task_id`
    /// field can be used to stream live output via `TaskService.WatchTask` while
    /// the task is still in the in-memory registry (~5 min).
    pub job_running: bool,
    /// Unix timestamp (milliseconds since epoch) when this record was appended.
    pub created_unix_ms: u64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Append one tool-call record to the session's `tool-calls.jsonl`.
///
/// Creates the session directory and the log file if they don't exist. The
/// write is a single `file.write_all` call so partial records are not possible
/// (POSIX write of ≤ PIPE_BUF bytes is atomic).
///
/// Callers **must not** treat a failure here as fatal — log it and continue.
pub fn append_tool_call(session_dir: &Path, record: &ToolCallRecord) -> anyhow::Result<()> {
    std::fs::create_dir_all(session_dir)
        .map_err(|e| anyhow::anyhow!("create session dir {}: {}", session_dir.display(), e))?;
    let path = session_dir.join(TOOL_CALLS_FILENAME);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| anyhow::anyhow!("open {}: {}", path.display(), e))?;
    let line = serde_json::to_string(record)
        .map_err(|e| anyhow::anyhow!("serialize ToolCallRecord: {}", e))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| anyhow::anyhow!("write {}: {}", path.display(), e))
}

/// Read all tool-call records for a session from `tool-calls.jsonl`.
///
/// Returns an empty `Vec` if the file does not exist (no calls recorded yet).
/// Malformed lines are skipped with a warning; valid lines after them are
/// still returned. When the file exceeds [`TOOL_CALLS_READ_CAP`] records, only
/// the newest ones are returned.
pub fn read_tool_calls(session_dir: &Path) -> anyhow::Result<Vec<ToolCallRecord>> {
    let path = session_dir.join(TOOL_CALLS_FILENAME);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
    let mut records = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<ToolCallRecord>(trimmed) {
            Ok(r) => records.push(r),
            Err(e) => {
                log::warn!(
                    "tool_call_log: skipping malformed line {} in {}: {}",
                    i + 1,
                    path.display(),
                    e
                );
            }
        }
    }
    // Keep the newest TOOL_CALLS_READ_CAP entries.
    if records.len() > TOOL_CALLS_READ_CAP {
        let skip = records.len() - TOOL_CALLS_READ_CAP;
        records.drain(..skip);
    }
    Ok(records)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tool_call_log_unit_tests {
    use super::*;

    fn a_record(tool_name: &str, args_json: &str) -> ToolCallRecord {
        ToolCallRecord {
            task_id: format!("task-{}", tool_name.to_lowercase()),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
            result_json: r#"{"ok":true}"#.to_string(),
            is_error: false,
            error_message: String::new(),
            job_running: false,
            created_unix_ms: 1_700_000_000_000,
        }
    }

    /// Appending a record and reading it back returns an identical record with
    /// `args_json` intact — proving the input-capture gap is closed.
    #[test]
    fn append_then_read_round_trips_args_json() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("test-session");
        let record = ToolCallRecord {
            task_id: "task-read-1".to_string(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"src/main.rs"}"#.to_string(),
            result_json: r#"{"content":"fn main() {}"}"#.to_string(),
            is_error: false,
            error_message: String::new(),
            job_running: false,
            created_unix_ms: 1_700_000_001_000,
        };

        // When
        append_tool_call(&session_dir, &record).unwrap();
        let records = read_tool_calls(&session_dir).unwrap();

        // Then
        assert_eq!(records.len(), 1, "must return exactly one record");
        assert_eq!(records[0], record, "round-tripped record must equal the original");
        assert_eq!(
            records[0].args_json, r#"{"path":"src/main.rs"}"#,
            "args_json must be preserved exactly"
        );
    }

    /// Reading from a session directory that has no `tool-calls.jsonl` returns an empty
    /// vec without error — the file being absent means no calls have been recorded yet.
    #[test]
    fn read_missing_file_returns_empty_vec() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("no-calls-session");
        std::fs::create_dir_all(&session_dir).unwrap();

        // When
        let records = read_tool_calls(&session_dir).unwrap();

        // Then
        assert!(records.is_empty(), "missing file must return empty vec, not an error");
    }

    /// A malformed line in the middle of the log is skipped; valid lines before and after
    /// it are still returned.
    #[test]
    fn malformed_line_is_skipped_valid_lines_returned() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("partial-session");
        std::fs::create_dir_all(&session_dir).unwrap();
        let good_a = a_record("Read", r#"{"path":"a.rs"}"#);
        let good_b = a_record("Shell", r#"{"command":"ls"}"#);
        let log_path = session_dir.join(TOOL_CALLS_FILENAME);
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&log_path).unwrap();
            writeln!(f, "{}", serde_json::to_string(&good_a).unwrap()).unwrap();
            writeln!(f, "{{not valid json}}").unwrap();
            writeln!(f, "{}", serde_json::to_string(&good_b).unwrap()).unwrap();
        }

        // When
        let records = read_tool_calls(&session_dir).unwrap();

        // Then
        assert_eq!(records.len(), 2, "only the two valid lines must be returned");
        assert_eq!(records[0].tool_name, "Read");
        assert_eq!(records[1].tool_name, "Shell");
    }

    /// When more than `TOOL_CALLS_READ_CAP` records are appended, only the newest
    /// `TOOL_CALLS_READ_CAP` are returned.
    #[test]
    fn tail_cap_returns_newest_records_when_exceeded() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("capped-session");
        let total = TOOL_CALLS_READ_CAP + 10;
        for i in 0..total {
            let record = ToolCallRecord {
                task_id: format!("task-{}", i),
                tool_name: "Grep".to_string(),
                args_json: format!(r#"{{"pattern":"{}"}}"#, i),
                result_json: r#"{"matches":[]}"#.to_string(),
                is_error: false,
                error_message: String::new(),
                job_running: false,
                created_unix_ms: 1_700_000_000_000 + i as u64,
            };
            append_tool_call(&session_dir, &record).unwrap();
        }

        // When
        let records = read_tool_calls(&session_dir).unwrap();

        // Then
        assert_eq!(
            records.len(),
            TOOL_CALLS_READ_CAP,
            "read must not return more than TOOL_CALLS_READ_CAP records"
        );
        // The oldest 10 are dropped; the last record's args_json must contain the
        // highest index.
        let last = records.last().unwrap();
        assert!(
            last.args_json.contains(&(total - 1).to_string()),
            "the newest record must be present; got args_json: {}",
            last.args_json
        );
        // The very first appended record (index 0) must be absent.
        assert!(
            !records[0].args_json.contains("\"0\""),
            "the oldest record must be dropped when cap is exceeded"
        );
    }

    /// Multiple append calls grow the file; order is preserved (oldest first).
    #[test]
    fn multiple_appends_preserve_order() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("ordered-session");
        let records_in = vec![
            a_record("Read", r#"{"path":"a.rs"}"#),
            a_record("Write", r#"{"path":"b.rs","content":"x"}"#),
            a_record("Shell", r#"{"command":"echo hi"}"#),
        ];

        // When
        for r in &records_in {
            append_tool_call(&session_dir, r).unwrap();
        }
        let records_out = read_tool_calls(&session_dir).unwrap();

        // Then
        assert_eq!(records_out.len(), 3);
        for (i, (out, inp)) in records_out.iter().zip(records_in.iter()).enumerate() {
            assert_eq!(
                out.tool_name, inp.tool_name,
                "record {} tool_name must match",
                i
            );
        }
    }
}
