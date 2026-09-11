//! Session-participant module — the tddy-coder process serves session-scoped RPCs (tools,
//! terminal control) from its own LiveKit participant and publishes `session` metadata.
//!
//! Three coordinates, registered together by [`session_service_entries`]:
//!
//! * `connection.ConnectionService` — the session's tools and its tool calls, served here.
//! * `terminal_session.TerminalSessionService` — the terminal family, served by
//!   [`terminal_session_service`] from `tddy-terminal-rpc`'s own handlers, so this process and the
//!   daemon answer one session identically. `#unbundle` node 6 moved the terminal streaming off
//!   `connection.ConnectionService` here for that reason.
//! * `activity.ActivityService` — the session's activity stream and its ACP replay, served by
//!   [`activity_service`]. `#unbundle` node 7 moved families M and N off
//!   `connection.ConnectionService`, and this participant moved with them in the same PR: the two
//!   servers of one session have to answer it the same way.
//!
//! `DeleteSession` / `SignalSession` are **not** served here: the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).

pub mod acp_transcript;
pub mod activity_service;
pub mod connection_service_participant;
pub mod metadata_publisher;
pub mod terminal_manager;
pub mod terminal_session_adapter;
pub mod terminal_session_service;

pub use acp_transcript::{append_frames_for_event, frame_for_event, spawn_acp_transcript_writer};
pub use activity_service::coder_activity_entry;
pub use connection_service_participant::{
    coder_session_tool_catalog, coder_session_tool_catalog_full, CoderSessionToolExecutor,
    SessionConnectionService, ToolDef, ToolExecutor, ToolOutcome,
};
pub use metadata_publisher::{
    session_metadata_json, spawn_session_metadata_tap, SessionMetadata, SessionMetadataSeed,
};
pub use terminal_session_service::coder_terminal_session_entry;

use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;
use tokio::sync::watch;

use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::connection::{
    ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse,
    ListSessionToolCallsRequest, ListSessionToolCallsResponse, ToolCallInfo,
    ToolDef as ProtoToolDef,
};

/// Buffer size for the `StreamSessionActivity` server-stream bridge (snapshot rows + live tail).
pub(crate) const AGENT_ACTIVITY_CHANNEL_CAPACITY: usize = 256;

/// Options for spawning a session participant. `tools` + `executor` are injected by `run.rs`
/// (production wires the shared tool engine; tests wire a fake).
#[derive(Clone)]
pub struct SessionParticipantOptions {
    pub session_id: String,
    pub daemon_instance_id: String,
    pub session_token: String,
    pub tool_calls_path: std::path::PathBuf,
    pub tools: Vec<ToolDef>,
    pub executor: Arc<dyn ToolExecutor>,
    /// Session worktree where started bash terminals are spawned (the coder's agent working dir).
    pub worktree: std::path::PathBuf,
}

/// Handle returned by `spawn_session_participant`. Dropping it does **not** cancel the participant —
/// the connection + metadata watcher run in spawned tasks. The handle keeps the `JoinHandle` for a
/// future graceful-shutdown wiring.
pub struct SessionParticipantHandle {
    _run: tokio::task::JoinHandle<()>,
}

/// Spawn the session's LiveKit participant, serving `connection.ConnectionService`
/// (session-scoped tools + terminal control) and publishing `session` metadata from `metadata_rx`.
///
/// The participant identity is `session-{daemon_instance_id}-{session_id}` (built by the caller and
/// passed as `identity`). The token must authorize that identity for the target room.
pub async fn spawn_session_participant(
    ws_url: &str,
    session_token: &str,
    identity: &str,
    opts: SessionParticipantOptions,
    metadata_rx: watch::Receiver<String>,
) -> anyhow::Result<SessionParticipantHandle> {
    // The agent-activity log lives alongside `tool-calls.jsonl` in the session directory.
    let agent_activity_dir = opts
        .tool_calls_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let svc = Arc::new(SessionConnectionService {
        session_id: opts.session_id.clone(),
        session_token: opts.session_token.clone(),
        tool_calls_path: opts.tool_calls_path.clone(),
        tools: opts.tools.clone(),
        executor: opts.executor.clone(),
        worktree: opts.worktree.clone(),
        terminal_manager: Arc::new(terminal_manager::TerminalManager::new()),
        agent_activity_dir,
        // This spawn path has no presenter broadcast wired; served as snapshot-only. `run.rs`
        // (the production coder participant) wires the live presenter channel directly.
        presenter_events: None,
    });
    let mut entries = session_service_entries_from(svc);
    let names: Vec<&str> = entries.iter().map(|e| e.name).collect();
    entries.push(tddy_service::reflection_entry_from(&names));
    let multi = tddy_rpc::MultiRpcService::new(entries);

    let participant = tddy_livekit::LiveKitParticipant::connect(
        ws_url,
        session_token,
        multi,
        tddy_livekit::RoomOptions::default(),
        None,
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("session participant connect (identity={identity}): {e}"))?;

    let local = participant.room().local_participant().clone();
    let lock = participant.metadata_publish_lock();
    let _meta_handle =
        tddy_livekit::spawn_local_participant_metadata_watcher(metadata_rx, local, lock);

    log::info!(
        target: "tddy_coder::session_participant",
        "session participant '{}' connected for session {}",
        identity,
        opts.session_id
    );

    let run = tokio::spawn(async move {
        participant.run().await;
    });
    Ok(SessionParticipantHandle { _run: run })
}

