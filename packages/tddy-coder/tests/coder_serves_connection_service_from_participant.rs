//! Acceptance: a spawned tddy-coder session participant serves session-scoped
//! `ConnectionService` methods (`ListExecTools`, `ExecuteTool`, `ClaimTerminalControl`)
//! over its LiveKit identity.
//!
//! `DeleteSession` / `SignalSession` are NOT served here — the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown (changeset
//! `2026-07-12-fast-session-change`). This test therefore does not exercise delete/signal.
//!
//! Run: `cargo test -p tddy-coder --test coder_serves_connection_service_from_participant`
//! With shared kit: `eval $(./run-livekit-testkit-server | grep '^export ')` then same command.

use anyhow::Result;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tddy_coder::session_participant::{
    coder_session_tool_catalog, spawn_session_participant, CoderSessionToolExecutor,
    SessionParticipantOptions, ToolDef, ToolExecutor, ToolOutcome,
};
use tddy_livekit::RpcClient;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::connection::{
    ClaimTerminalControlRequest, ExecuteToolRequest, ListExecToolsRequest,
};

const SESSION_IDENTITY: &str = "daemon-local-coder-session-aaaaaaaa-0000-4000-8000-000000000001";
const CLIENT_IDENTITY: &str = "client";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);
const RPC_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_ID: &str = "aaaaaaaa-0000-4000-8000-000000000001";

