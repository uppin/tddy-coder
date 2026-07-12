//! Integration tests for the session-level usage-watcher entry point that the `tddy-coder --daemon`
//! session process calls to feed live token usage onto its presenter broadcast (which the web
//! Inspector consumes over `TddyRemote.Stream`).
//!
//! `spawn_session_usage_watcher` is the wiring seam: given a session's on-disk sources and its
//! agent/model, it derives whether to include the Claude main agent (Claude sessions do; Cursor and
//! other agents don't), then polls those sources and broadcasts each snapshot as a
//! `PresenterEvent::TokenUsageUpdated`. `run_daemon` calls it with the session's presenter
//! `event_tx`; these tests drive it directly against a temp session tree.

use std::path::Path;
use std::time::Duration;

use tddy_core::token_accounting::ConversationRecord;
use tddy_core::usage_watcher::{spawn_session_usage_watcher, SessionUsageWatchConfig};
use tddy_core::PresenterEvent;
use tokio::sync::broadcast;

/// One `type:"assistant"` transcript line: in=120, out=30, model claude-opus-4-8.
const CLAUDE_TRANSCRIPT_LINE: &str = r#"{"type":"assistant","message":{"model":"claude-opus-4-8","usage":{"input_tokens":120,"output_tokens":30}}}"#;

/// A tddy-subagent accounting file with one `fastcontext` conversation (camelCase token fields).
const ACCOUNTING_JSON: &str = r#"{"conversations":[{"agent":"fastcontext","id":"fc-1","model":"ollama","inputTokens":5,"outputTokens":1,"totalTokens":6,"turns":1}]}"#;

fn write_claude_transcript(claude_home: &Path, session_id: &str) {
    let dir = claude_home.join(".claude").join("projects").join("proj");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{session_id}.jsonl")),
        format!("{CLAUDE_TRANSCRIPT_LINE}\n"),
    )
    .unwrap();
}

fn write_accounting(session_dir: &Path) {
    let egress = session_dir.join("egress");
    std::fs::create_dir_all(&egress).unwrap();
    std::fs::write(egress.join("accounting.json"), ACCOUNTING_JSON).unwrap();
}

/// Await the first broadcast usage snapshot, failing fast if none arrives.
async fn first_snapshot(rx: &mut broadcast::Receiver<PresenterEvent>) -> Vec<ConversationRecord> {
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("the watcher must broadcast a usage snapshot within 2s")
        .expect("the broadcast channel must deliver the event");
    match event {
        PresenterEvent::TokenUsageUpdated(records) => records,
        other => panic!("expected a TokenUsageUpdated event, got {other:?}"),
    }
}

#[tokio::test]
async fn broadcasts_the_current_claude_usage_snapshot_for_a_claude_session() {
    // Given a Claude session whose transcript is already on disk
    let tmp = tempfile::tempdir().unwrap();
    let claude_home = tmp.path().join("home");
    let session_dir = tmp.path().join("session");
    write_claude_transcript(&claude_home, "sess-1");

    let (event_tx, mut rx) = broadcast::channel(16);

    // When a session usage watcher is started for it
    let handle = spawn_session_usage_watcher(
        SessionUsageWatchConfig {
            session_dir,
            session_id: "sess-1".to_string(),
            claude_home,
            agent: "claude".to_string(),
            model: "claude-opus-4-8".to_string(),
            poll_interval: Duration::from_millis(20),
        },
        event_tx,
    );

    // Then it broadcasts the main-agent row summed from the transcript
    let snapshot = first_snapshot(&mut rx).await;
    assert_eq!(
        snapshot,
        vec![ConversationRecord {
            agent: "claude".to_string(),
            id: "sess-1".to_string(),
            model: "claude-opus-4-8".to_string(),
            input_tokens: 120,
            output_tokens: 30,
            total_tokens: 150,
            turns: 1,
        }]
    );

    handle.abort();
}

#[tokio::test]
async fn omits_the_claude_main_agent_for_a_cursor_session() {
    // Given a Cursor session that has a stray Claude transcript on disk but reports its usage only
    // through the tddy accounting file
    let tmp = tempfile::tempdir().unwrap();
    let claude_home = tmp.path().join("home");
    let session_dir = tmp.path().join("session");
    write_claude_transcript(&claude_home, "sess-2");
    write_accounting(&session_dir);

    let (event_tx, mut rx) = broadcast::channel(16);

    // When a session usage watcher is started for the Cursor agent
    let handle = spawn_session_usage_watcher(
        SessionUsageWatchConfig {
            session_dir,
            session_id: "sess-2".to_string(),
            claude_home,
            agent: "cursor".to_string(),
            model: "cursor-model".to_string(),
            poll_interval: Duration::from_millis(20),
        },
        event_tx,
    );

    // Then the snapshot carries only the accounting conversation — no Claude main-agent row
    let snapshot = first_snapshot(&mut rx).await;
    assert_eq!(
        snapshot,
        vec![ConversationRecord {
            agent: "fastcontext".to_string(),
            id: "fc-1".to_string(),
            model: "ollama".to_string(),
            input_tokens: 5,
            output_tokens: 1,
            total_tokens: 6,
            turns: 1,
        }]
    );

    handle.abort();
}
