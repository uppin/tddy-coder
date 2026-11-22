//! The host side of a jail's stdio bridge: dial the runner, relay its tool calls and terminal
//! events to the daemon. Moved out of `sandbox_session.rs` unchanged (`#tab-hardening` 3/4, file
//! length gate); its items are re-exported there, so every path is as before.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::ExecuteToolResponse;
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

/// Bridge a sandboxed process's piped stdio (spawned with `--stdio` in its argv — see
/// `tddy_sandbox_darwin::spawn_plan`) into an RPC endpoint, hosting `service` for inbound calls
/// (none exist in the current `SandboxService` protocol: the runner never initiates calls into
/// the daemon, only sends frames within the bidi `SessionChannel` the daemon itself calls) and
/// returning a client for calling into the runner. Spawns the endpoint's read/dispatch/write loop
/// on the current tokio runtime; the returned `JoinHandle` completes when the pipe closes (the
/// sandboxed process exits).
///
/// Does not return until the endpoint's read loop has started, so the first client frame cannot
/// race a peer that has not yet entered its dispatch loop (the stdio attach flake exercised by
/// `in_jail_conversation_acceptance`).
pub async fn bridge_sandbox_stdio<S: tddy_rpc::RpcService>(
    handle: &mut tddy_sandbox::SandboxHandle,
    service: S,
) -> Result<(Arc<tddy_stdio::StdioRpcClient>, tokio::task::JoinHandle<()>), String> {
    use std::os::fd::OwnedFd;

    let (stdin, stdout) = handle
        .take_stdio()
        .ok_or_else(|| "sandbox process was not spawned with piped stdio (--stdio)".to_string())?;
    let sender = tokio::net::unix::pipe::Sender::from_owned_fd(OwnedFd::from(stdin))
        .map_err(|e| format!("wrap sandbox stdin as async pipe: {e}"))?;
    let receiver = tokio::net::unix::pipe::Receiver::from_owned_fd(OwnedFd::from(stdout))
        .map_err(|e| format!("wrap sandbox stdout as async pipe: {e}"))?;
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        receiver,
        sender,
        service,
        tddy_rpc::RequestTransport::Pipe,
    );
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let endpoint = endpoint.signal_start_ready(ready_tx);
    let run_handle = tokio::spawn(async move { endpoint.run().await });
    ready_rx
        .await
        .map_err(|_| "sandbox stdio endpoint exited before its read loop started".to_string())?;
    Ok((client, run_handle))
}

/// TCP dialer for the in-jail gRPC server (macOS Seatbelt path; Linux uses AF_UNIX).
#[cfg(not(target_os = "linux"))]
async fn connect_sandbox_client(
    ready_marker: &Path,
) -> Result<SandboxServiceClient<tonic::transport::Channel>, String> {
    let port_str = std::fs::read_to_string(ready_marker).map_err(|e| e.to_string())?;
    let port: u16 = port_str
        .trim()
        .parse::<u16>()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    let endpoint = format!("http://127.0.0.1:{port}");
    SandboxServiceClient::connect(endpoint)
        .await
        .map_err(|e| format!("connect sandbox grpc: {e}"))
}

/// Tool handler that runs MCP tool calls in the session worktree via `tddy_tool_engine`.
pub(crate) struct DaemonToolHandler {
    pub(crate) worktree: PathBuf,
    pub(crate) task_registry: tddy_task::TaskRegistry,
    /// Extra env applied to spawned shell commands — for a managed session this carries the
    /// per-session `TDDY_SOCKET` (+ `PATH`) so host-side `tddy-tools transition` reaches the
    /// session's `WorkflowController`.
    pub(crate) session_env: Arc<Vec<(String, String)>>,
    /// Unified session directory holding `agent-activity.jsonl`, so every in-jail tool call is
    /// recorded as a `running` then terminal row for the web Agent Activity pane.
    pub(crate) session_dir: PathBuf,
    /// Live hub the recorded rows are published to for `StreamSessionActivity` subscribers.
    pub(crate) agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
}

