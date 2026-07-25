//! Session-participant module — the tddy-coder process serves session-scoped
//! `ConnectionService` RPCs (tools, terminal control) from its own LiveKit participant and
//! publishes `session` metadata.
//!
//! `DeleteSession` / `SignalSession` are **not** served here: the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).

pub mod acp_transcript;
pub mod connection_service_participant;
pub mod metadata_publisher;
pub mod terminal_manager;

pub use acp_transcript::{append_frames_for_event, frame_for_event, spawn_acp_transcript_writer};
pub use connection_service_participant::{
    coder_session_tool_catalog, coder_session_tool_catalog_full, CoderSessionToolExecutor,
    SessionConnectionService, ToolDef, ToolExecutor, ToolOutcome,
};
pub use metadata_publisher::{
    session_metadata_json, spawn_session_metadata_tap, SessionMetadata, SessionMetadataSeed,
};

use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;
use tokio::sync::watch;

use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::connection::{
    AcpReplayFrame, ClaimTerminalControlRequest, ClaimTerminalControlResponse, ExecuteToolRequest,
    ExecuteToolResponse, GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse,
    ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse, ListTerminalSessionsRequest, ListTerminalSessionsResponse,
    SendTerminalInputResponse, SessionTerminalInput, SessionTerminalOutput,
    StartTerminalSessionRequest, StartTerminalSessionResponse, StopTerminalSessionRequest,
    StopTerminalSessionResponse, StreamAcpReplayRequest, StreamMode, StreamSessionActivityRequest,
    StreamTerminalOutputRequest, TerminalSessionInfo, ToolCallInfo, ToolDef as ProtoToolDef,
};

use terminal_manager::MAIN_TERMINAL_ID;

/// Buffer size for the `StreamTerminalOutput` server-stream bridge (replay frame + live output).
/// Bounds memory if the client reads slower than the shell produces; overflow applies backpressure.
const TERMINAL_OUTPUT_CHANNEL_CAPACITY: usize = 256;

/// Buffer size for the `StreamSessionActivity` server-stream bridge (snapshot rows + live tail).
const AGENT_ACTIVITY_CHANNEL_CAPACITY: usize = 256;

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
    let rpc = SessionConnectionServiceRpc { svc };

    let mut entries = vec![ServiceEntry {
        name: "connection.ConnectionService",
        service: Arc::new(rpc) as Arc<dyn RpcService>,
    }];
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
struct SessionConnectionServiceRpc {
    svc: Arc<SessionConnectionService>,
}

