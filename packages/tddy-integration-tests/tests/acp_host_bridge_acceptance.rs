//! Acceptance tests for the ACP-host bridge (`tddy_coder::acp_host`).
//!
//! The bridge drives an ACP agent subprocess and translates its inbound `session/update`
//! notifications into internal `PresenterEvent`s — the same events the existing
//! `TddyRemoteService` already serves to the web unchanged. Here the agent is `tddy-acp-stub`
//! scripted with a scenario, so each run is deterministic and needs no real agent.
//!
//! Run: cargo test -p tddy-integration-tests --test acp_host_bridge_acceptance -- --test-threads=1

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_client_protocol as acp;
use serial_test::serial;
use tddy_coder::acp_host::{build_acp_agent_command, AcpHostBridge};
use tddy_core::{ActivityKind, PresenterEvent};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn stub_agent_path() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    // packages/tddy-integration-tests -> workspace root
    let workspace_root = PathBuf::from(manifest_dir).join("../..");
    #[cfg(windows)]
    let stub = workspace_root.join("target/debug/tddy-acp-stub.exe");
    #[cfg(not(windows))]
    let stub = workspace_root.join("target/debug/tddy-acp-stub");
    stub
}

/// Writes a stub scenario JSON to a unique temp file and returns its path.
fn write_scenario(name: &str, json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-acp-host-bridge-test");
    std::fs::create_dir_all(&dir).expect("create scenario dir");
    let path = dir.join(format!("{}-{}.json", std::process::id(), name));
    std::fs::write(&path, json).expect("write scenario");
    path
}

/// A stub bridge pointed at `tddy-acp-stub` driven by the given scenario file.
fn bridge_for_scenario(scenario_path: &Path) -> AcpHostBridge {
    let stub = stub_agent_path();
    assert!(
        stub.exists(),
        "tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub"
    );
    AcpHostBridge::with_agent_command(
        stub,
        vec![
            "--scenario".to_string(),
            scenario_path.to_string_lossy().to_string(),
        ],
    )
}

/// Runs one prompt turn and returns (collected presenter events, turn stop reason).
fn run_prompt_collecting(
    bridge: &AcpHostBridge,
    prompt: &str,
) -> (Vec<PresenterEvent>, acp::StopReason) {
    let collected: Arc<Mutex<Vec<PresenterEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_events = collected.clone();
    let cwd = std::env::current_dir().expect("cwd");
    let stop_reason = bridge
        .run_prompt(prompt, &cwd, move |event| {
            sink_events.lock().expect("sink lock").push(event);
        })
        .expect("run_prompt should complete the turn");
    let events = collected.lock().expect("collected lock").clone();
    (events, stop_reason)
}

/// The text of every `AgentOutput` event, in order.
fn agent_outputs(events: &[PresenterEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            PresenterEvent::AgentOutput(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Every logged activity as `(kind, text)`, in order.
fn logged_activities(events: &[PresenterEvent]) -> Vec<(ActivityKind, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            PresenterEvent::ActivityLogged(entry) => Some((entry.kind.clone(), entry.text.clone())),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn forwards_an_agent_message_chunk_from_the_acp_agent_as_an_agent_output_presenter_event() {
    // Given — a stub scripted to emit one agent message chunk for the prompt
    let scenario = write_scenario(
        "one-chunk",
        r#"{"responses":[{"chunks":["hello from acp"],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#,
    );
    let bridge = bridge_for_scenario(&scenario);

    // When — the bridge runs that prompt
    let (events, _stop) = run_prompt_collecting(&bridge, "say hello");

    // Then — the collected presenter events are exactly one AgentOutput carrying the chunk text
    assert_eq!(agent_outputs(&events), vec!["hello from acp".to_string()]);
}

#[test]
#[serial]
fn forwards_a_tool_call_from_the_acp_agent_as_a_tool_use_activity_presenter_event() {
    // Given — a stub scripted to emit one tool call named "Read"
    let scenario = write_scenario(
        "one-tool-call",
        r#"{"responses":[{"chunks":[],"tool_calls":[{"name":"Read","input":{}}],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#,
    );
    let bridge = bridge_for_scenario(&scenario);

    // When — the bridge runs the prompt
    let (events, _stop) = run_prompt_collecting(&bridge, "read a file");

    // Then — the collected presenter events are exactly one ToolUse activity titled "Read"
    assert_eq!(
        logged_activities(&events),
        vec![(ActivityKind::ToolUse, "Read".to_string())]
    );
}

#[test]
#[serial]
fn completes_the_turn_and_reports_the_agents_end_turn_stop_reason() {
    // Given — a stub scripted to end the turn with no output
    let scenario = write_scenario(
        "end-turn",
        r#"{"responses":[{"chunks":[],"tool_calls":[],"permission_requests":[],"stop_reason":"end_turn","error":false}]}"#,
    );
    let bridge = bridge_for_scenario(&scenario);

    // When — the bridge runs the prompt
    let (_events, stop_reason) = run_prompt_collecting(&bridge, "just finish");

    // Then — the turn completes with the agent's EndTurn stop reason
    assert_eq!(stop_reason, acp::StopReason::EndTurn);
}

#[test]
fn builds_an_acp_agent_command_carrying_acp_and_the_selected_agent_and_recipe() {
    // Given — a coder binary, a chosen agent + recipe, and a data dir
    let coder_bin = PathBuf::from("/usr/local/bin/tddy-coder");
    let data_dir = PathBuf::from("/var/lib/tddy");

    // When — the spawn command is built
    let command = build_acp_agent_command(&coder_bin, "claude", "free-prompting", &data_dir);

    // Then — the program is the coder binary and the args carry --acp, the agent, recipe, data dir
    assert_eq!(command.program, coder_bin);
    assert_eq!(
        command.args,
        vec![
            "--acp".to_string(),
            "--agent".to_string(),
            "claude".to_string(),
            "--recipe".to_string(),
            "free-prompting".to_string(),
            "--tddy-data-dir".to_string(),
            "/var/lib/tddy".to_string(),
        ]
    );
}
