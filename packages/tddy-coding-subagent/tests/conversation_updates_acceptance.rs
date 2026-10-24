//! A coding-agent conversation's ACP updates reach the recorder while the turn runs — what lets the
//! web show a subagent's tool calls before its turn has ended.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serial_test::serial;
use tddy_agent_backend::{AcpAgentCommand, AcpUpdate, AcpUpdateSink, OpenSession};
use tddy_coding_subagent::CodingAgentSession;
use tddy_discovery::subagent::SubagentSession;

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

#[tokio::test]
#[serial]
async fn every_update_of_a_turn_reaches_the_recorder_in_order() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let scenario = dir.path().join("scenario.json");
    std::fs::write(
        &scenario,
        serde_json::json!({ "responses": [{
            "chunks": ["Reading."],
            "tool_calls": [{ "name": "Read", "input": { "path": "src/lib.rs" } }],
            "permission_requests": [],
        }] })
        .to_string(),
    )
    .expect("write scenario");
    let command = AcpAgentCommand {
        program: stub_agent_path(),
        args: Vec::new(),
        env: vec![(
            "TDDY_ACP_SCENARIO".to_string(),
            scenario.display().to_string(),
        )],
        cwd: None,
    };
    let mut session = CodingAgentSession::start(
        command,
        OpenSession {
            cwd: dir.path().to_path_buf(),
            ..OpenSession::default()
        },
        "stub-model".to_string(),
    )
    .await
    .expect("the coding agent starts");
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink_recorded = Arc::clone(&recorded);
    let sink: AcpUpdateSink = Arc::new(move |update| {
        sink_recorded.lock().expect("recorded").push(update);
    });
    session.record_updates_with(sink);

    // When
    tokio::time::timeout(Duration::from_secs(10), session.prompt("Read lib.rs"))
        .await
        .expect("the turn ended in time")
        .expect("the turn succeeds");

    // Then
    let recorded = recorded.lock().expect("recorded");
    assert_eq!(recorded.len(), 2, "{recorded:?}");
    assert_eq!(recorded[0], AcpUpdate::Text("Reading.".to_string()));
    assert!(
        matches!(&recorded[1], AcpUpdate::ToolCall { title, .. } if title == "Read"),
        "{recorded:?}"
    );
}
