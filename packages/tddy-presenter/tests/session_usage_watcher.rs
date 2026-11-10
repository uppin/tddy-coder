//! Integration tests for the session-level usage-watcher entry point that the `tddy-coder --daemon`
//! session process calls to feed live token usage onto its presenter broadcast (which the web
//! Inspector consumes over `TddyRemote.Stream`).
//!
//! `spawn_session_usage_watcher` is the wiring seam: given the session agent's usage binding (from
//! its spec, as data) and the session it resolves against, it polls and broadcasts each snapshot as
//! a `PresenterEvent::TokenUsageUpdated`. Which agent the session runs is never consulted
//! (`#usage-binding 1/5`). `run_daemon` calls it with the session's presenter `event_tx`; these
//! tests drive it directly against a temp session tree.

use std::path::Path;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_agent_backend::token_accounting::ConversationRecord;
use tddy_presenter::usage_watcher::{spawn_session_usage_watcher, SessionUsageWatchConfig};
use tddy_presenter::PresenterEvent;
use tddy_usage::{UsageContext, UsageSection};
use tokio::sync::broadcast;

/// A binding that reads `<agent_home>/<conversation_id>.jsonl`: every `type: turn` line's `in` and
/// `out`, with the model from `model`.
fn a_binding() -> UsageSection {
    serde_json::from_value(serde_json::json!({
        "sources": [{
            "format": "jsonl",
            "main": { "path": "{agent_home}/{conversation_id}.jsonl" },
            "record": {
                "where": [{ "field": "type", "equals": "turn" }],
                "model": "model",
                "tokens": { "input": "in", "output": "out" }
            }
        }]
    }))
    .expect("a valid binding")
}

fn write_turn(agent_home: &Path, conversation_id: &str) {
    std::fs::create_dir_all(agent_home).unwrap();
    std::fs::write(
        agent_home.join(format!("{conversation_id}.jsonl")),
        "{\"type\":\"turn\",\"model\":\"m-1\",\"in\":120,\"out\":30}\n",
    )
    .unwrap();
}

/// A tddy-subagent accounting file with one `fastcontext` conversation (camelCase token fields).
fn write_accounting(session_dir: &Path) {
    let egress = session_dir.join("egress");
    std::fs::create_dir_all(&egress).unwrap();
    std::fs::write(
        egress.join("accounting.json"),
        r#"{"conversations":[{"agent":"fastcontext","id":"fc-1","model":"ollama","inputTokens":5,"outputTokens":1,"totalTokens":6,"turns":1}]}"#,
    )
    .unwrap();
}

/// The session `sess-1`, its agent's files under `root/home`, labelled `agent-x`.
fn a_session(root: &Path) -> UsageContext {
    UsageContext {
        agent_home: Some(root.join("home")),
        session_dir: root.join("session"),
        conversation_id: Some("sess-1".to_string()),
        worktree: None,
        agent_label: "agent-x".to_string(),
        fallback_model: "fallback".to_string(),
    }
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
async fn broadcasts_the_snapshot_the_sessions_usage_binding_yields() {
    // Given a session whose agent's file is already on disk
    let tmp = tempfile::tempdir().unwrap();
    let context = a_session(tmp.path());
    write_turn(context.agent_home.as_ref().unwrap(), "sess-1");
    let (event_tx, mut rx) = broadcast::channel(16);

    // When a session usage watcher is started for it with its agent's binding
    let handle = spawn_session_usage_watcher(
        SessionUsageWatchConfig {
            usage: Some(a_binding()),
            context,
            poll_interval: Duration::from_millis(20),
        },
        event_tx,
    );
    let records = first_snapshot(&mut rx).await;
    handle.abort();

    // Then the snapshot is the main conversation the binding read
    let rows: Vec<(&str, &str, &str, u64, u64, u32)> = records
        .iter()
        .map(|r| {
            (
                r.agent.as_str(),
                r.id.as_str(),
                r.model.as_str(),
                r.input_tokens,
                r.output_tokens,
                r.turns,
            )
        })
        .collect();
    assert_eq!(rows, vec![("agent-x", "sess-1", "m-1", 120, 30, 1)]);
}

#[tokio::test]
async fn a_session_without_a_usage_binding_broadcasts_only_its_tddy_subagents() {
    // Given a session whose agent declares no binding, with a stray file on disk and a tddy
    // subagent in its accounting file
    let tmp = tempfile::tempdir().unwrap();
    let context = a_session(tmp.path());
    write_turn(context.agent_home.as_ref().unwrap(), "sess-1");
    write_accounting(&context.session_dir);
    let (event_tx, mut rx) = broadcast::channel(16);

    // When
    let handle = spawn_session_usage_watcher(
        SessionUsageWatchConfig {
            usage: None,
            context,
            poll_interval: Duration::from_millis(20),
        },
        event_tx,
    );
    let records = first_snapshot(&mut rx).await;
    handle.abort();

    // Then
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["fc-1"]);
}
