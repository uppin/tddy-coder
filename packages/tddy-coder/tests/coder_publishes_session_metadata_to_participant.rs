//! Acceptance: a spawned tddy-coder session participant publishes its `session` metadata
//! on its LiveKit participant after a workflow state transition, observable by a client
//! in the room without any `ListSessions` call.
//!
//! Changeset: `2026-07-12-fast-session-change`
//! PRD: `docs/ft/coder/session-participant-rpc.md` (req 4)
//!
//! Run: `cargo test -p tddy-coder --test coder_publishes_session_metadata_to_participant`
//! With shared kit: `eval $(./run-livekit-testkit-server | grep '^export ')` then same command.
//!
//! ⚠️ RED PHASE — fails to compile until `tddy_coder::session_participant` exists with
//! `spawn_session_participant`, `SessionParticipantOptions`, `SessionMetadata`, and
//! `session_metadata_json`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use livekit::prelude::*;
use serde_json::Value;
use serial_test::serial;
use tddy_coder::session_participant::{
    session_metadata_json, spawn_session_participant, SessionMetadata, SessionParticipantOptions,
    ToolDef, ToolExecutor, ToolOutcome,
};
use tddy_livekit_testkit::LiveKitTestkit;

const SESSION_IDENTITY: &str = "daemon-local-coder-meta-aaaaaaaa-0000-4000-8000-000000000002";
const CLIENT_IDENTITY: &str = "meta-client";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);
const METADATA_POLL_TIMEOUT: Duration = Duration::from_secs(15);
const SESSION_ID: &str = "aaaaaaaa-0000-4000-8000-000000000002";

struct EchoExecutor;
#[async_trait::async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute(&self, _tool_name: &str, args_json: &str) -> ToolOutcome {
        ToolOutcome {
            result_json: args_json.to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }
}

async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return Ok(());
    }
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                if p.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timed out waiting for participant '{}'", identity))?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn coder_publishes_session_metadata_to_participant() -> Result<()> {
    let _ = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .try_init();

    // Given — a LiveKit server and a session participant with a metadata watch channel
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "coder-session-participant-metadata";

    let tool_calls_dir = tempfile::tempdir()?;
    let (metadata_tx, metadata_rx) = tokio::sync::watch::channel(String::new());
    let opts = SessionParticipantOptions {
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: "local".to_string(),
        session_token: "fake-token".to_string(),
        tool_calls_path: tool_calls_dir.path().join("tool-calls.jsonl"),
        tools: vec![ToolDef {
            name: "Echo".to_string(),
            description: "Echo a message".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        }],
        executor: Arc::new(EchoExecutor),
        worktree: tool_calls_dir.path().to_path_buf(),
    };

    let session_token = livekit.generate_token(room_name, SESSION_IDENTITY)?;
    let _session_handle =
        spawn_session_participant(&url, &session_token, SESSION_IDENTITY, opts, metadata_rx)
            .await?;

    // And — a client connects and sees the session participant
    let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;
    wait_for_participant(&client_room, &mut client_events, SESSION_IDENTITY).await?;

    // When — a workflow state transition publishes the session metadata
    let json = session_metadata_json(&SessionMetadata {
        goal: "acceptance-tests".to_string(),
        state: "Red".to_string(),
        agent: "claude".to_string(),
        model: "sonnet-4".to_string(),
        activity_status: String::new(),
        recipe: "tdd".to_string(),
        repo_path: "/home/dev/feature-meta".to_string(),
        elapsed_display: "3m".to_string(),
        pending_elicitation: false,
    });
    metadata_tx
        .send(json)
        .map_err(|e| anyhow::anyhow!("metadata send: {}", e))?;

    // Then — the client observes the `session` block on the participant metadata
    let target: ParticipantIdentity = SESSION_IDENTITY.to_string().into();
    let deadline = tokio::time::Instant::now() + METADATA_POLL_TIMEOUT;
    let mut last_meta = String::new();
    loop {
        if let Some(remote) = client_room.remote_participants().get(&target) {
            last_meta = remote.metadata();
            if let Ok(v) = serde_json::from_str::<Value>(&last_meta) {
                if let Some(session) = v.get("session") {
                    assert_eq!(
                        session.get("workflow_goal").and_then(|x| x.as_str()),
                        Some("acceptance-tests"),
                        "session metadata must include workflow_goal; got: {}",
                        last_meta
                    );
                    assert_eq!(
                        session.get("workflow_state").and_then(|x| x.as_str()),
                        Some("Red"),
                        "session metadata must include workflow_state; got: {}",
                        last_meta
                    );
                    assert_eq!(
                        session.get("agent").and_then(|x| x.as_str()),
                        Some("claude"),
                        "session metadata must include agent; got: {}",
                        last_meta
                    );
                    assert_eq!(
                        session.get("model").and_then(|x| x.as_str()),
                        Some("sonnet-4"),
                        "session metadata must include model; got: {}",
                        last_meta
                    );
                    return Ok(());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    panic!(
        "timed out waiting for session participant metadata to include the `session` block. Last metadata: {:?}",
        last_meta
    );
}