/// Test executor: echoes `args_json` back as `result_json` for the `Echo` tool. This is the same
/// shape the production tool engine returns for a successful tool call; it lets the acceptance test
/// verify the session participant's `ExecuteTool` path without depending on the real engine.
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
async fn coder_serves_connection_service_from_participant() -> Result<()> {
    let _ = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .try_init();

    // Given — a LiveKit server
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "coder-session-participant-rpc";

    let tool_calls_dir = tempfile::tempdir()?;
    let (_metadata_tx, metadata_rx) = tokio::sync::watch::channel(String::new());

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

    // When — the coder session participant connects
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
    let rpc_events = client_room.subscribe();
    let rpc_client = RpcClient::new(client_room, SESSION_IDENTITY.to_string(), rpc_events);

    // Then — ListExecTools answers from the session participant
    let list_resp = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc_client.call_unary(
            "connection.ConnectionService",
            "ListExecTools",
            ListExecToolsRequest {
                session_token: "fake-token".to_string(),
                daemon_instance_id: "local".to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ListExecTools timed out"))?
    .map_err(|e| anyhow::anyhow!("ListExecTools RPC: {}", e))?;
    let list_response =
        tddy_service::proto::connection::ListExecToolsResponse::decode(&list_resp[..])?;
    assert!(
        !list_response.tools.is_empty(),
        "session participant must serve ListExecTools with a non-empty tools list"
    );

    // And — ExecuteTool answers from the session participant
    let exec_resp = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc_client.call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            ExecuteToolRequest {
                session_token: "fake-token".to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: "local".to_string(),
                tool_name: "Echo".to_string(),
                args_json: "{}".to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ExecuteTool timed out"))?
    .map_err(|e| anyhow::anyhow!("ExecuteTool RPC: {}", e))?;
    let exec_response =
        tddy_service::proto::connection::ExecuteToolResponse::decode(&exec_resp[..])?;
    assert!(
        !exec_response.is_error,
        "ExecuteTool must succeed on the session participant; error_message='{}'",
        exec_response.error_message
    );

    // And — ClaimTerminalControl answers from the session participant
    let claim_resp = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc_client.call_unary(
            "connection.ConnectionService",
            "ClaimTerminalControl",
            ClaimTerminalControlRequest {
                session_token: "fake-token".to_string(),
                session_id: SESSION_ID.to_string(),
                screen_id: "test-screen".to_string(),
                steal: false,
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ClaimTerminalControl timed out"))?
    .map_err(|e| anyhow::anyhow!("ClaimTerminalControl RPC: {}", e))?;
    let claim_response =
        tddy_service::proto::connection::ClaimTerminalControlResponse::decode(&claim_resp[..])?;
    assert!(
        claim_response.granted,
        "session participant must grant terminal control for its own terminal"
    );

    Ok(())
}

/// Acceptance: when wired with the real [`CoderSessionToolExecutor`], the session participant
/// exposes the full shared tool catalog via `ListExecTools` and dispatches `ExecuteTool("Read")`
/// against the session's worktree root through the shared `tddy-tool-engine`.
#[tokio::test]
#[serial]
async fn coder_session_participant_executes_a_real_read_against_its_worktree() -> Result<()> {
    let _ = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .try_init();

    // Given — a LiveKit server and a tempdir worktree with a pre-written file
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "coder-session-participant-real-tool";

    let worktree = tempfile::tempdir()?;
    let worktree_root = worktree.path().to_path_buf();
    std::fs::write(worktree_root.join("notes.txt"), "hello from the worktree")?;

    let tool_calls_dir = tempfile::tempdir()?;
    let (_metadata_tx, metadata_rx) = tokio::sync::watch::channel(String::new());

    let opts = SessionParticipantOptions {
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: "local".to_string(),
        session_token: "fake-token".to_string(),
        tool_calls_path: tool_calls_dir.path().join("tool-calls.jsonl"),
        tools: coder_session_tool_catalog(),
        executor: Arc::new(CoderSessionToolExecutor {
            worktree_root: worktree_root.clone(),
            task_registry: tddy_task::TaskRegistry::new(),
            session_id: SESSION_ID.to_string(),
        }),
        worktree: worktree_root.clone(),
    };

    let session_identity = "daemon-local-coder-real-aaaaaaaa-0000-4000-8000-000000000003";
    let session_token = livekit.generate_token(room_name, session_identity)?;
    let _session_handle =
        spawn_session_participant(&url, &session_token, session_identity, opts, metadata_rx)
            .await?;

    let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;
    wait_for_participant(&client_room, &mut client_events, session_identity).await?;
    let rpc_events = client_room.subscribe();
    let rpc_client = RpcClient::new(client_room, session_identity.to_string(), rpc_events);

    // When — ListExecTools answers with the full shared catalog
    let list_resp = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc_client.call_unary(
            "connection.ConnectionService",
            "ListExecTools",
            ListExecToolsRequest {
                session_token: "fake-token".to_string(),
                daemon_instance_id: "local".to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ListExecTools timed out"))?
    .map_err(|e| anyhow::anyhow!("ListExecTools RPC: {}", e))?;
    let list_response =
        tddy_service::proto::connection::ListExecToolsResponse::decode(&list_resp[..])?;

    // Then — the catalog lists every shared engine tool, with non-empty schemas
    let names: Vec<String> = list_response.tools.iter().map(|t| t.name.clone()).collect();
    for required in [
        "Read",
        "Write",
        "StrReplace",
        "Delete",
        "Grep",
        "Glob",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
    ] {
        assert!(
            names.contains(&required.to_string()),
            "ListExecTools must list {required}; got: {names:?}"
        );
    }
    for t in &list_response.tools {
        assert!(
            !t.input_schema_json.is_empty(),
            "tool {} must carry a non-empty input_schema_json",
            t.name
        );
    }

    // And — ExecuteTool("Read") returns the worktree file's contents via the real engine
    let exec_resp = tokio::time::timeout(
        RPC_TIMEOUT,
        rpc_client.call_unary(
            "connection.ConnectionService",
            "ExecuteTool",
            ExecuteToolRequest {
                session_token: "fake-token".to_string(),
                session_id: SESSION_ID.to_string(),
                daemon_instance_id: "local".to_string(),
                tool_name: "Read".to_string(),
                args_json: r#"{"path":"notes.txt"}"#.to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ExecuteTool(Read) timed out"))?
    .map_err(|e| anyhow::anyhow!("ExecuteTool(Read) RPC: {}", e))?;
    let exec_response =
        tddy_service::proto::connection::ExecuteToolResponse::decode(&exec_resp[..])?;
    assert!(
        !exec_response.is_error,
        "ExecuteTool(Read) must succeed; error_message='{}'",
        exec_response.error_message
    );
    let parsed: serde_json::Value = serde_json::from_str(&exec_response.result_json)?;
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("hello from the worktree"),
        "ExecuteTool(Read) must return the worktree file contents; got: {}",
        exec_response.result_json
    );

    Ok(())
}
