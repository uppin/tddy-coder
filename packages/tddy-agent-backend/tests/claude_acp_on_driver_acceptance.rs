//! `claude-acp` on the shared ACP driver: the requested model takes effect, and a session id from
//! one backend resumes on another by `session/load`.
//!
//! The backend spawns its agent with the inherited environment, so the stub's scenario and record
//! paths are set process-wide — hence `#[serial]`.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-acp-driver.md

use std::path::{Path, PathBuf};

use serial_test::serial;
use tddy_agent_backend::{ClaudeAcpBackend, CodingBackend, InvokeRequest, SessionMode};

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let stub = PathBuf::from(manifest_dir).join("../../target/debug/tddy-acp-stub");
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    stub
}

/// Points the next spawned stub at `scenario` and a fresh record file in `dir`; returns that file.
fn a_stub_recording_into(dir: &Path, scenario: serde_json::Value) -> PathBuf {
    let scenario_path = dir.join("scenario.json");
    std::fs::write(&scenario_path, scenario.to_string()).expect("write scenario");
    let record = dir.join("record.jsonl");
    std::env::set_var("TDDY_ACP_SCENARIO", &scenario_path);
    std::env::set_var("TDDY_ACP_STUB_RECORD", &record);
    record
}

fn recorded(record: &Path, event: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(record)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("record line is JSON"))
        .filter(|line| line["event"] == event)
        .collect()
}

fn a_request(prompt: &str, dir: &Path) -> InvokeRequest {
    InvokeRequest {
        prompt: prompt.to_string(),
        working_dir: Some(dir.to_path_buf()),
        ..InvokeRequest::default()
    }
}

fn one_empty_turn() -> serde_json::Value {
    serde_json::json!({ "responses": [{ "chunks": ["ok"] }] })
}

#[tokio::test]
#[serial]
async fn claude_acp_honours_the_requested_model() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let mut scenario = one_empty_turn();
    scenario["models"] = serde_json::json!(["sonnet", "opus"]);
    let record = a_stub_recording_into(dir.path(), scenario);
    let backend = ClaudeAcpBackend::with_agent_path(stub_agent_path());

    // When
    backend
        .invoke(InvokeRequest {
            model: Some("opus".to_string()),
            ..a_request("Plan it", dir.path())
        })
        .await
        .expect("invoke succeeds");

    // Then
    let set_model = recorded(&record, "set_session_model");
    assert_eq!(set_model.len(), 1, "the model was set: {set_model:?}");
    assert_eq!(set_model[0]["model_id"], "opus");
}

#[tokio::test]
#[serial]
async fn claude_acp_resume_survives_a_new_backend() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    a_stub_recording_into(dir.path(), one_empty_turn());
    let first = ClaudeAcpBackend::with_agent_path(stub_agent_path());
    let session_id = first
        .invoke(InvokeRequest {
            session: Some(SessionMode::Fresh("tddy-1".to_string())),
            ..a_request("First turn", dir.path())
        })
        .await
        .expect("first invoke")
        .session_id
        .expect("the turn reports its session id");
    drop(first);
    let second_dir = tempfile::tempdir().expect("tempdir");
    let record = a_stub_recording_into(second_dir.path(), one_empty_turn());
    let second = ClaudeAcpBackend::with_agent_path(stub_agent_path());

    // When
    second
        .invoke(InvokeRequest {
            session: Some(SessionMode::Resume(session_id.clone())),
            ..a_request("Second turn", second_dir.path())
        })
        .await
        .expect("resumed invoke");

    // Then
    let loaded = recorded(&record, "load_session");
    assert_eq!(loaded.len(), 1, "resumed by session/load: {loaded:?}");
    assert_eq!(loaded[0]["session_id"], session_id.as_str());
    assert!(
        recorded(&record, "new_session").is_empty(),
        "no fresh session"
    );
}