#[async_trait]
impl tddy_sandbox_runner::HostToolHandler for DaemonToolHandler {
    async fn execute(
        &self,
        session_id: &str,
        conversation_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse {
        // A running row before execution, then a terminal row from the outcome. The durable log is
        // best-effort — a write failure is logged and never blocks the tool call (mirrors
        // tool_call_log handling in the ExecuteTool path).
        let call_id = uuid::Uuid::new_v4().to_string();
        let input = tddy_core::agent_activity::parse_activity_json(args_json);
        // What this call is credited with, for scoping its delta — AC2 of
        // `docs/ft/daemon/session-worktree-sync.md`. The same for both rows, because they describe
        // one call: what it declared is in its input, and the outcome cannot add to it.
        let declared_paths =
            tddy_core::agent_activity::declared_paths(tool_name, &input, &self.worktree);
        let running = tddy_core::agent_activity::AgentActivityRecord {
            call_id: call_id.clone(),
            tool_name: tool_name.to_string(),
            input: input.clone(),
            status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: tddy_daemon_kernel::now_unix_ms(),
            completed_unix_ms: 0,
            source: "sandbox".to_string(),
            // AC1 of the same document. Read from the filesystem, not by spawning `git rev-parse`:
            // this runs on every in-jail tool call. An unresolvable HEAD stamps an empty string
            // rather than a fabricated sha.
            head_commit: tddy_core::git_head::read_head_commit(&self.worktree),
            // The covering tick is the session room's poll loop to attribute; `0` is the wire's
            // "no tick has covered it yet".
            activity_seq: 0,
            changed_paths: declared_paths.clone(),
        };
        self.record_agent_activity(session_id, &running);

        let outcome =
            crate::conversation_tool::execute_in_conversation(crate::conversation_tool::ToolCall {
                worktree: &self.worktree,
                conversation_id,
                tool_name,
                args_json,
                registry: &self.task_registry,
                session_id,
                env: &self.session_env,
            })
            .await;

        let status = if outcome.is_error {
            tddy_core::agent_activity::STATUS_ERROR
        } else {
            tddy_core::agent_activity::STATUS_COMPLETED
        };
        let terminal = tddy_core::agent_activity::AgentActivityRecord {
            call_id,
            tool_name: tool_name.to_string(),
            input,
            status: status.to_string(),
            result: tddy_core::agent_activity::parse_activity_json(&outcome.result_json),
            error_message: outcome.error_message.clone(),
            started_unix_ms: running.started_unix_ms,
            completed_unix_ms: tddy_daemon_kernel::now_unix_ms(),
            source: "sandbox".to_string(),
            // Read again rather than copied from the `running` row: the call that just finished may
            // have been a `Bash` that committed, and this row is a record of where the checkout
            // stood when *it* was written.
            head_commit: tddy_core::git_head::read_head_commit(&self.worktree),
            activity_seq: 0,
            changed_paths: declared_paths,
        };
        self.record_agent_activity(session_id, &terminal);

        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}

impl DaemonToolHandler {
    /// Append an agent-activity row and publish it live; a persistence failure is non-fatal.
    ///
    // TODO(session-worktree-sync): also broadcast the row on the session room's `session.activity`
    // topic, as `ReportAgentActivity` does — AC4/AC5 of `docs/ft/daemon/session-worktree-sync.md`.
    // An in-jail MCP tool call reaches the durable log and the live hub but not the room, so a
    // participant mirroring a sandboxed session sees its edits only on the next poll tick. Doing it
    // needs `SessionRoomRegistry` threaded through `dial_and_bridge` into this handler.
    fn record_agent_activity(
        &self,
        session_id: &str,
        record: &tddy_core::agent_activity::AgentActivityRecord,
    ) {
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(&self.session_dir, record)
        {
            log::warn!(
                target: "tddy_daemon_sandbox::sandbox_session",
                "agent_activity: failed to persist {} row for session {}: {}",
                record.status,
                session_id,
                e
            );
        }
        self.agent_activity_hub.publish(session_id, record.clone());
    }
}

/// Dial the in-jail gRPC server over the platform's transport: loopback TCP on macOS (port from
/// the ready marker), AF_UNIX on Linux (a netns-isolated cgroups jail can't be reached over
/// loopback TCP — a UDS on the shared filesystem can).
#[cfg(target_os = "linux")]
pub async fn connect_sandbox_session_client(
    _ready_marker: &Path,
    grpc_socket: &Path,
) -> Result<SandboxServiceClient<tonic::transport::Channel>, String> {
    tddy_sandbox_runner::connect_sandbox_client_uds(grpc_socket)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub async fn connect_sandbox_session_client(
    ready_marker: &Path,
    _grpc_socket: &Path,
) -> Result<SandboxServiceClient<tonic::transport::Channel>, String> {
    connect_sandbox_client(ready_marker).await
}

/// The runner never initiates calls into the daemon over the stdio-served `SessionChannel` — it
/// only sends frames within the bidi call the daemon itself opens. This exists purely to satisfy
/// [`bridge_sandbox_stdio`]'s hosting requirement; any inbound request here is a bug.
pub struct NoCallbackSandboxService;

#[async_trait]
impl tddy_rpc::RpcService for NoCallbackSandboxService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(Status::unimplemented(format!(
            "tddy-daemon hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// Dial the sandboxed runner over its stdio-served `SessionChannel` and drive the host side via
/// the shared relay. PTY output fans into the broadcast channel (live subscribers) and the
/// rolling capture buffer (late subscribers); tool calls run in `worktree_path`. Returns the tasks
/// that do it, for the jail's state to stop with the jail
/// ([`JailBridge`](crate::jail_exit_watch::JailBridge)).
///
/// The relay admits every host the jail asks to tunnel to; [`dial_and_bridge_with_egress`] holds it
/// to a policy.
#[allow(clippy::too_many_arguments)]
pub async fn dial_and_bridge(
    session_id: &str,
    worktree_path: PathBuf,
    handle: &mut tddy_sandbox::SandboxHandle,
    task_registry: tddy_task::TaskRegistry,
    stdout_tx: broadcast::Sender<Bytes>,
    capture: Arc<StdMutex<TerminalCapture>>,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    session_env: Arc<Vec<(String, String)>>,
    session_dir: PathBuf,
    agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    rpc_handler: Arc<dyn tddy_sandbox_runner::HostRpcHandler>,
) -> Result<crate::jail_exit_watch::JailBridge, String> {
    dial_and_bridge_with_egress(
        session_id,
        worktree_path,
        handle,
        task_registry,
        stdout_tx,
        capture,
        stdin_rx,
        session_env,
        session_dir,
        agent_activity_hub,
        rpc_handler,
        tddy_sandbox_runner::EgressGuard::unrestricted(session_id),
    )
    .await
}

/// [`dial_and_bridge`] with the jail's egress policy enforced on the host relay's `TunnelOpen`s,
/// independently of the in-jail shim: a jailed process that speaks the relay protocol directly is
/// refused here too (`#agent-tools 3/12`).
#[allow(clippy::too_many_arguments)]
pub async fn dial_and_bridge_with_egress(
    session_id: &str,
    worktree_path: PathBuf,
    handle: &mut tddy_sandbox::SandboxHandle,
    task_registry: tddy_task::TaskRegistry,
    stdout_tx: broadcast::Sender<Bytes>,
    capture: Arc<StdMutex<TerminalCapture>>,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    session_env: Arc<Vec<(String, String)>>,
    session_dir: PathBuf,
    agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    rpc_handler: Arc<dyn tddy_sandbox_runner::HostRpcHandler>,
    egress: tddy_sandbox_runner::EgressGuard,
) -> Result<crate::jail_exit_watch::JailBridge, String> {
    log::info!(
        target: "tddy_daemon_sandbox::sandbox_session",
        "opening sandbox SessionChannel for session {session_id}"
    );

    let (client, endpoint) = bridge_sandbox_stdio(handle, NoCallbackSandboxService).await?;
    let stdio_client = tddy_sandbox_runner::StdioSandboxClient::new(client);

    // Output and the jail's size changes arrive on one channel, in order: a size change read apart
    // from the output around it would reinterpret that output at the wrong grid.
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    // `HostRelayConfig::new` requires a byte sink; with events set the relay never writes to it.
    let (term_tx, _unused_bytes_sink) = mpsc::unbounded_channel::<Bytes>();
    let forwarder =
        forward_jail_terminal_events(events_rx, Arc::clone(&capture), stdout_tx.clone());

    let handler = DaemonToolHandler {
        worktree: worktree_path,
        task_registry,
        session_env,
        session_dir,
        agent_activity_hub,
    };
    let relay = tddy_sandbox_runner::run_host_relay_with_rpc(
        stdio_client,
        handler,
        rpc_handler,
        tddy_sandbox_runner::HostRelayConfig::new(session_id, term_tx)
            .with_terminal_events(events_tx)
            .with_egress(egress),
        stdin_rx,
    )
    .await?;
    Ok(crate::jail_exit_watch::JailBridge::new(
        endpoint, relay, forwarder,
    ))
}

/// Feed a jailed terminal's reported events into its capture and live broadcast: output is appended
/// and broadcast under the capture lock (the sequence point a snapshot depends on), and a size
/// change resizes the capture's screen model at the same point in the stream, so snapshots of a
/// jailed terminal are serialized at the jail's real grid. Runs until `events` closes.
pub fn forward_jail_terminal_events(
    mut events: mpsc::UnboundedReceiver<tddy_sandbox_runner::HostTerminalEvent>,
    capture: Arc<StdMutex<TerminalCapture>>,
    stdout_tx: broadcast::Sender<Bytes>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                tddy_sandbox_runner::HostTerminalEvent::Output(chunk) => {
                    if let Ok(mut cap) = capture.lock() {
                        cap.append(&chunk);
                        let _ = stdout_tx.send(chunk);
                    }
                }
                tddy_sandbox_runner::HostTerminalEvent::Resized { cols, rows } => {
                    if let Ok(mut cap) = capture.lock() {
                        cap.resize(rows, cols);
                    }
                }
            }
        }
    })
}