/// `RpcService` adapter that dispatches the session-scoped `ConnectionService` methods to a
/// [`SessionConnectionService`]. Methods not served by the session participant (delete/signal,
/// project listing, session start/resume, terminal streaming, …) return `Unimplemented` — the web
/// routes them to the daemon participant instead.
///
/// Dispatch is on the **method** alone, which is why this is registered under exactly one name —
/// see [`session_service_entries`].
///
/// **No terminal method is answered here.** The whole family left this coordinate in
/// `#unbundle` node 6 — the streaming half first, the five unary ones with the milestone that
/// removed family K from `connection.proto`. They are served on
/// `terminal_session.TerminalSessionService` by [`terminal_session_service`], over the same
/// [`terminal_manager::TerminalManager`] and the same control lease, so there is one answer rather
/// than two that could diverge.
struct SessionConnectionServiceRpc {
    svc: Arc<SessionConnectionService>,
}

/// The three [`ServiceEntry`]s a coder session participant registers, backed by one `svc`.
///
/// Used by `run.rs`, where the coder's own participant identity *is* the session participant
/// (`daemon-{instanceId}-{sessionId}`), and by [`spawn_session_participant`].
///
/// One entry per coordinate rather than one service registered under three names, because each
/// dispatches on the method alone: a single service answering at every coordinate would serve
/// `ListExecTools` on `terminal_session.TerminalSessionService` and `StreamAcpReplay` on
/// `connection.ConnectionService`, which is the opposite of moving the family. All are built from
/// the same `svc`, so they address one terminal manager, one lease and one transcript.
#[must_use]
pub fn session_service_entries(svc: SessionConnectionService) -> Vec<ServiceEntry> {
    session_service_entries_from(Arc::new(svc))
}

/// [`session_service_entries`] over a service the caller already shares.
fn session_service_entries_from(svc: Arc<SessionConnectionService>) -> Vec<ServiceEntry> {
    vec![
        ServiceEntry {
            name: "connection.ConnectionService",
            service: Arc::new(SessionConnectionServiceRpc {
                svc: Arc::clone(&svc),
            }) as Arc<dyn RpcService>,
        },
        activity_service::coder_activity_entry(Arc::clone(&svc)),
        terminal_session_service::coder_terminal_session_entry(svc),
    ]
}

#[async_trait]
impl RpcService for SessionConnectionServiceRpc {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        match method {
            "ListExecTools" => {
                if let Err(e) = ListExecToolsRequest::decode(&message.payload[..]) {
                    return RpcResult::Unary(Err(Status::invalid_argument(format!(
                        "decode ListExecToolsRequest: {e}"
                    ))));
                }
                let tools: Vec<ProtoToolDef> = self
                    .svc
                    .list_exec_tools()
                    .into_iter()
                    .map(|t| ProtoToolDef {
                        name: t.name,
                        description: t.description,
                        input_schema_json: t.input_schema_json,
                    })
                    .collect();
                let resp = ListExecToolsResponse { tools };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            "ExecuteTool" => {
                let req = match ExecuteToolRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode ExecuteToolRequest: {e}"
                        ))))
                    }
                };
                let r = self.svc.execute_tool(&req.tool_name, &req.args_json).await;
                let resp = ExecuteToolResponse {
                    result_json: r.result_json,
                    is_error: r.is_error,
                    error_message: r.error_message,
                    job_id: r.job_id,
                    job_running: r.job_running,
                };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            "ListSessionToolCalls" => {
                let req = match ListSessionToolCallsRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode ListSessionToolCallsRequest: {e}"
                        ))))
                    }
                };
                let rows = read_tool_calls(&self.svc.tool_calls_path, &req.session_id);
                let tool_calls: Vec<ToolCallInfo> = rows
                    .into_iter()
                    .map(|r| ToolCallInfo {
                        task_id: r.task_id,
                        tool_name: r.tool_name,
                        args_json: r.args_json,
                        result_json: r.result_json,
                        is_error: r.is_error,
                        error_message: r.error_message,
                        job_running: r.job_running,
                        created_unix_ms: r.created_unix_ms,
                    })
                    .collect();
                let resp = ListSessionToolCallsResponse { tool_calls };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "session participant does not serve ConnectionService/{other}"
            )))),
        }
    }
}

/// Read the session's `tool-calls.jsonl` as parsed records, scoped to `session_id`. Lines that fail
/// to parse are skipped with a warning (the file is append-only JSONL; a partial tail line is
/// tolerated).
fn read_tool_calls(
    path: &std::path::Path,
    _session_id: &str,
) -> Vec<connection_service_participant::ToolCallRecord> {
    use connection_service_participant::ToolCallRecord;
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            log::warn!(
                target: "tddy_coder::session_participant",
                "read_tool_calls: read {}: {}",
                path.display(),
                e
            );
            return Vec::new();
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| match serde_json::from_str::<ToolCallRecord>(l) {
            Ok(r) => Some(r),
            Err(e) => {
                log::warn!(
                    target: "tddy_coder::session_participant",
                    "read_tool_calls: skip malformed line: {}",
                    e
                );
                None
            }
        })
        .collect()
}
