//! `AgentActivityRecord`'s new fields — AC1-AC3 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! The reason these are worth a test of their own: `agent-activity.jsonl` is a durable log that
//! outlives the build that wrote it, so a session started before this change and read after it must
//! still load. A field added without `#[serde(default)]` would turn every historical row into a
//! skipped line with a warning, silently emptying the Agent Activity pane for every existing
//! session.

use pretty_assertions::assert_eq;
use tddy_core::agent_activity::{
    append_agent_activity, read_agent_activity, AgentActivityRecord, STATUS_COMPLETED,
};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_completed_record(call_id: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({ "file_path": "src/lib.rs" }),
        status: STATUS_COMPLETED.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: 1_000,
        completed_unix_ms: 2_000,
        source: "claude-cli".to_string(),
        head_commit: "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c".to_string(),
        activity_seq: 42,
        changed_paths: vec!["src/lib.rs".to_string()],
    }
}

/// A row exactly as a build before this change wrote it: the nine original fields and no more.
const A_ROW_FROM_BEFORE_THIS_CHANGE: &str = r#"{"call_id":"call-legacy","tool_name":"Read","input":null,"status":"completed","result":null,"error_message":"","started_unix_ms":1,"completed_unix_ms":2,"source":"coder"}"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn reads_an_activity_row_written_before_it_carried_a_commit() {
    // Given a log written by an older build
    let session_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        session_dir.path().join("agent-activity.jsonl"),
        format!("{A_ROW_FROM_BEFORE_THIS_CHANGE}\n"),
    )
    .expect("write the legacy log");

    // When it is read by this one
    let records = read_agent_activity(session_dir.path()).expect("must read");

    // Then the row survives, with the absent fields at their empty values rather than the row
    // being skipped as malformed.
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].call_id, "call-legacy");
    assert_eq!(records[0].head_commit, "");
    assert_eq!(records[0].activity_seq, 0);
    assert_eq!(records[0].changed_paths, Vec::<String>::new());
}

#[test]
fn round_trips_the_commit_a_call_ran_upon() {
    // Given a record carrying its base commit
    let session_dir = tempfile::tempdir().expect("tempdir");
    append_agent_activity(session_dir.path(), &a_completed_record("call-1")).expect("must append");

    // When
    let records = read_agent_activity(session_dir.path()).expect("must read");

    // Then — a mirror applies a change against the state it was cut from, so this field surviving
    // the round trip is what makes the log replayable at all.
    assert_eq!(
        records[0].head_commit,
        "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c"
    );
    assert_eq!(records[0].activity_seq, 42);
    assert_eq!(records[0].changed_paths, vec!["src/lib.rs".to_string()]);
}

#[test]
fn keeps_an_empty_commit_rather_than_inventing_one() {
    // Given a record whose HEAD could not be read
    let session_dir = tempfile::tempdir().expect("tempdir");
    let unstamped = AgentActivityRecord {
        head_commit: String::new(),
        ..a_completed_record("call-2")
    };
    append_agent_activity(session_dir.path(), &unstamped).expect("must append");

    // When
    let records = read_agent_activity(session_dir.path()).expect("must read");

    // Then it stays empty. A fabricated sha would make a mirror confidently wrong, which is worse
    // than a mirror that knows it cannot place the change.
    assert_eq!(records[0].head_commit, "");
}