/// Build a `connection.ConnectionService` [`ServiceEntry`] backed by `svc`, for registering on an
/// existing LiveKit participant's `MultiRpcService` (used by `run.rs` when the coder's own
/// participant identity is the session participant, `daemon-{instanceId}-{sessionId}`).
pub fn session_connection_service_entry(svc: SessionConnectionService) -> ServiceEntry {
    ServiceEntry {
        name: "connection.ConnectionService",
        service: Arc::new(SessionConnectionServiceRpc { svc: Arc::new(svc) })
            as Arc<dyn RpcService>,
    }
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
            "ClaimTerminalControl" => {
                let req = match ClaimTerminalControlRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode ClaimTerminalControlRequest: {e}"
                        ))))
                    }
                };
                let r = self.svc.claim_terminal_control(&req.screen_id, req.steal);
                let resp = ClaimTerminalControlResponse {
                    granted: r.granted,
                    control_token: r.control_token,
                    current_holder_screen_id: String::new(),
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
            "StartTerminalSession" => {
                if let Err(e) = StartTerminalSessionRequest::decode(&message.payload[..]) {
                    return RpcResult::Unary(Err(Status::invalid_argument(format!(
                        "decode StartTerminalSessionRequest: {e}"
                    ))));
                }
                // Bash terminals run the user's login shell (resolved from passwd, not the
                // possibly-Nix `$SHELL`), falling back to /bin/bash. The coder already runs as the
                // target OS user, so no impersonation is applied.
                let shell = terminal_manager::resolve_login_shell();
                match self
                    .svc
                    .terminal_manager
                    .start_terminal(&self.svc.session_id, self.svc.worktree.clone(), &shell)
                    .await
                {
                    Ok(handle) => {
                        let resp = StartTerminalSessionResponse {
                            terminal_id: handle.terminal_id.clone(),
                        };
                        RpcResult::Unary(Ok(resp.encode_to_vec()))
                    }
                    Err(e) => RpcResult::Unary(Err(Status::internal(format!(
                        "failed to start terminal: {e}"
                    )))),
                }
            }
            "StopTerminalSession" => {
                let req = match StopTerminalSessionRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode StopTerminalSessionRequest: {e}"
                        ))))
                    }
                };
                let terminal_id = req.terminal_id.trim();
                // The main terminal is torn down via Delete/Signal on the daemon, never here.
                if terminal_id == MAIN_TERMINAL_ID {
                    return RpcResult::Unary(Err(Status::invalid_argument(
                        "the main terminal cannot be stopped via StopTerminalSession; \
                         use SignalSession or DeleteSession",
                    )));
                }
                if self.svc.terminal_manager.stop_terminal(terminal_id).await {
                    let resp = StopTerminalSessionResponse {
                        ok: true,
                        message: String::new(),
                    };
                    RpcResult::Unary(Ok(resp.encode_to_vec()))
                } else {
                    RpcResult::Unary(Err(Status::not_found("terminal not found")))
                }
            }
            "ListTerminalSessions" => {
                if let Err(e) = ListTerminalSessionsRequest::decode(&message.payload[..]) {
                    return RpcResult::Unary(Err(Status::invalid_argument(format!(
                        "decode ListTerminalSessionsRequest: {e}"
                    ))));
                }
                let terminals: Vec<TerminalSessionInfo> = self
                    .svc
                    .terminal_manager
                    .list_terminals()
                    .await
                    .iter()
                    .map(|h| TerminalSessionInfo {
                        terminal_id: h.terminal_id.clone(),
                        kind: h.kind.clone(),
                        pid: h.pid,
                    })
                    .collect();
                let resp = ListTerminalSessionsResponse { terminals };
                RpcResult::Unary(Ok(resp.encode_to_vec()))
            }
            "SendTerminalInput" => {
                let req = match SessionTerminalInput::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode SessionTerminalInput: {e}"
                        ))))
                    }
                };
                let terminal_id = resolved_terminal_id(&req.terminal_id);
                match self.svc.terminal_manager.get_terminal(terminal_id).await {
                    Some(handle) => {
                        if !req.data.is_empty() {
                            let input_offset = req.input_offset;
                            handle.send_input(tddy_pty::Bytes::from(req.data), input_offset);
                        }
                        RpcResult::Unary(Ok(SendTerminalInputResponse {}.encode_to_vec()))
                    }
                    None => RpcResult::Unary(Err(Status::not_found(
                        "terminal not found or not running",
                    ))),
                }
            }
            "StreamTerminalOutput" => {
                let req = match StreamTerminalOutputRequest::decode(&message.payload[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::ServerStream(Err(Status::invalid_argument(format!(
                            "decode StreamTerminalOutputRequest: {e}"
                        ))))
                    }
                };
                let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
                let handle = match self.svc.terminal_manager.get_terminal(&terminal_id).await {
                    Some(h) => h,
                    None => {
                        return RpcResult::ServerStream(Err(Status::not_found(
                            "terminal not found or not running",
                        )))
                    }
                };

                // Resize the PTY to the client's dimensions before replay so the shell redraws at
                // the browser's actual width rather than the PTY's spawn-time default.
                if req.initial_cols > 0 && req.initial_rows > 0 {
                    handle
                        .resize(req.initial_rows as u16, req.initial_cols as u16)
                        .await;
                }

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(
                    TERMINAL_OUTPUT_CHANNEL_CAPACITY,
                );

                // Subscribe BEFORE snapshotting the capture buffer so bytes produced between the
                // snapshot and the first bridge recv() are still delivered via the broadcast.
                let mut stdout_rx = handle.stdout_tx.subscribe();
                let replay = handle
                    .capture
                    .lock()
                    .map(|cap| cap.replay())
                    .unwrap_or_default();
                if !replay.is_empty() {
                    let frame = SessionTerminalOutput {
                        data: replay,
                        acked_input_offset: 0,
                    }
                    .encode_to_vec();
                    let _ = tx.try_send(Ok(frame));
                }

                // Input-offset ACKs ride the same output stream (docs/ft/web/enqueued-input-overlay.md):
                // when the applied input offset advances, emit an empty-data frame carrying it. Emit
                // the current offset up front so a stream opened after some input was already applied
                // learns the acknowledged position immediately.
                let mut acked_rx = handle.subscribe_acked_offset();
                let initial_acked = *acked_rx.borrow_and_update();
                if initial_acked > 0 {
                    let frame = SessionTerminalOutput {
                        data: Vec::new(),
                        acked_input_offset: initial_acked,
                    }
                    .encode_to_vec();
                    let _ = tx.try_send(Ok(frame));
                }

                // Bridge live PTY output → the server stream, interleaving ACK frames, ending when
                // the shell exits.
                let mut pty_done = handle.pty_done.clone();
                tokio::spawn(async move {
                    use tokio::sync::broadcast::error::RecvError;
                    // Disabled once the ACK sender drops, so a closed watch never busy-spins.
                    let mut ack_open = true;
                    loop {
                        tokio::select! {
                            result = stdout_rx.recv() => match result {
                                Ok(bytes) => {
                                    let frame = SessionTerminalOutput {
                                        data: bytes.to_vec(),
                                        acked_input_offset: 0,
                                    }
                                    .encode_to_vec();
                                    if tx.send(Ok(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
                            },
                            changed = acked_rx.changed(), if ack_open => {
                                match changed {
                                    Ok(()) => {
                                        let offset = *acked_rx.borrow_and_update();
                                        let frame = SessionTerminalOutput {
                                            data: Vec::new(),
                                            acked_input_offset: offset,
                                        }
                                        .encode_to_vec();
                                        if tx.send(Ok(frame)).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => ack_open = false,
                                }
                            },
                            _ = pty_done.changed() => break,
                        }
                    }
                });

                RpcResult::ServerStream(Ok(rx))
            }
            "StreamSessionActivity" => {
                let req = match StreamSessionActivityRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::ServerStream(Err(Status::invalid_argument(format!(
                            "decode StreamSessionActivityRequest: {e}"
                        ))));
                    }
                };
                let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(
                    AGENT_ACTIVITY_CHANNEL_CAPACITY,
                );

                // Subscribe to the live tail BEFORE snapshotting the durable log so a record
                // appended between the snapshot read and the first bridge recv() is still delivered
                // (via the broadcast) rather than dropped in the gap.
                let live_rx = self.svc.presenter_events.as_ref().map(|tx| tx.subscribe());

                // Snapshot-then-live (the default) replays the coalesced on-disk records first;
                // live-only skips the snapshot and carries only records that arrive after subscribe.
                if mode == StreamMode::SnapshotThenLive {
                    let snapshot = tddy_core::agent_activity::read_agent_activity(
                        &self.svc.agent_activity_dir,
                    )
                    .unwrap_or_default();
                    for record in snapshot {
                        let frame = tddy_service::agent_activity_to_proto(record).encode_to_vec();
                        if tx.try_send(Ok(frame)).is_err() {
                            // Receiver already gone — return the (now-closed) stream.
                            return RpcResult::ServerStream(Ok(rx));
                        }
                    }
                }

                // Live tail: forward every AgentActivity the presenter broadcasts, ending when the
                // presenter channel closes or the client disconnects.
                if let Some(mut live_rx) = live_rx {
                    tokio::spawn(async move {
                        use tokio::sync::broadcast::error::RecvError;
                        loop {
                            match live_rx.recv().await {
                                Ok(tddy_core::PresenterEvent::AgentActivity(record)) => {
                                    let frame = tddy_service::agent_activity_to_proto(record)
                                        .encode_to_vec();
                                    if tx.send(Ok(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(_) => continue,
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
                            }
                        }
                    });
                }

                RpcResult::ServerStream(Ok(rx))
            }
            "StreamAcpReplay" => {
                let req = match StreamAcpReplayRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::ServerStream(Err(Status::invalid_argument(format!(
                            "decode StreamAcpReplayRequest: {e}"
                        ))));
                    }
                };
                let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(
                    AGENT_ACTIVITY_CHANNEL_CAPACITY,
                );

                // Wrap one ACP frame in the connection-local `AcpReplayFrame` envelope and encode it
                // to the transport bytes the client decodes back into an `AcpReplayFrame`.
                fn replay_frame_bytes(
                    frame: &tddy_service::proto::acp::AcpAgentMessage,
                ) -> Vec<u8> {
                    AcpReplayFrame {
                        acp_agent_message: tddy_service::acp_replay::strip_tool_body(frame)
                            .encode_to_vec(),
                        // A transcript frame carries no count; count-first mode sets this instead.
                        activity_count: 0,
                    }
                    .encode_to_vec()
                }

                // Encode a count-only `AcpReplayFrame` envelope (no transcript payload) carrying the
                // running number of persisted activity frames — the cheap feed for the overlay badge.
                fn count_frame_bytes(activity_count: u64) -> Vec<u8> {
                    AcpReplayFrame {
                        acp_agent_message: Vec::new(),
                        activity_count,
                    }
                    .encode_to_vec()
                }

                // Subscribe to the live tail BEFORE snapshotting so an event produced between the
                // snapshot read and the first bridge recv() is still delivered (via the broadcast)
                // rather than dropped in the gap.
                let live_rx = self.svc.presenter_events.as_ref().map(|tx| tx.subscribe());

                // Count-first mode emits only the running count of persisted transcript frames — one
                // frame now with the current count, then a fresh count for each subsequent renderable
                // presenter event — with no transcript payload. It never replays the snapshot itself.
                if mode == StreamMode::CountThenLive {
                    let snapshot = tddy_service::acp_replay::read_session_transcript(
                        &self.svc.agent_activity_dir,
                    )
                    .unwrap_or_default();
                    let mut count = tddy_service::acp_replay::count_activity_entries(&snapshot);
                    let mut seen_ids = tddy_service::acp_replay::tool_call_ids(&snapshot);
                    if tx.try_send(Ok(count_frame_bytes(count))).is_err() {
                        // Receiver already gone — return the (now-closed) stream.
                        return RpcResult::ServerStream(Ok(rx));
                    }
                    if let Some(mut live_rx) = live_rx {
                        tokio::spawn(async move {
                            use tddy_core::PresenterEvent;
                            use tokio::sync::broadcast::error::RecvError;
                            loop {
                                match live_rx.recv().await {
                                    Ok(event) => {
                                        // Count each new entry the pane would render: agent text
                                        // always, a tool call once (coalesced by call_id across its
                                        // running + terminal records).
                                        let counts = match &event {
                                            PresenterEvent::AgentOutput(_) => true,
                                            PresenterEvent::AgentActivity(record) => {
                                                seen_ids.insert(record.call_id.clone())
                                            }
                                            _ => false,
                                        };
                                        if !counts {
                                            continue;
                                        }
                                        count += 1;
                                        if tx.send(Ok(count_frame_bytes(count))).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(RecvError::Closed) => break,
                                    Err(RecvError::Lagged(_)) => continue,
                                }
                            }
                        });
                    }
                    return RpcResult::ServerStream(Ok(rx));
                }

                // Snapshot-then-live (the default) replays the session's resolved transcript first
                // (the persisted ACP frames merged with the durable agent-activity rows);
                // live-only skips it and carries only frames produced after subscribe.
                if mode == StreamMode::SnapshotThenLive {
                    let snapshot = tddy_service::acp_replay::read_session_transcript(
                        &self.svc.agent_activity_dir,
                    )
                    .unwrap_or_default();
                    for frame in snapshot {
                        if tx.try_send(Ok(replay_frame_bytes(&frame))).is_err() {
                            // Receiver already gone — return the (now-closed) stream.
                            return RpcResult::ServerStream(Ok(rx));
                        }
                    }
                }

                // Live tail: map every renderable presenter event to its ACP frame (via the same
                // mapper the on-disk writer uses) and forward it, ending when the presenter channel
                // closes or the client disconnects.
                if let Some(mut live_rx) = live_rx {
                    tokio::spawn(async move {
                        use tokio::sync::broadcast::error::RecvError;
                        loop {
                            match live_rx.recv().await {
                                Ok(event) => {
                                    let Some(frame) = acp_transcript::frame_for_event(
                                        &event,
                                        acp_transcript::now_unix_ms(),
                                    ) else {
                                        continue;
                                    };
                                    if tx.send(Ok(replay_frame_bytes(&frame))).await.is_err() {
                                        break;
                                    }
                                }
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
                            }
                        }
                    });
                }

                RpcResult::ServerStream(Ok(rx))
            }
            "GetAcpToolCallDetail" => {
                let req = match GetAcpToolCallDetailRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode GetAcpToolCallDetailRequest: {e}"
                        ))));
                    }
                };
                match tddy_service::acp_replay::tool_call_detail(
                    &self.svc.agent_activity_dir,
                    &req.tool_call_id,
                ) {
                    Err(e) => {
                        RpcResult::Unary(Err(Status::internal(format!("read transcript: {e}"))))
                    }
                    Ok(None) => RpcResult::Unary(Err(Status::not_found(format!(
                        "no tool call with id {} in this session",
                        req.tool_call_id
                    )))),
                    Ok(Some(d)) => RpcResult::Unary(Ok(GetAcpToolCallDetailResponse {
                        raw_input: d.raw_input,
                        raw_output: d.raw_output,
                    }
                    .encode_to_vec())),
                }
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "session participant does not serve ConnectionService/{other}"
            )))),
        }
    }
}

