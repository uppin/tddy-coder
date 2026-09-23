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
use std::path::{Component, Path, PathBuf};

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentActivityRecord {
    /// Stable id correlating the `running` and terminal rows of one call.
    pub call_id: String,
    /// Tool name, e.g. `"Read"`, `"Bash"`, `"Edit"`.
    pub tool_name: String,
    /// Full tool input as structured JSON (object, array, string, number, bool, or null).
    pub input: serde_json::Value,
    /// One of [`STATUS_RUNNING`], [`STATUS_COMPLETED`], [`STATUS_ERROR`].
    pub status: String,
    /// Full tool output as structured JSON; [`serde_json::Value::Null`] until terminal.
    pub result: serde_json::Value,
    /// Human-readable error message when `status == "error"`; otherwise empty.
    pub error_message: String,
    /// Unix timestamp (ms since epoch) when the call started.
    pub started_unix_ms: u64,
    /// Unix timestamp (ms since epoch) when the call finished; `0` until terminal.
    pub completed_unix_ms: u64,
    /// Provenance of the record: `"coder"` | `"cursor-cli"` | `"claude-cli"` | `"sandbox"`.
    pub source: String,
    /// The worktree HEAD when this call was recorded, or empty when it could not be read.
    ///
    /// A consumer that reconstructs the worktree from these records applies a change against the
    /// state it was cut from; without the base commit it cannot tell a clean apply from a corrupt
    /// one. Empty is honest — a fabricated sha would make a mirror confidently wrong.
    ///
    /// `#[serde(default)]`: a row written before this field existed still reads.
    #[serde(default)]
    pub head_commit: String,
    /// The session-room poll tick whose delta covers this call; `0` when no tick has covered it yet.
    ///
    /// Measuring per window rather than per call is what catches every writer, including the ones
    /// that never declare their edits. Several calls landing in one window therefore share a seq —
    /// but not a patch: [`Self::changed_paths`] narrows the tick's diff to this call's own files.
    ///
    /// `#[serde(default)]`: a row written before this field existed still reads.
    #[serde(default)]
    pub activity_seq: u64,
    /// The worktree paths this call is credited with, relative to the worktree root.
    ///
    /// This is what makes a delta *this call's* rather than *its window's*: the patch served for a
    /// call is the tick's diff limited to exactly these paths, so two calls landing in one poll
    /// window get two different patches.
    ///
    /// Populated from what the call declared — an `Edit`'s or `Write`'s `file_path`. A call that
    /// declared nothing is credited with nothing, and whatever it changed reaches a consumer
    /// through the tick's *residual* delta instead; attribution never silently drops a change.
    ///
    /// Plain paths, not git's C-quoted display form — these are used to open files.
    ///
    /// `#[serde(default)]`: a row written before this field existed still reads.
    #[serde(default)]
    pub changed_paths: Vec<String>,
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

/// Parse a raw JSON string (a tool's `input_json` / `result_json`) into the structured
/// [`serde_json::Value`] stored on an [`AgentActivityRecord`].
///
/// The contract is:
/// - an **empty string** maps to [`serde_json::Value::Null`] — the field is absent (e.g. a
///   `running` row carries no result, empty input is unset);
/// - a non-empty string that **parses as JSON** yields that structured value;
/// - a non-empty string that **fails to parse** is preserved verbatim as a
///   [`serde_json::Value::String`] scalar, so non-JSON tool text is never lost or fabricated.
pub fn parse_activity_json(raw: &str) -> serde_json::Value {
    if raw.is_empty() {
        return serde_json::Value::Null;
    }
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

/// The worktree paths a tool call declares it will touch, relative to `worktree_root`.
///
/// This is what scopes a call's delta to its own files. It reads only what the call *declared* —
/// an `Edit`'s or `Write`'s `file_path`, a `NotebookEdit`'s `notebook_path` — and never guesses:
/// a `Bash` call may write anything and says so about nothing, so it declares nothing here and
/// whatever it changed reaches a consumer through the tick's *residual* delta instead.
///
/// Read-only tools declare nothing either. Crediting `Read` with the file it read would scope a
/// delta to a path that call did not change, and hand a client a patch belonging to whichever call
/// actually wrote it.
///
/// Paths are returned **relative to the worktree**, because that is what a git pathspec and a diff
/// header speak. A declared path outside the worktree is dropped rather than returned absolute: it
/// cannot appear in that worktree's diff, so keeping it would be a scope that silently matches
/// nothing.
pub fn declared_paths(
    tool_name: &str,
    input: &serde_json::Value,
    worktree_root: &Path,
) -> Vec<String> {
    let Some(field) = declaring_field(tool_name) else {
        return Vec::new();
    };
    // `Value::get` on a non-object (a string, a number, `null`) is `None`, so a malformed input
    // yields nothing rather than panicking. That matters because the `running` row is written
    // before the tool has produced anything, which makes an incomplete input ordinary.
    let Some(declared) = input.get(field).and_then(serde_json::Value::as_str) else {
        return Vec::new();
    };
    if declared.is_empty() {
        return Vec::new();
    }
    worktree_relative(Path::new(declared), worktree_root)
        .into_iter()
        .collect()
}

/// The input field naming the file a writing tool declared, or `None` for every other tool.
///
/// The set is deliberately **closed**. An unknown tool's `file_path` need not be a file it writes —
/// `Read`, `Grep` and `Bash` all carry paths they only look at — so crediting a name this build has
/// never seen would hand that call a patch belonging to whichever call actually wrote the file.
/// Being wrong here is worse than declaring nothing: an undeclared change still reaches a consumer
/// through the tick's residual delta, whereas a misattributed one reaches the wrong call.
fn declaring_field(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "Edit" | "Write" | "MultiEdit" => Some("file_path"),
        "NotebookEdit" => Some("notebook_path"),
        _ => None,
    }
}

/// A declared path expressed relative to `worktree_root`, or `None` when it is not a path inside
/// that worktree.
///
/// Resolution is **lexical**: neither the worktree nor the declared file is required to exist on
/// disk. A `running` row is written before the tool has created anything, and a record may be read
/// back long after the worktree is gone, so a `canonicalize` would fail exactly when the answer is
/// still needed. It also must not follow symlinks — the question is which path a diff will name, not
/// which inode it ends at.
fn worktree_relative(declared: &Path, worktree_root: &Path) -> Option<String> {
    // A relative path is already relative to the worktree, which is the tool's working directory;
    // joining it makes both sides absolute so one `strip_prefix` decides containment for either
    // shape.
    let absolute = if declared.is_absolute() {
        declared.to_path_buf()
    } else {
        worktree_root.join(declared)
    };
    let relative = lexically_normalized(&absolute)
        .strip_prefix(lexically_normalized(worktree_root))
        // Outside the worktree — an edit elsewhere on the host, or one that climbed out via `..`.
        // Dropped rather than returned absolute: it cannot appear in this worktree's diff, so
        // keeping it would be a scope that silently matches nothing.
        .ok()?
        .to_path_buf();
    if relative.as_os_str().is_empty() {
        // The declared path *is* the worktree root. An empty pathspec matches the entire tree, so
        // returning it would scope the call to every file in the worktree.
        return None;
    }
    // The remainder always comes from the declared JSON string, so it is UTF-8; a lossy conversion
    // is refused rather than performed, because a mangled name is a pathspec that matches nothing.
    relative.to_str().map(str::to_string)
}

/// `path` with `.` and `..` resolved by text alone, touching no filesystem.
///
/// `..` is collapsed rather than merely detected, because a path that climbs out and back in
/// (`../worktree/src/lib.rs`) is inside the worktree while one that only climbs (`../../etc/hosts`)
/// is not, and the literal component cannot tell them apart. `/..` resolves to `/`, as POSIX
/// defines it; a leading `..` in a path that is still relative is kept, since nothing here knows
/// what it would climb out of.
fn lexically_normalized(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => normalized.push(Component::ParentDir),
            },
            other => normalized.push(other),
        }
    }
    normalized
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
            input: serde_json::json!({ "path": "src/main.rs" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "content": "fn main() {}" }),
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 1_700_000_000_500,
            source: "coder".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
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
            input: serde_json::json!({ "command": "cargo test --workspace" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "test result: ok. 412 passed", "exit_code": 0 }),
            error_message: String::new(),
            started_unix_ms: 1_700_000_001_000,
            completed_unix_ms: 1_700_000_001_800,
            source: "sandbox".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
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

    /// A tool call whose input and output are structured JSON objects survives the JSONL round-trip
    /// as structured `Value`s (stored un-nested, not re-encoded as a JSON-string-of-JSON).
    #[test]
    fn append_then_read_round_trips_a_structured_object_input_and_result() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("structured");
        let record = AgentActivityRecord {
            call_id: "call-1".to_string(),
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "cargo test --workspace" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "test result: ok. 412 passed", "exit_code": 0 }),
            error_message: String::new(),
            started_unix_ms: 1_700_000_001_000,
            completed_unix_ms: 1_700_000_001_800,
            source: "sandbox".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        };

        // When
        append_agent_activity(&session_dir, &record).unwrap();
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].input,
            serde_json::json!({ "command": "cargo test --workspace" })
        );
        assert_eq!(
            records[0].result,
            serde_json::json!({ "stdout": "test result: ok. 412 passed", "exit_code": 0 })
        );
    }

    /// Tool output is frequently a bare string, not an object; it round-trips as a JSON string
    /// value — the case an object-only `Struct` could not carry.
    #[test]
    fn append_then_read_round_trips_a_bare_string_tool_result() {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let session_dir = tmp.path().join("sessions").join("bare-string");
        let record = AgentActivityRecord {
            call_id: "call-1".to_string(),
            tool_name: "Read".to_string(),
            input: serde_json::json!({ "file_path": "src/main.rs" }),
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::Value::String("fn main() { println!(\"hi\"); }".to_string()),
            error_message: String::new(),
            started_unix_ms: 1_700_000_002_000,
            completed_unix_ms: 1_700_000_002_500,
            source: "coder".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        };

        // When
        append_agent_activity(&session_dir, &record).unwrap();
        let records = read_agent_activity(&session_dir).unwrap();

        // Then
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].result,
            serde_json::Value::String("fn main() { println!(\"hi\"); }".to_string())
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
            input: serde_json::json!({ "command": "cargo build" }),
            status: STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: 1_700_000_002_000,
            completed_unix_ms: 0,
            source: "coder".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        };
        let completed = AgentActivityRecord {
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "Compiling", "exit_code": 0 }),
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
            records[0].result,
            serde_json::json!({ "stdout": "Compiling", "exit_code": 0 })
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
