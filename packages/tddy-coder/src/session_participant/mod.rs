//! Session-participant module — the tddy-coder process serves session-scoped
//! `ConnectionService` RPCs (tools, terminal control) from its own LiveKit participant and
//! publishes `session` metadata.
//!
//! `DeleteSession` / `SignalSession` are **not** served here: the web routes them directly to the
//! daemon participant (`daemon-{instanceId}`), which owns process teardown and must be reachable
//! even when the coder participant is stuck (changeset `2026-07-12-fast-session-change`).

pub mod connection_service_participant;
pub mod metadata_publisher;
pub mod terminal_manager;

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
    ClaimTerminalControlRequest, ClaimTerminalControlResponse, ExecuteToolRequest,
    ExecuteToolResponse, ListExecToolsRequest, ListExecToolsResponse, ListSessionToolCallsRequest,
    ListSessionToolCallsResponse, ListTerminalSessionsRequest, ListTerminalSessionsResponse,
    SendTerminalInputResponse, SessionTerminalInput, SessionTerminalOutput,
    StartTerminalSessionRequest, StartTerminalSessionResponse, StopTerminalSessionRequest,
    StopTerminalSessionResponse, StreamMode, StreamSessionActivityRequest,
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
                            handle.send_input(tddy_pty::Bytes::from(req.data));
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
                    .map(|cap| cap.clone())
                    .unwrap_or_default();
                if !replay.is_empty() {
                    let frame = SessionTerminalOutput { data: replay }.encode_to_vec();
                    let _ = tx.try_send(Ok(frame));
                }

                // Bridge live PTY output → the server stream, ending when the shell exits.
                let mut pty_done = handle.pty_done.clone();
                tokio::spawn(async move {
                    use tokio::sync::broadcast::error::RecvError;
                    loop {
                        tokio::select! {
                            result = stdout_rx.recv() => match result {
                                Ok(bytes) => {
                                    let frame = SessionTerminalOutput { data: bytes.to_vec() }
                                        .encode_to_vec();
                                    if tx.send(Ok(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
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
}