/// Resolve a request's `terminal_id`, mapping an empty value to the reserved main terminal.
fn resolved_terminal_id(terminal_id: &str) -> &str {
    let trimmed = terminal_id.trim();
    if trimmed.is_empty() {
        MAIN_TERMINAL_ID
    } else {
        trimmed
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::broadcast;

    use tddy_core::agent_activity::{
        append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
    };
    use tddy_core::PresenterEvent;
    use tddy_service::proto::connection::AgentActivityRecord as ProtoAgentActivityRecord;

    /// Executor that is never invoked by the `StreamSessionActivity` path.
    struct UnusedExecutor;
    #[async_trait]
    impl ToolExecutor for UnusedExecutor {
        async fn execute(&self, _tool_name: &str, _args_json: &str) -> ToolOutcome {
            ToolOutcome::default()
        }
    }

    fn a_running_record(call_id: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: call_id.to_string(),
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "cargo build" }),
            status: STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 0,
            source: "coder".to_string(),
        }
    }

    fn a_completed_record(call_id: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "done" }),
            completed_unix_ms: 1_700_000_000_500,
            ..a_running_record(call_id)
        }
    }

    fn stream_request_message(session_id: &str) -> RpcMessage {
        let req = StreamSessionActivityRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    fn rpc_for(
        dir: &std::path::Path,
        events: broadcast::Sender<PresenterEvent>,
    ) -> SessionConnectionServiceRpc {
        SessionConnectionServiceRpc {
            svc: Arc::new(SessionConnectionService {
                session_id: "sess-1".to_string(),
                session_token: "session-token".to_string(),
                tool_calls_path: dir.join("tool-calls.jsonl"),
                tools: Vec::new(),
                executor: Arc::new(UnusedExecutor),
                worktree: dir.to_path_buf(),
                terminal_manager: Arc::new(terminal_manager::TerminalManager::new()),
                agent_activity_dir: dir.to_path_buf(),
                presenter_events: Some(events),
            }),
        }
    }

    async fn recv_record(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> ProtoAgentActivityRecord {
        let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed agent-activity frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        ProtoAgentActivityRecord::decode(&frame[..]).expect("decode AgentActivityRecord")
    }

    #[tokio::test]
    async fn stream_session_activity_replays_the_persisted_snapshot_then_the_live_broadcast() {
        // Given — a session dir with one persisted (coalesced) call, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamSessionActivity arm is dispatched
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "StreamSessionActivity",
                &stream_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the snapshot's coalesced completed call arrives first
        let snapshot = recv_record(&mut rx).await;
        assert_eq!(snapshot.call_id, "call-1");
        assert_eq!(snapshot.status, STATUS_COMPLETED);
        assert_eq!(
            snapshot.result,
            tddy_service::json_to_proto_value(&serde_json::json!({ "stdout": "done" }))
        );

        // And — a subsequently-broadcast AgentActivity is forwarded live
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-2")))
            .expect("broadcast send");
        let live = recv_record(&mut rx).await;
        assert_eq!(live.call_id, "call-2");
        assert_eq!(live.status, STATUS_RUNNING);
        assert_eq!(live.tool_name, "Bash");
    }

    fn stream_request_message_live_only(session_id: &str) -> RpcMessage {
        let req = StreamSessionActivityRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: tddy_service::proto::connection::StreamMode::LiveOnly as i32,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    #[tokio::test]
    async fn stream_session_activity_in_live_only_mode_skips_the_persisted_snapshot() {
        // Given — a session dir with a persisted call, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-snapshot")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamSessionActivity arm is dispatched in LIVE_ONLY mode
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "StreamSessionActivity",
                &stream_request_message_live_only("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // and — a record is broadcast live after the subscription
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-live")))
            .expect("broadcast send");

        // Then — the first frame is the live record; the persisted 'call-snapshot' was skipped
        let first = recv_record(&mut rx).await;
        assert_eq!(
            first.call_id, "call-live",
            "live-only must not replay the persisted snapshot ('call-snapshot')"
        );
    }

    fn acp_replay_request_message(session_id: &str) -> RpcMessage {
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    /// Receive one streamed replay byte-frame and decode its inner ACP `AcpAgentMessage`.
    async fn recv_acp_frame(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> tddy_service::proto::acp::AcpAgentMessage {
        let bytes = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed replay frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        let envelope = AcpReplayFrame::decode(&bytes[..]).expect("decode AcpReplayFrame");
        tddy_service::proto::acp::AcpAgentMessage::decode(&envelope.acp_agent_message[..])
            .expect("decode inner AcpAgentMessage")
    }

    /// The text of an `agent_message_chunk` ACP frame (panics on any other shape).
    fn acp_agent_text(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                    Some(session_update::Update::AgentMessageChunk(c)) => {
                        match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                            Some(content_block::Block::Text(t)) => t.text.clone(),
                            other => panic!("expected text content, got {other:?}"),
                        }
                    }
                    other => panic!("expected AgentMessageChunk, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// The tool_call_id of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call_id(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => {
                        tc.tool_call_id.map(|id| id.value).unwrap_or_default()
                    }
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stream_acp_replay_replays_the_persisted_transcript_then_the_live_broadcast() {
        // Given — a session dir with one persisted ACP transcript frame, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamAcpReplay arm is dispatched
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the persisted agent-text frame arrives first
        let snapshot = recv_acp_frame(&mut rx).await;
        assert_eq!(acp_agent_text(&snapshot), "Analyzing the parser.");

        // And — a subsequently-broadcast AgentActivity is mapped to a live tool_call frame
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-2")))
            .expect("broadcast send");
        let live = recv_acp_frame(&mut rx).await;
        assert_eq!(acp_tool_call_id(&live), "call-2");
    }

    /// Receive one streamed byte-frame and decode the raw `AcpReplayFrame` envelope (the count
    /// carrier), with a timeout so a count-mode subscription that never emits fails fast.
    async fn recv_acp_envelope(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> AcpReplayFrame {
        let bytes = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed replay frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        AcpReplayFrame::decode(&bytes[..]).expect("decode AcpReplayFrame")
    }

    #[tokio::test]
    async fn stream_acp_replay_count_then_live_broadcasts_the_activity_count() {
        // Given — a session dir with two persisted transcript frames and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_running_record("call-a")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamAcpReplay arm is dispatched in count-first mode
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let result = rpc
            .handle_rpc("connection.ConnectionService", "StreamAcpReplay", &message)
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the first frame carries the current count (2) and no transcript payload
        let first = recv_acp_envelope(&mut rx).await;
        assert_eq!(first.activity_count, 2);
        assert!(
            first.acp_agent_message.is_empty(),
            "a count frame must not carry a transcript payload"
        );

        // And — a subsequently-broadcast AgentActivity raises the count to 3
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-live")))
            .expect("broadcast send");
        let next = recv_acp_envelope(&mut rx).await;
        assert_eq!(next.activity_count, 3);
    }

    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_a_tool_call_once_across_its_two_records() {
        // Given — a session dir with one persisted agent-text frame (count baseline 1)
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let mut rx = match rpc
            .handle_rpc("connection.ConnectionService", "StreamAcpReplay", &message)
            .await
        {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 1);

        // When — a tool call broadcasts its running then terminal record under one call_id
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-x")))
            .expect("broadcast send");
        // Then — the first (running) record lifts the count to 2
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 2);
        // The terminal record for call-x emits nothing; a distinct call is the next frame and reads
        // 3, proving call-x was counted once.
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-x")))
            .expect("broadcast send");
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-y")))
            .expect("broadcast send");
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 3);
    }

    // -----------------------------------------------------------------------
    // Persisted-activity replay (bug fc990524: badge counts, pane opens empty)
    //
    // `acp-transcript.jsonl` only exists for sessions that ran the presenter seam that writes it; a
    // session started before it (or one whose tool calls were recorded by another host) has only the
    // durable `agent-activity.jsonl`. Replaying the transcript file alone serves an empty snapshot
    // while the count feed keeps counting live records — badge, but nothing to see.
    // -----------------------------------------------------------------------

    /// The snapshot must project the session's durable `agent-activity.jsonl` rows, coalesced by
    /// call_id, when no ACP transcript was written.
    #[tokio::test]
    async fn stream_acp_replay_replays_persisted_agent_activity_when_no_acp_transcript_exists() {
        // Given — a session dir whose activity log holds one coalesced call and one still-running
        // call, and no `acp-transcript.jsonl`
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-2")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay arm is dispatched in snapshot mode
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — both persisted calls are replayed as tool_call frames, each once, in recorded order
        assert_eq!(acp_tool_call_id(&recv_acp_frame(&mut rx).await), "call-1");
        assert_eq!(acp_tool_call_id(&recv_acp_frame(&mut rx).await), "call-2");
    }

    /// The count baseline comes from the same resolved transcript the snapshot replays, so a badge
    /// never promises entries the pane cannot deliver.
    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_persisted_agent_activity_rows() {
        // Given — the same dir: two distinct calls persisted in the activity log alone
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-2")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay arm is dispatched in count-first mode
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let mut rx = match rpc
            .handle_rpc("connection.ConnectionService", "StreamAcpReplay", &message)
            .await
        {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the first count frame reports the two persisted calls
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 2);
    }

    /// The full `ToolCall` payload of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call(
        frame: &tddy_service::proto::acp::AcpAgentMessage,
    ) -> tddy_service::proto::acp::ToolCall {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => tc,
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    fn detail_request_message(session_id: &str, tool_call_id: &str) -> RpcMessage {
        let req = GetAcpToolCallDetailRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            tool_call_id: tool_call_id.to_string(),
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    /// A `SNAPSHOT_THEN_LIVE` tool-call frame carries the call's id but not its bodies: the heavy
    /// `raw_input`/`raw_output` are stripped so the stream stays small.
    #[tokio::test]
    async fn stream_acp_replay_snapshot_frames_omit_tool_bodies() {
        // Given — a session dir whose persisted transcript holds a completed call with full bodies
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay snapshot arm is dispatched
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the tool call arrives with its id intact but neither body
        let tc = acp_tool_call(&recv_acp_frame(&mut rx).await);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-1");
        assert_eq!(tc.raw_input, None);
        assert_eq!(tc.raw_output, None);
    }

    /// The bodies the stream strips are fetched on demand: GetAcpToolCallDetail returns the exact
    /// raw_input/raw_output the transcript recorded for one call.
    #[tokio::test]
    async fn get_acp_tool_call_detail_returns_the_full_tool_bodies() {
        // Given — a session dir whose transcript holds a completed Bash call
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the detail for that call is requested
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "GetAcpToolCallDetail",
                &detail_request_message("sess-1", "call-1"),
            )
            .await;
        let bytes = match result {
            RpcResult::Unary(Ok(bytes)) => bytes,
            RpcResult::Unary(Err(status)) => {
                panic!("expected a detail response, got error status: {status:?}")
            }
            _ => panic!("expected a unary result, got a server stream"),
        };
        let resp = GetAcpToolCallDetailResponse::decode(&bytes[..])
            .expect("decode GetAcpToolCallDetailResponse");

        // Then — it returns the exact bodies the stream used to inline
        let raw_input: serde_json::Value =
            serde_json::from_str(&resp.raw_input.expect("raw_input")).expect("raw_input is JSON");
        let raw_output: serde_json::Value =
            serde_json::from_str(&resp.raw_output.expect("raw_output"))
                .expect("raw_output is JSON");
        assert_eq!(raw_input, serde_json::json!({ "command": "cargo build" }));
        assert_eq!(raw_output, serde_json::json!({ "stdout": "done" }));
    }

    /// A tool_call_id absent from the transcript is a NOT_FOUND error, not an empty success.
    #[tokio::test]
    async fn get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id() {
        // Given — a session dir whose transcript holds only call-1
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the detail for a non-existent call is requested
        let result = rpc
            .handle_rpc(
                "connection.ConnectionService",
                "GetAcpToolCallDetail",
                &detail_request_message("sess-1", "does-not-exist"),
            )
            .await;
        let status = match result {
            RpcResult::Unary(Err(status)) => status,
            RpcResult::Unary(Ok(_)) => panic!("expected NOT_FOUND, got a success response"),
            _ => panic!("expected a unary result, got a server stream"),
        };

        // Then — the status is NOT_FOUND
        assert_eq!(status.code(), tddy_rpc::Code::NotFound);
    }
}
