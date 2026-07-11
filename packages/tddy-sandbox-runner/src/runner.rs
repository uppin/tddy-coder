//! Sandbox runner — in-jail gRPC server + claude PTY + MCP tool-exec bridge.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

use tddy_service::proto::connection::{
    ExecuteToolRequest, ExecuteToolResponse, SessionTerminalOutput,
};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{
    EchoRequest, EchoResponse, EchoStreamFrame, EgressRequest, EgressResponse, SessionEnded,
    SessionFrame, TunnelClose, TunnelData, TunnelOpen, TunnelOpenAck,
};
use tddy_service::tonic_sandbox::sandbox_service_server::{
    SandboxService as TonicSandboxService, SandboxServiceServer as TonicSandboxServiceServer,
};

use tddy_sandbox::{
    append_line, egress_log_path, session_id_from_env, SANDBOX_RUNNER_FAILURE, SANDBOX_RUNNER_LOG,
};
use tddy_sandbox_recipes::{
    append_claude_mcp_args, claude_scratch_mcp_dir,
};

/// Hosts `connection.ConnectionService/ExecuteTool` over the tool-IPC socket, using `tddy-rpc`'s
/// length-prefixed framing instead of the old unframed single-`read()`/`write_all()` JSON
/// protocol (which silently truncated payloads that didn't arrive in one syscall).
struct ToolExecService {
    relay: Arc<SandboxSessionRelay>,
}

#[async_trait::async_trait]
impl tddy_rpc::RpcService for ToolExecService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        use prost::Message;
        if service != "connection.ConnectionService" || method != "ExecuteTool" {
            return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "unknown {service}/{method}"
            ))));
        }
        let req = match ExecuteToolRequest::decode(message.payload.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::invalid_argument(
                    format!("decode ExecuteToolRequest: {e}"),
                )))
            }
        };
        let resp = self.relay.call_tool(&req.tool_name, &req.args_json).await;
        tddy_rpc::RpcResult::Unary(Ok(resp.encode_to_vec()))
    }
}

fn egress_dir_from_env() -> Option<PathBuf> {
    std::env::var("TDDY_SANDBOX_EGRESS_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

static BOOT_LOG_FALLBACK: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn set_boot_log_fallback(dir: PathBuf) {
    let _ = BOOT_LOG_FALLBACK.set(dir);
}

/// Append a boot line before `env_logger` init — writes to egress dir and project fallback.
fn boot_log(level: &str, message: &str) {
    let line = format!("[{level}] {message}");
    if let Some(egress) = egress_dir_from_env() {
        let path = egress_log_path(&egress, SANDBOX_RUNNER_LOG);
        let _ = append_line(&path, &line);
    }
    if let Some(fallback_dir) = BOOT_LOG_FALLBACK.get() {
        let path = fallback_dir.join("sandbox-runner.boot.log");
        let _ = append_line(&path, &line);
    }
}

fn boot_log_error(step: &str, err: impl std::fmt::Display) {
    boot_log("ERROR", &format!("step={step} error={err}"));
}

fn write_failure_marker(message: &str) {
    if let Some(egress) = egress_dir_from_env() {
        let path = egress_log_path(&egress, SANDBOX_RUNNER_FAILURE);
        let _ = std::fs::write(&path, message);
    }
}

fn install_sandbox_panic_hook() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "non-string panic payload".to_string()
            };
            let location = info
                .location()
                .map(|loc| format!("{}:{}", loc.file(), loc.line()))
                .unwrap_or_else(|| "unknown".to_string());
            let text = format!("panic at {location}: {message}");
            boot_log("FATAL", &text);
            write_failure_marker(&text);
        }));
    });
}

fn log_startup_environment(args: &SandboxRunnerArgs) {
    boot_log(
        "INFO",
        &format!(
            "boot env: session_id={} context_dir={} claude_binary={} grpc_listen_port={:?} egress_shim_port={:?} ready_marker={}",
            args.session_id,
            args.context_dir.display(),
            args.claude_binary,
            args.grpc_listen_port,
            args.egress_shim_port,
            args.ready_marker.display(),
        ),
    );
    for key in [
        "TDDY_SANDBOX_EGRESS_DIR",
        "TDDY_SANDBOX_SESSION_ID",
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_EGRESS_PROBE_HOST",
        "TDDY_EGRESS_PROBE_PORT",
        "TDDY_EGRESS_PROBE_URL",
        "HOME",
        "TMPDIR",
        "PATH",
    ] {
        match std::env::var(key) {
            Ok(value) if !value.is_empty() => {
                boot_log("INFO", &format!("boot env: {key}={value}"));
            }
            Ok(_) => boot_log("INFO", &format!("boot env: {key}=<empty>")),
            Err(_) => boot_log("INFO", &format!("boot env: {key}=<unset>")),
        }
    }
}

/// Initialize logging to the sandbox egress directory when `TDDY_SANDBOX_EGRESS_DIR` is set.
fn init_sandbox_egress_logging() {
    let Ok(egress) = std::env::var("TDDY_SANDBOX_EGRESS_DIR") else {
        let _ = env_logger::try_init();
        return;
    };
    let log_path = egress_log_path(std::path::Path::new(&egress), SANDBOX_RUNNER_LOG);
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    else {
        let _ = env_logger::try_init();
        return;
    };
    let _ = env_logger::Builder::from_default_env()
        .format_timestamp_secs()
        .target(env_logger::Target::Pipe(Box::new(file)))
        .try_init();
}

fn sandbox_log_line(level: &str, message: &str) {
    boot_log(level, message);
}

/// Args for `tddy-sandbox-runner` (runs inside the platform jail — darwin Seatbelt or Linux cgroups).
#[derive(Parser, Debug)]
pub struct SandboxRunnerArgs {
    #[arg(long)]
    pub session_id: String,
    #[arg(long)]
    pub context_dir: PathBuf,
    /// Working directory for the Claude process. Defaults to `context_dir`; set to a mounted
    /// project dir so the agent starts inside the repo.
    #[arg(long)]
    pub cwd: Option<PathBuf>,
    /// Vestigial: unused once `--stdio` is passed. Kept optional for callers that still pass it
    /// (e.g. `tddy-sandbox-app`'s standalone gRPC demo path).
    #[arg(long)]
    pub grpc_socket: Option<PathBuf>,
    #[arg(long)]
    pub tool_ipc_socket: PathBuf,
    /// Path to `tddy-tools` for in-jail MCP config (`--mcp` server). Required for Claude mode.
    #[arg(long)]
    pub tddy_tools_path: Option<PathBuf>,
    #[arg(long, default_value = "claude")]
    pub claude_binary: String,
    /// Agent kind for in-jail PTY: `claude` (default) or `cursor`.
    #[arg(long, default_value = "claude")]
    pub agent_kind: String,
    /// Binary path for the in-jail agent when `agent_kind` is `cursor` (default: `agent`).
    #[arg(long)]
    pub agent_binary: Option<String>,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub ready_marker: PathBuf,
    #[arg(long, default_value = "auto")]
    pub permission_mode: String,
    /// When set, pass `--append-system-prompt-file <path>` to the jailed `claude` (a managed
    /// workflow's orchestration prompt). The path must be readable inside the jail (e.g. under the
    /// context dir).
    #[arg(long)]
    pub append_system_prompt_file: Option<PathBuf>,
    /// When set, bind sandbox gRPC on this loopback port (required inside Seatbelt).
    #[arg(long)]
    pub grpc_listen_port: Option<u16>,
    /// When set, bind the in-jail egress HTTP shim on this loopback port.
    #[arg(long)]
    pub egress_shim_port: Option<u16>,
    /// AF_UNIX socket path for the gRPC `SessionChannel` (Linux cgroups path). A UDS bound on a
    /// bind-mounted path crosses the jail's network namespace, where loopback TCP cannot. When set,
    /// it takes precedence over [`grpc_listen_port`](Self::grpc_listen_port).
    #[arg(long)]
    pub grpc_uds: Option<PathBuf>,
    /// When set, spawn this command in a PTY instead of Claude (generic confined action mode).
    /// Repeat the flag once per argv token (`--pty-command=/bin/sh --pty-command=-c …`).
    #[arg(long = "pty-command", allow_hyphen_values = true)]
    pub pty_command: Vec<String>,
    /// Extra args passed verbatim to the in-jail `claude` invocation, inserted after the fixed
    /// `--model`/`--session-id`/`--permission-mode` flags and BEFORE the MCP allowlist args (whose
    /// trailing variadic `--mcp-config` would otherwise swallow a bare positional prompt). Repeat
    /// once per token (`--claude-arg=--add-dir --claude-arg=/foo`). Ignored in `--pty-command` mode.
    #[arg(long = "claude-arg", allow_hyphen_values = true)]
    pub claude_arg: Vec<String>,
    /// Extra args for the in-jail `agent` when `agent_kind=cursor`. Falls back to `claude_arg`.
    #[arg(long = "agent-arg", allow_hyphen_values = true)]
    pub agent_arg: Vec<String>,
    /// `RUST_LOG` for the in-jail `tddy-tools --mcp` server (whose logs — including specialized
    /// subagent HTTP activity — are persisted to `<egress-dir>/tddy-tools.mcp.log`). Defaults to a
    /// level that captures subagent turns. Claude-mode only.
    #[arg(long)]
    pub mcp_log_level: Option<String>,
    /// Serve `SandboxService` over stdin/stdout (RPC over stdio, see `tddy-stdio`) instead of
    /// `--grpc-uds`/`--grpc-listen-port`/`--grpc-socket`'s UDS/TCP transport.
    #[arg(long)]
    pub stdio: bool,
    /// Initial PTY width, known by the host before it even attaches (e.g. `tddy-sandbox-app`
    /// reads its own controlling terminal's size before spawning this process). Opening the PTY
    /// at this size from the start avoids a visible resize/redraw once the host's `SubscribeTerminal`
    /// arrives, and avoids ever being wrong if the host never sends a live resize afterward.
    #[arg(long, default_value_t = 80)]
    pub initial_cols: u16,
    /// Initial PTY height — see `initial_cols`.
    #[arg(long, default_value_t = 24)]
    pub initial_rows: u16,
}

struct PendingToolCall {
    response_tx: oneshot::Sender<ExecuteToolResponse>,
    request: ExecuteToolRequest,
}

struct PendingEgressCall {
    response_tx: oneshot::Sender<EgressResponse>,
    request: EgressRequest,
}

type OutboundSender = tokio::sync::mpsc::UnboundedSender<Result<SessionFrame, Status>>;

/// Host-poll session relay: MCP tool calls and egress requests queue until the host sends `HostPoll`.
#[derive(Default)]
struct SandboxSessionRelay {
    queued_tools: Mutex<VecDeque<PendingToolCall>>,
    awaiting_tool: Mutex<Option<PendingToolCall>>,
    queued_egress: Mutex<VecDeque<PendingEgressCall>>,
    awaiting_egress: Mutex<Option<PendingEgressCall>>,
    terminal_subscribed: Mutex<bool>,
    /// PTY bytes produced before the host sends `SubscribeTerminal` (broadcast drops them).
    terminal_backlog: Mutex<VecDeque<Bytes>>,
    /// Set once the pty command exits; delivered on the next `HostPoll`, after the terminal
    /// backlog is drained. See `signal_session_ended`.
    session_ended: Mutex<Option<i32>>,
    egress_seq: AtomicU64,
    /// Server-stream sender, captured when the host opens the `SessionChannel`. Tunnel frames are
    /// pushed here directly (not poll-gated) so relayed TLS bytes don't incur the `HostPoll` cadence.
    outbound: Mutex<Option<OutboundSender>>,
    /// Active CONNECT tunnels: tunnel_id → sender feeding host→jail bytes into the agent socket.
    tunnels: Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<Bytes>>>,
    /// Pending `CONNECT` opens awaiting a `TunnelOpenAck` from the host.
    tunnel_acks: Mutex<HashMap<String, oneshot::Sender<TunnelOpenAck>>>,
    tunnel_seq: AtomicU64,
}

impl SandboxSessionRelay {
    async fn call_tool(&self, tool_name: &str, args_json: &str) -> ExecuteToolResponse {
        let (tx, rx) = oneshot::channel();
        let req = ExecuteToolRequest {
            session_id: session_id_from_env(),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
            ..Default::default()
        };
        self.queued_tools
            .lock()
            .unwrap()
            .push_back(PendingToolCall {
                response_tx: tx,
                request: req,
            });

        match tokio::time::timeout(Duration::from_secs(300), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => ExecuteToolResponse {
                is_error: true,
                error_message: "tool-exec response channel closed".to_string(),
                ..Default::default()
            },
            Err(_) => ExecuteToolResponse {
                is_error: true,
                error_message: "tool-exec timed out".to_string(),
                ..Default::default()
            },
        }
    }

    fn deliver_tool_response(&self, resp: ExecuteToolResponse) {
        if let Some(call) = self.awaiting_tool.lock().unwrap().take() {
            let _ = call.response_tx.send(resp);
        }
    }

    fn deliver_egress_response(&self, resp: EgressResponse) {
        if let Some(call) = self.awaiting_egress.lock().unwrap().take() {
            let _ = call.response_tx.send(resp);
        }
    }

    fn set_outbound(&self, tx: OutboundSender) {
        *self.outbound.lock().unwrap() = Some(tx);
    }

    /// Wait until the host attaches its `SessionChannel` (the outbound sender is set). A CONNECT
    /// tunnel opened before the host attaches must wait rather than hard-fail — the agent's PTY
    /// starts before the host dials in. Returns false if the host never attaches within `timeout`.
    async fn wait_for_outbound(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.outbound.lock().unwrap().is_some() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Push a frame on the server stream immediately. Returns false if the channel is gone.
    fn push_frame(&self, payload: SessionPayload) -> bool {
        match &*self.outbound.lock().unwrap() {
            Some(tx) => tx
                .send(Ok(SessionFrame {
                    payload: Some(payload),
                }))
                .is_ok(),
            None => false,
        }
    }

    /// Register a new CONNECT tunnel: returns its id, the host→jail byte receiver, and an ack
    /// receiver resolved when the host replies `TunnelOpenAck`.
    fn register_tunnel(
        &self,
    ) -> (
        String,
        tokio::sync::mpsc::UnboundedReceiver<Bytes>,
        oneshot::Receiver<TunnelOpenAck>,
    ) {
        let tunnel_id = format!("tun-{}", self.tunnel_seq.fetch_add(1, Ordering::Relaxed));
        let (in_tx, in_rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tunnels
            .lock()
            .unwrap()
            .insert(tunnel_id.clone(), in_tx);
        self.tunnel_acks
            .lock()
            .unwrap()
            .insert(tunnel_id.clone(), ack_tx);
        (tunnel_id, in_rx, ack_rx)
    }

    fn drop_tunnel(&self, tunnel_id: &str) {
        self.tunnels.lock().unwrap().remove(tunnel_id);
        self.tunnel_acks.lock().unwrap().remove(tunnel_id);
    }

    fn deliver_tunnel_ack(&self, ack: TunnelOpenAck) {
        if let Some(tx) = self.tunnel_acks.lock().unwrap().remove(&ack.tunnel_id) {
            let _ = tx.send(ack);
        }
    }

    /// Host→jail bytes (server reply): route to the tunnel's agent-facing socket.
    fn deliver_tunnel_data(&self, data: TunnelData) {
        if let Some(tx) = self.tunnels.lock().unwrap().get(&data.tunnel_id) {
            let _ = tx.send(Bytes::from(data.data));
        }
    }

    /// Host closed its end: drop the sender so the agent-facing writer shuts down.
    fn deliver_tunnel_close(&self, close: TunnelClose) {
        self.tunnels.lock().unwrap().remove(&close.tunnel_id);
    }

    async fn call_egress(&self, method: &str, url: &str) -> EgressResponse {
        let (tx, rx) = oneshot::channel();
        let request_id = format!("egress-{}", self.egress_seq.fetch_add(1, Ordering::Relaxed));
        let request = EgressRequest {
            request_id: request_id.clone(),
            method: method.to_string(),
            url: url.to_string(),
            ..Default::default()
        };
        self.queued_egress
            .lock()
            .unwrap()
            .push_back(PendingEgressCall {
                response_tx: tx,
                request,
            });

        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => EgressResponse {
                request_id,
                error_message: "egress response channel closed".to_string(),
                ..Default::default()
            },
            Err(_) => EgressResponse {
                request_id,
                error_message: "egress timed out".to_string(),
                ..Default::default()
            },
        }
    }

    fn push_terminal(&self, chunk: Bytes) {
        if chunk.is_empty() {
            return;
        }
        self.terminal_backlog.lock().unwrap().push_back(chunk);
    }

    /// Tell the host the pty command exited, so it stops `HostPoll`-ing and lets the session
    /// channel close. Always deferred to the next `HostPoll` (`handle_host_poll` delivers it
    /// exactly once) rather than pushed immediately: an immediate push on `outbound` could race
    /// ahead of terminal output still sitting in `terminal_backlog` awaiting a poll to flush it,
    /// since the reader breaks its loop as soon as it sees `SessionEnded`.
    fn signal_session_ended(&self, exit_code: i32) {
        *self.session_ended.lock().unwrap() = Some(exit_code);
    }

    fn handle_host_poll(
        &self,
        out_tx: &tokio::sync::mpsc::UnboundedSender<Result<SessionFrame, Status>>,
    ) {
        if self.awaiting_tool.lock().unwrap().is_none() {
            if let Some(call) = self.queued_tools.lock().unwrap().pop_front() {
                let frame = SessionFrame {
                    payload: Some(SessionPayload::ToolRequest(call.request.clone())),
                };
                if out_tx.send(Ok(frame)).is_ok() {
                    *self.awaiting_tool.lock().unwrap() = Some(call);
                } else {
                    let _ = call.response_tx.send(ExecuteToolResponse {
                        is_error: true,
                        error_message: "session channel disconnected".to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        if self.awaiting_egress.lock().unwrap().is_none() {
            if let Some(call) = self.queued_egress.lock().unwrap().pop_front() {
                let frame = SessionFrame {
                    payload: Some(SessionPayload::EgressRequest(call.request.clone())),
                };
                if out_tx.send(Ok(frame)).is_ok() {
                    *self.awaiting_egress.lock().unwrap() = Some(call);
                } else {
                    let _ = call.response_tx.send(EgressResponse {
                        request_id: call.request.request_id,
                        error_message: "session channel disconnected".to_string(),
                        ..Default::default()
                    });
                }
            }
        }

        if *self.terminal_subscribed.lock().unwrap() {
            loop {
                let chunk = self.terminal_backlog.lock().unwrap().pop_front();
                let Some(chunk) = chunk else { break };
                if chunk.is_empty() {
                    continue;
                }
                let frame = SessionFrame {
                    payload: Some(SessionPayload::TerminalOutput(SessionTerminalOutput {
                        data: chunk.to_vec(),
                    })),
                };
                if out_tx.send(Ok(frame)).is_err() {
                    break;
                }
            }
        }

        // Sent last: the reader breaks its loop on `SessionEnded`, so any trailing terminal output
        // must already be queued ahead of it in this same batch, not behind it.
        if let Some(exit_code) = self.session_ended.lock().unwrap().take() {
            let _ = out_tx.send(Ok(SessionFrame {
                payload: Some(SessionPayload::SessionEnded(SessionEnded { exit_code })),
            }));
        }
    }
}

struct SandboxRunnerService {
    session_id: String,
    stdin_tx: std::sync::mpsc::Sender<Bytes>,
    relay: Arc<SandboxSessionRelay>,
}

#[tonic::async_trait]
impl TonicSandboxService for SandboxRunnerService {
    type SessionChannelStream =
        tokio_stream::wrappers::UnboundedReceiverStream<Result<SessionFrame, Status>>;

    async fn session_channel(
        &self,
        request: Request<Streaming<SessionFrame>>,
    ) -> Result<Response<Self::SessionChannelStream>, Status> {
        let mut inbound = request.into_inner();
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay = Arc::clone(&self.relay);
        // Capture the server-stream sender so the CONNECT proxy can push tunnel frames directly.
        relay.set_outbound(out_tx.clone());
        let session_id = self.session_id.clone();
        let stdin_tx = self.stdin_tx.clone();

        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                match frame.payload {
                    Some(SessionPayload::SubscribeTerminal(sub)) => {
                        if sub.session_id == session_id {
                            *relay.terminal_subscribed.lock().unwrap() = true;
                        }
                    }
                    Some(SessionPayload::TerminalInput(input)) => {
                        if !input.data.is_empty() {
                            let _ = stdin_tx.send(Bytes::from(input.data));
                        }
                    }
                    Some(SessionPayload::ToolResponse(resp)) => relay.deliver_tool_response(resp),
                    Some(SessionPayload::EgressResponse(resp)) => {
                        relay.deliver_egress_response(resp)
                    }
                    Some(SessionPayload::TunnelOpenAck(ack)) => relay.deliver_tunnel_ack(ack),
                    Some(SessionPayload::TunnelData(data)) => relay.deliver_tunnel_data(data),
                    Some(SessionPayload::TunnelClose(close)) => relay.deliver_tunnel_close(close),
                    Some(SessionPayload::HostPoll(_)) => {
                        relay.handle_host_poll(&out_tx);
                    }
                    _ => {}
                }
            }
        });

        Ok(Response::new(
            tokio_stream::wrappers::UnboundedReceiverStream::new(out_rx),
        ))
    }

    async fn echo(&self, request: Request<EchoRequest>) -> Result<Response<EchoResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(EchoResponse {
            message: req.message,
        }))
    }

    type EchoStreamStream =
        tokio_stream::wrappers::UnboundedReceiverStream<Result<EchoStreamFrame, Status>>;

    async fn echo_stream(
        &self,
        request: Request<Streaming<EchoStreamFrame>>,
    ) -> Result<Response<Self::EchoStreamStream>, Status> {
        let mut inbound = request.into_inner();
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                let _ = out_tx.send(Ok(frame));
            }
        });
        Ok(Response::new(
            tokio_stream::wrappers::UnboundedReceiverStream::new(out_rx),
        ))
    }
}

/// Converts a `tonic::Status` (this crate's tonic version, 0.12) into `tddy_rpc::Status`, by
/// hand: `tddy-rpc`'s own optional `tonic` feature pins tonic 0.11, an incompatible major version
/// with this crate's tonic 0.12, so its blanket `From<tonic::Status>` impl doesn't apply here.
fn tonic_status_to_rpc(status: Status) -> tddy_rpc::Status {
    use tddy_rpc::Code;
    let code = match status.code() {
        tonic::Code::Ok => Code::Ok,
        tonic::Code::Cancelled => Code::Cancelled,
        tonic::Code::Unknown => Code::Unknown,
        tonic::Code::InvalidArgument => Code::InvalidArgument,
        tonic::Code::DeadlineExceeded => Code::DeadlineExceeded,
        tonic::Code::NotFound => Code::NotFound,
        tonic::Code::AlreadyExists => Code::AlreadyExists,
        tonic::Code::PermissionDenied => Code::PermissionDenied,
        tonic::Code::ResourceExhausted => Code::ResourceExhausted,
        tonic::Code::FailedPrecondition => Code::FailedPrecondition,
        tonic::Code::Aborted => Code::Aborted,
        tonic::Code::OutOfRange => Code::OutOfRange,
        tonic::Code::Unimplemented => Code::Unimplemented,
        tonic::Code::Internal => Code::Internal,
        tonic::Code::Unavailable => Code::Unavailable,
        tonic::Code::DataLoss => Code::DataLoss,
        tonic::Code::Unauthenticated => Code::Unauthenticated,
    };
    tddy_rpc::Status {
        code,
        message: status.message().to_string(),
    }
}

/// Same `SandboxRunnerService`, served over `tddy-stdio` instead of tonic gRPC (`--stdio`).
/// `sandbox.proto`'s own message types are `extern_path`-linked back to this canonical
/// `proto::sandbox` module from both the tonic pass (`tonic_sandbox`) and this one (see
/// `tddy-service/build.rs`), so `SessionFrame`/`EchoRequest`/etc. are the identical Rust types
/// used by the tonic impl above — this impl mirrors that one's relay logic exactly, just wrapped
/// in `tddy_rpc::{Request, Response, Streaming, Status}` instead of `tonic::{Request, Response,
/// Status}` (duplicated, not delegated — same dual-transport pattern as every other service here).
#[async_trait::async_trait]
impl tddy_service::proto::sandbox::SandboxService for SandboxRunnerService {
    type SessionChannelStream =
        tokio_stream::wrappers::UnboundedReceiverStream<Result<SessionFrame, tddy_rpc::Status>>;

    async fn session_channel(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<SessionFrame>>,
    ) -> Result<tddy_rpc::Response<Self::SessionChannelStream>, tddy_rpc::Status> {
        let mut inbound = request.into_inner();
        // `SandboxSessionRelay`'s outbound channel is typed against `tonic::Status` (shared with
        // the tonic transport above) — convert to `tddy_rpc::Status` only at this trait's
        // boundary (`tddy-rpc`'s `tonic` feature provides the `From` impl) rather than making the
        // relay generic over the status type.
        let (out_tx, out_rx): (OutboundSender, _) = tokio::sync::mpsc::unbounded_channel();
        let relay = Arc::clone(&self.relay);
        // Capture the server-stream sender so the CONNECT proxy can push tunnel frames directly.
        relay.set_outbound(out_tx.clone());
        let session_id = self.session_id.clone();
        let stdin_tx = self.stdin_tx.clone();

        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                match frame.payload {
                    Some(SessionPayload::SubscribeTerminal(sub)) => {
                        if sub.session_id == session_id {
                            *relay.terminal_subscribed.lock().unwrap() = true;
                        }
                    }
                    Some(SessionPayload::TerminalInput(input)) => {
                        if !input.data.is_empty() {
                            let _ = stdin_tx.send(Bytes::from(input.data));
                        }
                    }
                    Some(SessionPayload::ToolResponse(resp)) => relay.deliver_tool_response(resp),
                    Some(SessionPayload::EgressResponse(resp)) => {
                        relay.deliver_egress_response(resp)
                    }
                    Some(SessionPayload::TunnelOpenAck(ack)) => relay.deliver_tunnel_ack(ack),
                    Some(SessionPayload::TunnelData(data)) => relay.deliver_tunnel_data(data),
                    Some(SessionPayload::TunnelClose(close)) => relay.deliver_tunnel_close(close),
                    Some(SessionPayload::HostPoll(_)) => {
                        relay.handle_host_poll(&out_tx);
                    }
                    _ => {}
                }
            }
        });

        // Bridge the relay's tonic::Status-typed channel to this trait's tddy_rpc::Status return
        // type — a plain forwarding task, same pattern as echo_stream's below. Converts by hand
        // rather than via tddy-rpc's optional `tonic` feature: that feature pins tonic 0.11,
        // incompatible with this crate's tonic 0.12.
        let (final_tx, final_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut out_rx = out_rx;
            while let Some(item) = out_rx.recv().await {
                if final_tx.send(item.map_err(tonic_status_to_rpc)).is_err() {
                    break;
                }
            }
        });

        Ok(tddy_rpc::Response::new(
            tokio_stream::wrappers::UnboundedReceiverStream::new(final_rx),
        ))
    }

    async fn echo(
        &self,
        request: tddy_rpc::Request<EchoRequest>,
    ) -> Result<tddy_rpc::Response<EchoResponse>, tddy_rpc::Status> {
        let req = request.into_inner();
        Ok(tddy_rpc::Response::new(EchoResponse {
            message: req.message,
        }))
    }

    type EchoStreamStream =
        tokio_stream::wrappers::UnboundedReceiverStream<Result<EchoStreamFrame, tddy_rpc::Status>>;

    async fn echo_stream(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<EchoStreamFrame>>,
    ) -> Result<tddy_rpc::Response<Self::EchoStreamStream>, tddy_rpc::Status> {
        let mut inbound = request.into_inner();
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                let _ = out_tx.send(Ok(frame));
            }
        });
        Ok(tddy_rpc::Response::new(
            tokio_stream::wrappers::UnboundedReceiverStream::new(out_rx),
        ))
    }
}

/// Resolve out-of-band secrets passed via `TDDY_SECRET_<NAME>=<file path>` entries.
///
/// For each such entry, read the file at the path, yield `(<NAME>, contents)` to be set on the
/// inner Claude PTY child only, and unlink the file so the secret does not linger in scratch. The
/// secret value never travels through `sandbox-exec` argv or the broad env list — only the file
/// path does.
pub fn resolve_secret_envs(
    vars: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, String)> {
    const PREFIX: &str = "TDDY_SECRET_";
    let mut resolved = Vec::new();
    for (key, path) in vars {
        let Some(name) = key.strip_prefix(PREFIX) else {
            continue;
        };
        match std::fs::read_to_string(path) {
            Ok(value) => {
                resolved.push((name.to_string(), value));
                let _ = std::fs::remove_file(path);
            }
            Err(e) => {
                boot_log("ERROR", &format!("secret {name}: read {path} failed: {e}"));
            }
        }
    }
    resolved
}

// ---------------------------------------------------------------------------
// Resize escape parsing
// ---------------------------------------------------------------------------

/// In-jail counterpart of `tddy_daemon::claude_cli_session::strip_resize`: strips an OSC resize
/// sequence (`\x1b]resize;{cols};{rows}\x07`) from `data`.
///
/// Returns `(Some((cols, rows)), remaining)` when found, or `(None, original)` otherwise. The
/// escape sequence is removed from the returned bytes so it is not forwarded to the PTY stdin.
fn strip_resize_escape(data: &[u8]) -> (Option<(u16, u16)>, Bytes) {
    let prefix = b"\x1b]resize;";
    let start = match (0..data.len().saturating_sub(prefix.len()))
        .find(|&i| data[i..].starts_with(prefix))
    {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let after = &data[start + prefix.len()..];
    let bel = match after.iter().position(|&b| b == 0x07) {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let inner = &after[..bel];
    let semi = match inner.iter().position(|&b| b == b';') {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let parsed = std::str::from_utf8(&inner[..semi])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .zip(
            std::str::from_utf8(&inner[semi + 1..])
                .ok()
                .and_then(|s| s.parse::<u16>().ok()),
        );
    match parsed {
        Some((cols, rows)) => {
            let end = start + prefix.len() + bel + 1;
            let mut remaining = data[..start].to_vec();
            remaining.extend_from_slice(&data[end..]);
            (Some((cols, rows)), Bytes::from(remaining))
        }
        None => (None, Bytes::copy_from_slice(data)),
    }
}

struct PtyState {
    stdin_tx: std::sync::mpsc::Sender<Bytes>,
}

fn run_generic_pty_thread(
    argv: Vec<String>,
    cwd: PathBuf,
    relay: Arc<SandboxSessionRelay>,
    stdin_rx: std::sync::mpsc::Receiver<Bytes>,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<i32> {
    boot_log(
        "INFO",
        &format!(
            "pty: openpty generic cwd={} argv={argv:?} size={initial_cols}x{initial_rows}",
            cwd.display(),
        ),
    );
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: initial_rows,
            cols: initial_cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("openpty")?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("spawn generic pty command")?;
    boot_log("INFO", "pty: generic command spawned");
    drop(pair.slave);
    let master = Arc::new(Mutex::new(pair.master));

    let master_reader = Arc::clone(&master);
    let relay_reader = Arc::clone(&relay);
    let reader_thread = std::thread::spawn(move || {
        let reader = master_reader.lock().unwrap().try_clone_reader();
        let mut r = match reader {
            Ok(r) => r,
            Err(_) => {
                boot_log("ERROR", "pty: try_clone_reader failed");
                return;
            }
        };
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut r, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    relay_reader.push_terminal(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    boot_log("ERROR", &format!("pty: stdout read failed: {e}"));
                    break;
                }
            }
        }
    });

    let master_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        let writer = master_writer.lock().unwrap().take_writer();
        let mut w = match writer {
            Ok(w) => w,
            Err(_) => {
                boot_log("ERROR", "pty: take_writer failed");
                return;
            }
        };
        while let Ok(chunk) = stdin_rx.recv() {
            let (resize, remaining) = strip_resize_escape(&chunk);
            if let Some((cols, rows)) = resize {
                let _ = master_writer.lock().unwrap().resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            if remaining.is_empty() {
                continue;
            }
            if std::io::Write::write_all(&mut w, &remaining).is_err() {
                break;
            }
            let _ = std::io::Write::flush(&mut w);
        }
    });

    let exit_code = match child.wait() {
        Ok(status) => {
            boot_log("INFO", &format!("pty: generic command exited {status}"));
            status.exit_code() as i32
        }
        Err(e) => {
            boot_log("ERROR", &format!("pty: wait failed: {e}"));
            1
        }
    };
    // Join the reader thread first: it only returns after observing EOF on the pty master, which
    // guarantees every byte the child ever wrote has already been pushed to the relay. Without
    // this, `signal_session_ended` could race ahead of trailing output still sitting in the pty's
    // kernel buffer, since `child.wait()` returning doesn't imply the reader thread has drained it.
    let _ = reader_thread.join();
    relay.signal_session_ended(exit_code);
    Ok(exit_code)
}

fn spawn_generic_pty(
    argv: Vec<String>,
    cwd: PathBuf,
    relay: Arc<SandboxSessionRelay>,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<(PtyState, std::sync::mpsc::Receiver<i32>)> {
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Bytes>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<i32>();
    let relay_thread = Arc::clone(&relay);
    std::thread::spawn(move || {
        let code = match run_generic_pty_thread(
            argv,
            cwd,
            relay_thread,
            stdin_rx,
            initial_cols,
            initial_rows,
        ) {
            Ok(code) => code,
            Err(e) => {
                boot_log_error("spawn_generic_pty", format!("{e:#}"));
                write_failure_marker(&format!("spawn_generic_pty failed: {e:#}"));
                1
            }
        };
        let _ = done_tx.send(code);
    });
    Ok((PtyState { stdin_tx }, done_rx))
}

fn run_claude_pty_thread(
    argv: Vec<String>,
    cwd: PathBuf,
    egress_shim: String,
    relay: Arc<SandboxSessionRelay>,
    stdin_rx: std::sync::mpsc::Receiver<Bytes>,
    initial_cols: u16,
    initial_rows: u16,
) -> Result<()> {
    boot_log(
        "INFO",
        &format!(
            "pty: openpty claude={} cwd={} argv={argv:?} size={initial_cols}x{initial_rows}",
            argv.first().map(String::as_str).unwrap_or("<missing>"),
            cwd.display(),
        ),
    );
    let binary = std::fs::canonicalize(&argv[0])
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| argv[0].clone());
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: initial_rows,
            cols: initial_cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("openpty")?;
    let mut cmd = CommandBuilder::new(&binary);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("TDDY_EGRESS_SHIM", &egress_shim);
    // Route the agent's outbound HTTPS through the in-jail CONNECT proxy (the egress shim).
    // claude honors HTTPS_PROXY and issues `CONNECT api.anthropic.com:443`, which the shim
    // tunnels to the host over the SessionChannel. The jail itself still has (deny network*).
    cmd.env("HTTPS_PROXY", &egress_shim);
    cmd.env("HTTP_PROXY", &egress_shim);
    cmd.env("https_proxy", &egress_shim);
    cmd.env("http_proxy", &egress_shim);
    for key in [
        "TDDY_EGRESS_PROBE_HOST",
        "TDDY_EGRESS_PROBE_PORT",
        "TDDY_EGRESS_PROBE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                cmd.env(key, value);
            }
        }
    }
    // Out-of-band secrets (e.g. CLAUDE_CODE_OAUTH_TOKEN): read from their `0600` scratch files and
    // set on the inner claude child only, so the value never appears in the sandbox-exec argv.
    let process_env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    for (name, value) in resolve_secret_envs(&process_env) {
        boot_log(
            "INFO",
            &format!("pty: injecting secret env {name} into claude child"),
        );
        cmd.env(name, value);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("spawn claude in pty")?;
    boot_log("INFO", "pty: claude spawned");
    drop(pair.slave);
    let master = Arc::new(Mutex::new(pair.master));

    let master_reader = Arc::clone(&master);
    let relay_reader = Arc::clone(&relay);
    let reader_thread = std::thread::spawn(move || {
        // Clone the reader under the lock, then release it: holding the `master` guard across the
        // read loop would deadlock the stdin writer thread below (edition 2021 keeps a temporary
        // guard in an `if let` scrutinee alive for the whole block).
        let reader = master_reader.lock().unwrap().try_clone_reader();
        let mut r = match reader {
            Ok(r) => r,
            Err(_) => {
                boot_log("ERROR", "pty: try_clone_reader failed");
                return;
            }
        };
        let mut buf = [0u8; 4096];
        loop {
            match std::io::Read::read(&mut r, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    relay_reader.push_terminal(Bytes::copy_from_slice(&buf[..n]));
                }
                Err(e) => {
                    boot_log("ERROR", &format!("pty: stdout read failed: {e}"));
                    break;
                }
            }
        }
    });

    let master_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        // Take the writer once under the lock, then release it; the write loop must not hold
        // `master` (the reader clone above needs it, and blocking writes would stall output).
        let writer = master_writer.lock().unwrap().take_writer();
        let mut w = match writer {
            Ok(w) => w,
            Err(_) => {
                boot_log("ERROR", "pty: take_writer failed");
                return;
            }
        };
        while let Ok(chunk) = stdin_rx.recv() {
            let (resize, remaining) = strip_resize_escape(&chunk);
            if let Some((cols, rows)) = resize {
                let _ = master_writer.lock().unwrap().resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            if remaining.is_empty() {
                continue;
            }
            if let Err(e) = std::io::Write::write_all(&mut w, &remaining) {
                boot_log("ERROR", &format!("pty: stdin write failed: {e}"));
                break;
            }
            if let Err(e) = std::io::Write::flush(&mut w) {
                boot_log("ERROR", &format!("pty: stdin flush failed: {e}"));
                break;
            }
        }
    });

    let exit_code = match child.wait() {
        Ok(status) => {
            boot_log("INFO", &format!("pty: claude exited {status}"));
            status.exit_code() as i32
        }
        Err(e) => {
            boot_log("ERROR", &format!("pty: wait failed: {e}"));
            1
        }
    };
    // Join the reader thread first so every byte claude wrote is flushed to the relay before we
    // announce the session end (mirrors `spawn_generic_pty`). Then signal `SessionEnded` so the
    // host relay stops `HostPoll`-ing, both stream ends drop, the in-jail gRPC server shuts down,
    // and the host app's terminal bridge exits — the sandbox must not outlive claude.
    let _ = reader_thread.join();
    relay.signal_session_ended(exit_code);
    Ok(())
}

/// Pure resolution of the effective replaced-tool set: no subagent name (absent or blank) means
/// nothing is replaced, regardless of any override — there's no subagent behind it to honor.
/// Otherwise delegates to [`tddy_discovery::subagent::resolve_replaced_tools`], so the
/// default/override contract matches the one `tddy-discovery` already specifies.
fn resolve_subagent_replaced_tools(
    subagent_name: Option<&str>,
    override_csv: Option<&str>,
) -> Vec<String> {
    match subagent_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => tddy_discovery::subagent::resolve_replaced_tools(name, override_csv),
        None => Vec::new(),
    }
}

/// Pure resolution of the effective replaced-tool set from a raw `TDDY_SUBAGENTS_JSON` payload
/// (a serialized `Vec<SpecializedAgentDef>`): parses the JSON and unions every def's own
/// `replaces` list via [`tddy_discovery::subagent::resolve_replaced_tools_for_defs`]. Absent,
/// blank, or unparseable JSON resolves to an empty set — no panic, no fallback fabrication.
fn subagents_json_replaced_tools(raw_json: Option<&str>) -> Vec<String> {
    let raw_json = match raw_json.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw_json) => raw_json,
        None => return Vec::new(),
    };
    match serde_json::from_str::<Vec<tddy_discovery::agent_def::SpecializedAgentDef>>(raw_json) {
        Ok(defs) => tddy_discovery::subagent::resolve_replaced_tools_for_defs(&defs),
        Err(_) => Vec::new(),
    }
}

/// Thin env-reading wrapper around [`resolve_subagent_replaced_tools`] and
/// [`subagents_json_replaced_tools`]: prefers the array model (`TDDY_SUBAGENTS_JSON`) when it
/// parses to a non-empty replaced-tool set, otherwise falls back to the legacy single-subagent
/// pair (`TDDY_SUBAGENT`/`TDDY_SUBAGENT_REPLACES`).
fn subagent_replaced_tools_from_env() -> Vec<String> {
    let from_json =
        subagents_json_replaced_tools(std::env::var("TDDY_SUBAGENTS_JSON").ok().as_deref());
    if !from_json.is_empty() {
        return from_json;
    }
    resolve_subagent_replaced_tools(
        std::env::var("TDDY_SUBAGENT").ok().as_deref(),
        std::env::var("TDDY_SUBAGENT_REPLACES").ok().as_deref(),
    )
}

struct SpawnClaudePtyParams<'a> {
    context_dir: &'a Path,
    /// Working directory for the Claude process (defaults to `context_dir` when no project dir is
    /// mounted into the jail).
    cwd: &'a Path,
    claude_binary: &'a str,
    model: &'a str,
    permission_mode: &'a str,
    /// When set, passed to `claude` as `--append-system-prompt-file <path>` (must be readable in
    /// the jail).
    append_system_prompt_file: Option<&'a Path>,
    session_id: &'a str,
    tddy_tools_path: &'a Path,
    egress_shim: &'a str,
    relay: Arc<SandboxSessionRelay>,
    initial_cols: u16,
    initial_rows: u16,
    /// Extra args appended verbatim after the fixed flags and MCP allowlist args (see
    /// `SandboxRunnerArgs::claude_arg`).
    claude_args: &'a [String],
    /// `RUST_LOG` for the in-jail `tddy-tools --mcp` server (see `SandboxRunnerArgs::mcp_log_level`).
    mcp_log_level: Option<&'a str>,
}

/// Default `RUST_LOG` for the in-jail `tddy-tools --mcp` server when `--mcp-log-level` is unset:
/// captures specialized-subagent turns and HTTP activity so failures land in the persisted log.
const DEFAULT_MCP_RUST_LOG: &str = "info,tddy_tools=debug,tddy_discovery=debug";

fn spawn_claude_pty(params: SpawnClaudePtyParams<'_>) -> Result<PtyState> {
    let SpawnClaudePtyParams {
        context_dir,
        cwd,
        claude_binary,
        model,
        permission_mode,
        append_system_prompt_file,
        session_id,
        tddy_tools_path,
        egress_shim,
        relay,
        initial_cols,
        initial_rows,
        claude_args,
        mcp_log_level,
    } = params;
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Bytes>();

    let mut argv = vec![claude_binary.to_string()];
    if !model.is_empty() {
        argv.push("--model".into());
        argv.push(model.to_string());
    }
    argv.push("--session-id".into());
    argv.push(session_id.to_string());
    argv.push("--permission-mode".into());
    argv.push(permission_mode.to_string());
    if let Some(path) = append_system_prompt_file {
        argv.push("--append-system-prompt-file".into());
        argv.push(path.to_string_lossy().into_owned());
    }

    // Caller-supplied pass-through args go here — after our fixed flags but BEFORE the MCP args.
    // The MCP block ends in `--mcp-config <path>`, and Claude's `--mcp-config` is variadic: a bare
    // positional (e.g. a trailing prompt) placed after it would be greedily swallowed as another
    // config path. Bounded by `--permission-mode <mode>` before and `--allowedTools` after, a
    // positional prompt here stays a positional and extra flags keep their order.
    if !claude_args.is_empty() {
        argv.extend(claude_args.iter().cloned());
        boot_log(
            "INFO",
            &format!(
                "pty: inserted {} pass-through claude arg(s) before MCP args",
                claude_args.len()
            ),
        );
    }

    let scratch_dir = claude_scratch_mcp_dir(context_dir);
    let subagent_enabled = std::env::var("TDDY_SUBAGENT")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let replaced_tools = subagent_replaced_tools_from_env();
    let replaced_refs: Vec<&str> = replaced_tools.iter().map(String::as_str).collect();

    // Persist the in-jail MCP server's logs (incl. specialized-subagent HTTP activity) to the
    // session egress dir, and set its RUST_LOG — so failures like a subagent's model-server error
    // land on disk instead of vanishing into Claude's captured MCP stderr.
    let mut mcp_env = std::collections::BTreeMap::new();
    if let Some(egress_dir) = std::env::var_os("TDDY_SANDBOX_EGRESS_DIR") {
        if !egress_dir.is_empty() {
            let log_path = Path::new(&egress_dir).join("tddy-tools.mcp.log");
            mcp_env.insert(
                "TDDY_TOOLS_LOG_FILE".to_string(),
                log_path.to_string_lossy().into_owned(),
            );
            let accounting_path = Path::new(&egress_dir).join("accounting.json");
            mcp_env.insert(
                "TDDY_TOOLS_ACCOUNTING_FILE".to_string(),
                accounting_path.to_string_lossy().into_owned(),
            );
        }
    }
    mcp_env.insert(
        "RUST_LOG".to_string(),
        mcp_log_level.unwrap_or(DEFAULT_MCP_RUST_LOG).to_string(),
    );

    append_claude_mcp_args(
        &mut argv,
        &scratch_dir,
        tddy_tools_path,
        subagent_enabled,
        &replaced_refs,
        &mcp_env,
    )
    .context("append sandbox claude MCP allowlist args")?;
    boot_log(
        "INFO",
        &format!(
            "pty: sandbox claude allowlist wired ({} tools, mcp_config scratch={})",
            tddy_sandbox::workspace_exec_tool_names().len(),
            scratch_dir.display()
        ),
    );

    let cwd = cwd.to_path_buf();
    let relay_thread = Arc::clone(&relay);
    let egress_shim = egress_shim.to_string();

    std::thread::spawn(move || {
        if let Err(e) = run_claude_pty_thread(
            argv,
            cwd,
            egress_shim,
            relay_thread,
            stdin_rx,
            initial_cols,
            initial_rows,
        ) {
            boot_log_error("spawn_claude_pty", format!("{e:#}"));
            write_failure_marker(&format!("spawn_claude_pty failed: {e:#}"));
        }
    });

    Ok(PtyState { stdin_tx })
}

struct SpawnCursorPtyParams<'a> {
    cwd: &'a Path,
    cursor_binary: &'a str,
    model: &'a str,
    tddy_tools_path: &'a Path,
    egress_shim: &'a str,
    relay: Arc<SandboxSessionRelay>,
    initial_cols: u16,
    initial_rows: u16,
    agent_args: &'a [String],
    mcp_log_level: Option<&'a str>,
}

fn spawn_cursor_pty(params: SpawnCursorPtyParams<'_>) -> Result<PtyState> {
    let SpawnCursorPtyParams {
        cwd,
        cursor_binary,
        model,
        tddy_tools_path,
        egress_shim,
        relay,
        initial_cols,
        initial_rows,
        agent_args,
        mcp_log_level,
    } = params;
    let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Bytes>();

    let mut mcp_env = std::collections::BTreeMap::new();
    if let Some(egress_dir) = std::env::var_os("TDDY_SANDBOX_EGRESS_DIR") {
        if !egress_dir.is_empty() {
            let log_path = Path::new(&egress_dir).join("tddy-tools.mcp.log");
            mcp_env.insert(
                "TDDY_TOOLS_LOG_FILE".to_string(),
                log_path.to_string_lossy().into_owned(),
            );
            let accounting_path = Path::new(&egress_dir).join("accounting.json");
            mcp_env.insert(
                "TDDY_TOOLS_ACCOUNTING_FILE".to_string(),
                accounting_path.to_string_lossy().into_owned(),
            );
        }
    }
    mcp_env.insert(
        "RUST_LOG".to_string(),
        mcp_log_level.unwrap_or(DEFAULT_MCP_RUST_LOG).to_string(),
    );

    let mcp_base = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf());
    let argv = tddy_sandbox_recipes::build_cursor_sandbox_argv(
        Path::new(cursor_binary),
        model,
        agent_args,
        &mcp_base,
        tddy_tools_path,
        &mcp_env,
    )
    .context("build sandbox cursor argv")?;

    let cwd = cwd.to_path_buf();
    let relay_thread = Arc::clone(&relay);
    let egress_shim = egress_shim.to_string();

    std::thread::spawn(move || {
        if let Err(e) = run_claude_pty_thread(
            argv,
            cwd,
            egress_shim,
            relay_thread,
            stdin_rx,
            initial_cols,
            initial_rows,
        ) {
            boot_log_error("spawn_cursor_pty", format!("{e:#}"));
            write_failure_marker(&format!("spawn_cursor_pty failed: {e:#}"));
        }
    });

    Ok(PtyState { stdin_tx })
}

async fn start_tool_ipc_server(path: PathBuf, relay: Arc<SandboxSessionRelay>) -> Result<()> {
    let _ = std::fs::remove_file(&path);
    let (ready_tx, ready_rx) = oneshot::channel();
    tokio::spawn(async move {
        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("tool ipc bind failed: {e}");
                sandbox_log_line("ERROR", &format!("tool ipc bind failed: {e}"));
                return;
            }
        };
        let _ = ready_tx.send(());
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let relay = Arc::clone(&relay);
            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(stream);
                let service = ToolExecService { relay };
                let (_client, endpoint) =
                    tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, service);
                endpoint.run().await;
            });
        }
    });
    ready_rx
        .await
        .map_err(|_| anyhow::anyhow!("tool ipc server exited before bind"))?;
    Ok(())
}

async fn start_egress_shim(relay: Arc<SandboxSessionRelay>, port: Option<u16>) -> Result<u16> {
    // Bind to the literal loopback IP, never "localhost": inside the Seatbelt jail the
    // process runs under a clean `env -i` with no resolver, so getaddrinfo("localhost")
    // fails with "nodename nor servname provided". 127.0.0.1 needs no name resolution.
    let listener = match port {
        Some(port) => tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .context("bind egress shim on fixed port")?,
        None => tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind egress shim")?,
    };
    let port = listener
        .local_addr()
        .context("egress shim local addr")?
        .port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let relay = Arc::clone(&relay);
            tokio::spawn(async move {
                handle_egress_shim_connection(stream, &relay).await;
            });
        }
    });
    Ok(port)
}

async fn handle_egress_shim_connection(
    mut stream: tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 4096];
    let Ok(n) = stream.read(&mut buf).await else {
        return;
    };
    if n == 0 {
        return;
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let first = req.lines().next().unwrap_or("").to_string();

    // HTTPS_PROXY path: `CONNECT host:port HTTP/1.1` → raw TCP tunnel relayed to the host.
    // Invariant: the client waits for `200 Connection Established` before sending tunnel bytes
    // (confirmed for claude and `curl --proxytunnel`), so this first read captures only the CONNECT
    // request — no tunnel payload is buffered here and lost before the pump in handle_connect_tunnel.
    if first.starts_with("CONNECT ") {
        if let Some((host, port)) = parse_connect_target(&first) {
            handle_connect_tunnel(stream, relay, host, port).await;
        } else {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        }
        return;
    }

    // Plain-HTTP forward-proxy path: reqwest sends an absolute-form request
    // (`METHOD http://host:port/path HTTP/1.1`) when HTTP_PROXY points here and the target is
    // http:// (only https:// uses CONNECT). This is how the specialized subagent's HTTP client
    // reaches a local model server such as Ollama. Rewrite to origin form and relay to host:port.
    if let Some((head, host, port)) = rewrite_http_proxy_request(&buf[..n]) {
        handle_http_forward(stream, relay, host, port, head).await;
        return;
    }

    if !first.starts_with("GET /probe") {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    let url = std::env::var("TDDY_EGRESS_PROBE_URL").unwrap_or_else(|_| {
        let host = std::env::var("TDDY_EGRESS_PROBE_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = std::env::var("TDDY_EGRESS_PROBE_PORT").unwrap_or_else(|_| "9".into());
        format!("http://{host}:{port}/llm")
    });
    let resp = relay.call_egress("GET", &url).await;
    let status_line = if resp.error_message.is_empty() && (200..300).contains(&resp.status_code) {
        "HTTP/1.1 200 OK"
    } else {
        "HTTP/1.1 502 Bad Gateway"
    };
    let response = format!("{status_line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
}

/// Parse `CONNECT host:port HTTP/1.1` → (host, port).
fn parse_connect_target(request_line: &str) -> Option<(String, u16)> {
    let target = request_line.split_whitespace().nth(1)?;
    let (host, port) = target.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port))
}

/// Relay a `CONNECT` tunnel: ask the host to dial the target, ack with `200 Connection
/// Established`, then pump raw bytes both ways over the `SessionChannel`. The runner never
/// dials out — the host owns the outbound socket and TLS stays end-to-end with the agent.
async fn handle_connect_tunnel(
    mut stream: tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    host: String,
    port: u16,
) {
    use tokio::io::AsyncWriteExt;

    let Some((tunnel_id, in_rx)) = open_relay_tunnel(&mut stream, relay, host, port).await else {
        return; // open_relay_tunnel already wrote a 502 to the client.
    };

    // CONNECT contract: acknowledge the tunnel, then TLS stays end-to-end (no initial payload).
    if stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .is_err()
    {
        relay.drop_tunnel(&tunnel_id);
        let _ = relay.push_frame(SessionPayload::TunnelClose(TunnelClose {
            tunnel_id,
            error: String::new(),
        }));
        return;
    }

    pump_tunnel(stream, relay, tunnel_id, in_rx, None).await;
}

/// Forward-proxy path for plain-HTTP egress (`METHOD http://host:port/path HTTP/1.1`, absolute
/// form). Unlike CONNECT there is no `200 Connection Established` — the origin's own response is the
/// reply — so the already-read, origin-form-rewritten request `head` is delivered as the first
/// upstream frame and the rest of the request body is then streamed. Used by the specialized
/// subagent's HTTP client to reach a local model server (e.g. Ollama). The host owns the real
/// outbound socket, so no jail network rule is needed.
async fn handle_http_forward(
    mut stream: tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    host: String,
    port: u16,
    head: Vec<u8>,
) {
    let Some((tunnel_id, in_rx)) = open_relay_tunnel(&mut stream, relay, host, port).await else {
        return;
    };
    pump_tunnel(stream, relay, tunnel_id, in_rx, Some(head)).await;
}

/// Open a relayed outbound tunnel to `host:port`: wait for the host to attach, register the tunnel,
/// push `TunnelOpen`, and await the ack. On any failure a `502 Bad Gateway` is written to `stream`
/// and `None` is returned (the runner never dials out itself — the host owns the outbound socket).
async fn open_relay_tunnel(
    stream: &mut tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    host: String,
    port: u16,
) -> Option<(String, tokio::sync::mpsc::UnboundedReceiver<Bytes>)> {
    use tokio::io::AsyncWriteExt;
    const BAD_GATEWAY: &[u8] =
        b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    // The agent's PTY starts before the host dials in; wait for the SessionChannel to attach so an
    // early request is relayed rather than hard-failed with 502.
    if !relay.wait_for_outbound(Duration::from_secs(10)).await {
        let _ = stream.write_all(BAD_GATEWAY).await;
        return None;
    }

    let (tunnel_id, in_rx, ack_rx) = relay.register_tunnel();
    if !relay.push_frame(SessionPayload::TunnelOpen(TunnelOpen {
        tunnel_id: tunnel_id.clone(),
        host,
        port: port as u32,
    })) {
        relay.drop_tunnel(&tunnel_id);
        let _ = stream.write_all(BAD_GATEWAY).await;
        return None;
    }

    let opened = matches!(
        tokio::time::timeout(Duration::from_secs(15), ack_rx).await,
        Ok(Ok(ack)) if ack.ok
    );
    if !opened {
        relay.drop_tunnel(&tunnel_id);
        let _ = stream.write_all(BAD_GATEWAY).await;
        return None;
    }

    Some((tunnel_id, in_rx))
}

/// Bidirectional byte pump between the agent socket and an opened relay tunnel. `initial_up`, when
/// present, is forwarded to the host as the first `TunnelData` — the HTTP-forward path uses it to
/// deliver the request head it already read off the socket before streaming the remaining bytes.
async fn pump_tunnel(
    stream: tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    tunnel_id: String,
    mut in_rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>,
    initial_up: Option<Vec<u8>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    if let Some(head) = initial_up {
        if !relay.push_frame(SessionPayload::TunnelData(TunnelData {
            tunnel_id: tunnel_id.clone(),
            data: head,
        })) {
            relay.drop_tunnel(&tunnel_id);
            return;
        }
    }

    let (mut read_half, mut write_half) = stream.into_split();

    // agent → host: read agent socket, forward as TunnelData; signal close on EOF/error.
    let id_up = tunnel_id.clone();
    let up = async move {
        let mut buf = [0u8; 16384];
        loop {
            match read_half.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if !relay.push_frame(SessionPayload::TunnelData(TunnelData {
                        tunnel_id: id_up.clone(),
                        data: buf[..n].to_vec(),
                    })) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        relay.drop_tunnel(&id_up);
        let _ = relay.push_frame(SessionPayload::TunnelClose(TunnelClose {
            tunnel_id: id_up,
            error: String::new(),
        }));
    };

    // host → agent: drain inbound bytes into the agent socket until the host closes the tunnel.
    let down = async move {
        while let Some(bytes) = in_rx.recv().await {
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
    };

    tokio::join!(up, down);
}

/// Parse an absolute-form HTTP proxy request line (`METHOD scheme://host[:port]/path HTTP/x.y`) out
/// of the already-read `raw` request bytes and rewrite it to origin form (`METHOD /path HTTP/x.y`),
/// returning the rewritten bytes together with the target `host` and `port`. Returns `None` when
/// `raw` is not an absolute-form request (e.g. `CONNECT`, an origin-form probe, or a non-`http://`
/// target) — those are handled elsewhere. Only the request line is rewritten; headers and any body
/// bytes already in `raw` are preserved verbatim so the caller can stream the remainder.
fn rewrite_http_proxy_request(raw: &[u8]) -> Option<(Vec<u8>, String, u16)> {
    let line_end = raw.windows(2).position(|w| w == b"\r\n")?;
    let request_line = std::str::from_utf8(&raw[..line_end]).ok()?;
    let mut parts = request_line.splitn(3, ' ');
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parts.next()?;

    // Only absolute-form http:// targets are forward-proxied here; https:// uses CONNECT.
    let after_scheme = target.strip_prefix("http://")?;
    let (authority, path) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, "/"),
    };
    if authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()?),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return None;
    }

    let mut head = format!("{method} {path} {version}\r\n").into_bytes();
    head.extend_from_slice(&raw[line_end + 2..]);
    Some((head, host, port))
}

/// Run the sandbox gRPC server and claude PTY until shutdown.
pub async fn run_sandbox_runner(args: SandboxRunnerArgs) -> Result<()> {
    if let Some(parent) = args.ready_marker.parent() {
        set_boot_log_fallback(parent.to_path_buf());
    }
    install_sandbox_panic_hook();
    boot_log("INFO", "boot: enter run_sandbox_runner");
    log_startup_environment(&args);

    let result = run_sandbox_runner_inner(args).await;
    if let Err(ref err) = result {
        let message = format!("{err:#}");
        boot_log_error("run_sandbox_runner", &message);
        write_failure_marker(&message);
        eprintln!("sandbox-runner failed: {message}");
    } else {
        boot_log("INFO", "boot: run_sandbox_runner finished normally");
    }
    result
}

async fn run_sandbox_runner_inner(args: SandboxRunnerArgs) -> Result<()> {
    boot_log("INFO", "boot: init_sandbox_egress_logging");
    init_sandbox_egress_logging();
    log::info!(
        target: "tddy_sandbox_runner::runner",
        "starting sandbox-runner session_id={} context_dir={} ready_marker={}",
        args.session_id,
        args.context_dir.display(),
        args.ready_marker.display(),
    );
    sandbox_log_line(
        "INFO",
        &format!(
            "sandbox-runner start session_id={} context_dir={}",
            args.session_id,
            args.context_dir.display()
        ),
    );

    boot_log("INFO", "boot: remove stale ipc socket and ready marker");
    let _ = std::fs::remove_file(&args.tool_ipc_socket);
    let _ = std::fs::remove_file(&args.ready_marker);

    let generic_pty = !args.pty_command.is_empty();

    boot_log(
        "INFO",
        &format!(
            "boot: start_egress_shim fixed_port={:?}",
            args.egress_shim_port
        ),
    );
    let relay = Arc::new(SandboxSessionRelay::default());
    let shim_port = start_egress_shim(Arc::clone(&relay), args.egress_shim_port)
        .await
        .inspect_err(|e| boot_log_error("start_egress_shim", format!("{e:#}")))?;
    let egress_shim = format!("http://127.0.0.1:{shim_port}");
    boot_log(
        "INFO",
        &format!("boot: egress shim listening on {egress_shim}"),
    );

    boot_log(
        "INFO",
        &format!(
            "boot: start_tool_ipc_server path={}",
            args.tool_ipc_socket.display()
        ),
    );
    if generic_pty {
        boot_log("INFO", "boot: skip tool ipc (generic pty mode)");
    } else {
        start_tool_ipc_server(args.tool_ipc_socket.clone(), Arc::clone(&relay))
            .await
            .inspect_err(|e| boot_log_error("start_tool_ipc_server", format!("{e:#}")))?;
        boot_log("INFO", "boot: tool ipc server ready");
    }

    let cwd = args.cwd.clone().unwrap_or_else(|| args.context_dir.clone());
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());

    let pty =
        if generic_pty {
            boot_log(
                "INFO",
                &format!("boot: spawn_generic_pty argv={:?}", args.pty_command),
            );
            let (pty_state, done_rx) = spawn_generic_pty(
                args.pty_command.clone(),
                cwd,
                Arc::clone(&relay),
                args.initial_cols,
                args.initial_rows,
            )
            .inspect_err(|e| boot_log_error("spawn_generic_pty", format!("{e:#}")))?;
            let notify = Arc::clone(&shutdown_notify);
            tokio::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || done_rx.recv()).await;
                tokio::time::sleep(Duration::from_millis(300)).await;
                notify.notify_one();
            });
            boot_log("INFO", "boot: generic pty thread spawned");
            pty_state
        } else if args.agent_kind == "cursor" {
            boot_log(
                "INFO",
                &format!(
                    "boot: spawn_cursor_pty binary={}",
                    args.agent_binary.as_deref().unwrap_or("agent")
                ),
            );
            let tddy_tools_path = args.tddy_tools_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!("tddy_tools_path is required for cursor pty mode")
            })?;
            let cursor_binary = args.agent_binary.as_deref().unwrap_or("agent");
            let agent_args = if args.agent_arg.is_empty() {
                &args.claude_arg
            } else {
                &args.agent_arg
            };
            let pty_state = spawn_cursor_pty(SpawnCursorPtyParams {
                cwd: &cwd,
                cursor_binary,
                model: &args.model,
                tddy_tools_path,
                egress_shim: &egress_shim,
                relay: Arc::clone(&relay),
                initial_cols: args.initial_cols,
                initial_rows: args.initial_rows,
                agent_args,
                mcp_log_level: args.mcp_log_level.as_deref(),
            })
            .inspect_err(|e| boot_log_error("spawn_cursor_pty", format!("{e:#}")))?;
            boot_log("INFO", "boot: cursor pty thread spawned");
            pty_state
        } else {
            boot_log(
                "INFO",
                &format!("boot: spawn_claude_pty binary={}", args.claude_binary),
            );
            let tddy_tools_path = args.tddy_tools_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!("tddy_tools_path is required for claude pty mode")
            })?;
            let pty_state = spawn_claude_pty(SpawnClaudePtyParams {
                context_dir: &args.context_dir,
                cwd: &cwd,
                claude_binary: &args.claude_binary,
                model: &args.model,
                permission_mode: &args.permission_mode,
                append_system_prompt_file: args.append_system_prompt_file.as_deref(),
                session_id: &args.session_id,
                tddy_tools_path,
                egress_shim: &egress_shim,
                relay: Arc::clone(&relay),
                initial_cols: args.initial_cols,
                initial_rows: args.initial_rows,
                claude_args: &args.claude_arg,
                mcp_log_level: args.mcp_log_level.as_deref(),
            })
            .inspect_err(|e| boot_log_error("spawn_claude_pty", format!("{e:#}")))?;
            boot_log("INFO", "boot: claude pty thread spawned");
            pty_state
        };

    let service = SandboxRunnerService {
        session_id: args.session_id.clone(),
        stdin_tx: pty.stdin_tx,
        relay,
    };

    if args.stdio {
        // --stdio dedicates this process's real stdin/stdout to RPC framing (see
        // `tddy_core::stdio_safety`) — keep stderr off the terminal but stdin/stdout live, same
        // discipline as `--stdio` on `tddy-coder`. Best-effort: no fallback dir means no terminal
        // to redirect away from in the first place.
        if let Some(fallback_dir) = BOOT_LOG_FALLBACK.get() {
            let _ = tddy_core::stdio_safety::redirect_fd_to_file(
                libc::STDERR_FILENO,
                &fallback_dir.join("sandbox-runner.stdio_stderr.log"),
            );
        }
        boot_log("INFO", "boot: serve sandbox SandboxService over stdio");
        std::fs::write(&args.ready_marker, "stdio")
            .context("write ready marker")
            .inspect_err(|e| boot_log_error("write_ready_marker", format!("{e:#}")))?;
        sandbox_log_line("INFO", "SandboxService serving over stdio");
        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_process_stdio(
            tddy_service::proto::sandbox::SandboxServiceServer::new(service),
        );
        endpoint.run().await;
        return Ok(());
    }

    // AF_UNIX control channel (Linux cgroups path): a UDS on a bind-mounted path crosses the jail's
    // network namespace, where loopback TCP cannot. The ready marker becomes a bind sentinel.
    if let Some(uds_path) = args.grpc_uds.clone() {
        boot_log(
            "INFO",
            &format!("boot: bind sandbox grpc uds={}", uds_path.display()),
        );
        let _ = std::fs::remove_file(&uds_path);
        let listener = tokio::net::UnixListener::bind(&uds_path)
            .context("bind sandbox grpc uds")
            .inspect_err(|e| boot_log_error("bind_sandbox_grpc_uds", format!("{e:#}")))?;
        std::fs::write(&args.ready_marker, "uds")
            .context("write ready marker")
            .inspect_err(|e| boot_log_error("write_ready_marker", format!("{e:#}")))?;
        boot_log(
            "INFO",
            &format!(
                "boot: ready marker written path={} (uds)",
                args.ready_marker.display()
            ),
        );
        log::info!(
            target: "tddy_sandbox_runner::runner",
            "sandbox gRPC listening on uds {} (ready_marker={})",
            uds_path.display(),
            args.ready_marker.display()
        );
        sandbox_log_line(
            "INFO",
            &format!("gRPC listening on uds {}", uds_path.display()),
        );
        boot_log("INFO", "boot: serve sandbox gRPC (uds)");
        let generic_pty_shutdown = generic_pty;
        let shutdown_notify = Arc::clone(&shutdown_notify);
        Server::builder()
            .add_service(TonicSandboxServiceServer::new(service))
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::UnixListenerStream::new(listener),
                async move {
                    if generic_pty_shutdown {
                        shutdown_notify.notified().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                },
            )
            .await
            .context("serve sandbox grpc uds")
            .inspect_err(|e| boot_log_error("serve_sandbox_grpc", format!("{e:#}")))?;
        return Ok(());
    }

    boot_log(
        "INFO",
        &format!(
            "boot: bind sandbox grpc fixed_port={:?}",
            args.grpc_listen_port
        ),
    );
    // Literal loopback IP (not "localhost") — no resolver inside the jail; see start_egress_shim.
    let listener = match args.grpc_listen_port {
        Some(port) => tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .context("bind sandbox grpc tcp on fixed port"),
        None => tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind sandbox grpc tcp"),
    }
    .inspect_err(|e| boot_log_error("bind_sandbox_grpc", format!("{e:#}")))?;
    let port = listener
        .local_addr()
        .context("grpc local addr")
        .inspect_err(|e| boot_log_error("grpc_local_addr", format!("{e:#}")))?
        .port();
    std::fs::write(&args.ready_marker, port.to_string())
        .context("write ready marker")
        .inspect_err(|e| boot_log_error("write_ready_marker", format!("{e:#}")))?;
    boot_log(
        "INFO",
        &format!(
            "boot: ready marker written path={} port={port}",
            args.ready_marker.display()
        ),
    );
    log::info!(
        target: "tddy_sandbox_runner::runner",
        "sandbox gRPC listening on localhost:{port} (ready_marker={})",
        args.ready_marker.display()
    );
    sandbox_log_line("INFO", &format!("gRPC listening on localhost:{port}"));

    boot_log("INFO", "boot: serve sandbox gRPC");
    let generic_pty_shutdown = generic_pty;
    let shutdown_notify = Arc::clone(&shutdown_notify);
    Server::builder()
        .add_service(TonicSandboxServiceServer::new(service))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            async move {
                if generic_pty_shutdown {
                    shutdown_notify.notified().await;
                } else {
                    std::future::pending::<()>().await;
                }
            },
        )
        .await
        .context("serve sandbox grpc")
        .inspect_err(|e| boot_log_error("serve_sandbox_grpc", format!("{e:#}")))?;
    Ok(())
}

/// gRPC client for the in-jail `SandboxService`, over either a TCP or an AF_UNIX `Channel`.
pub type SandboxClient = tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient<
    tonic::transport::Channel,
>;

/// Connect a tonic client to the sandbox gRPC server over an AF_UNIX socket (Linux; survives the
/// jail's network namespace). The HTTP authority is a required-but-ignored placeholder for the UDS
/// connector.
pub async fn connect_sandbox_client_uds(uds_path: &Path) -> Result<SandboxClient> {
    use hyper_util::rt::TokioIo;

    let uds_path = uds_path.to_path_buf();
    let channel = tonic::transport::Endpoint::try_from("http://127.0.0.1:50051")
        .context("build uds endpoint")?
        .connect_with_connector(tower::service_fn(move |_| {
            let uds_path = uds_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&uds_path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .context("connect sandbox grpc uds")?;
    Ok(tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient::new(channel))
}

/// Connect a tonic client to the sandbox gRPC endpoint (port read from ready marker).
pub async fn connect_sandbox_client(
    ready_marker: &Path,
) -> Result<
    tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient<
        tonic::transport::Channel,
    >,
> {
    let port_str = std::fs::read_to_string(ready_marker).context("read ready marker")?;
    let port: u16 = port_str.trim().parse().context("parse grpc port")?;
    let endpoint = format!("http://127.0.0.1:{port}");
    // Prefer 127.0.0.1 for host-side dial; server may bind localhost (same loopback on macOS).
    tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient::connect(endpoint)
        .await
        .context("connect sandbox grpc")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **session_relay_flushes_tool_request_on_host_poll**: queued MCP calls are sent to the
    /// host only after a `HostPoll` inbound frame.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_relay_flushes_tool_request_on_host_poll() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();

        let call = tokio::spawn({
            let relay = Arc::clone(&relay);
            async move { relay.call_tool("Read", r#"{"path":"README.md"}"#).await }
        });

        // When — host poll flushes the queued request. Production polls every 25ms; poll in a
        // loop here so the test doesn't race the spawned `call_tool` push (single poll could run
        // before the request is queued, leaving nothing to flush).
        let mut frame = None;
        for _ in 0..400 {
            relay.handle_host_poll(&out_tx);
            if let Ok(f) = out_rx.try_recv() {
                frame = Some(f.expect("ok frame"));
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let frame = frame.expect("tool request frame flushed within 2s");
        let req = match frame.payload {
            Some(SessionPayload::ToolRequest(req)) => req,
            _ => panic!("expected tool request frame"),
        };
        relay.deliver_tool_response(ExecuteToolResponse {
            result_json: format!(r#"{{"tool":"{}"}}"#, req.tool_name),
            is_error: false,
            ..Default::default()
        });

        // Then
        let resp = call.await.expect("call_tool task");
        assert!(!resp.is_error, "{}", resp.error_message);
        assert_eq!(resp.result_json, r#"{"tool":"Read"}"#);
    }

    #[test]
    fn parses_repeated_pty_command_flags_including_hyphen_args() {
        let args = SandboxRunnerArgs::try_parse_from([
            "tddy-sandbox-runner",
            "--session-id",
            "sess",
            "--context-dir",
            "/tmp/ctx",
            "--grpc-socket",
            "/tmp/grpc.sock",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--model",
            "",
            "--ready-marker",
            "/tmp/ready",
            "--pty-command=/bin/sh",
            "--pty-command=-c",
            "--pty-command=printf",
            "--pty-command=pty_ok",
        ])
        .expect("argv must parse");
        assert_eq!(args.pty_command, vec!["/bin/sh", "-c", "printf", "pty_ok"]);
    }

    // ─── resolve_subagent_replaced_tools ────────────────────────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criteria 15, 17)
    // Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md
    //
    // Pure resolution (no env access) so these tests never touch process-global state — the thin
    // `subagent_replaced_tools_from_env` wrapper reads `TDDY_SUBAGENT`/`TDDY_SUBAGENT_REPLACES`
    // and delegates here.

    /// No subagent name means nothing is replaced, regardless of any override — there is no
    /// subagent behind the override to honor.
    #[test]
    fn resolve_subagent_replaced_tools_is_empty_when_no_subagent_name_is_given() {
        // When
        let replaced = resolve_subagent_replaced_tools(None, Some("grep"));

        // Then
        assert_eq!(replaced, Vec::<String>::new());
    }

    /// A blank subagent name (unset env var reads as `Some("")` in some shells) is treated the
    /// same as no subagent at all.
    #[test]
    fn resolve_subagent_replaced_tools_is_empty_when_subagent_name_is_blank() {
        // When
        let replaced = resolve_subagent_replaced_tools(Some("  "), None);

        // Then
        assert_eq!(replaced, Vec::<String>::new());
    }

    /// With a known subagent name and no override, the runner falls back to that subagent's
    /// declared default — so `--specialized-agent fastcontext` alone still filters `Grep`/`Glob`
    /// from the allowlist.
    #[test]
    fn resolve_subagent_replaced_tools_falls_back_to_the_subagent_default_when_no_override_env_is_set(
    ) {
        // When
        let replaced = resolve_subagent_replaced_tools(Some("fastcontext"), None);

        // Then
        assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
    }

    /// An explicit `TDDY_SUBAGENT_REPLACES` override (comma-separated, arbitrary whitespace)
    /// wins over the subagent's declared default.
    #[test]
    fn resolve_subagent_replaced_tools_honors_a_csv_override_with_whitespace() {
        // When
        let replaced = resolve_subagent_replaced_tools(Some("fastcontext"), Some(" read , grep "));

        // Then
        assert_eq!(replaced, vec!["Read".to_string(), "Grep".to_string()]);
    }

    /// An override that is present but empty is treated as "no override" — the subagent default
    /// applies, matching `resolve_replaced_tools`'s own contract (criterion 14).
    #[test]
    fn resolve_subagent_replaced_tools_treats_an_empty_override_as_no_override() {
        // When
        let replaced = resolve_subagent_replaced_tools(Some("fastcontext"), Some(""));

        // Then
        assert_eq!(replaced, vec!["Grep".to_string(), "Glob".to_string()]);
    }

    /// An unknown token in the override is dropped, not passed through — a typo in
    /// `--subagent-replaces` must not silently produce a nonsense allowlist entry.
    #[test]
    fn resolve_subagent_replaced_tools_drops_unrecognized_override_tokens() {
        // When
        let replaced =
            resolve_subagent_replaced_tools(Some("fastcontext"), Some("grep,not-a-real-tool"));

        // Then
        assert_eq!(replaced, vec!["Grep".to_string()]);
    }

    /// Drift guard (mirrors `tddy-daemon`'s `workspace_exec_tool_names_match_tool_catalog`):
    /// `tddy_discovery::subagent`'s canonical exec-tool name table is intentionally kept local to
    /// avoid a `tddy-discovery -> tddy-sandbox` dependency, so nothing enforces that it stays in
    /// sync with `tddy_sandbox::workspace_exec_tool_names()` at the type level. Round-trip every
    /// real exec tool name through an override to catch the list silently falling behind: a tool
    /// missing from the canonical table would be dropped here instead of resolving to itself.
    #[test]
    fn every_workspace_exec_tool_name_round_trips_through_an_override() {
        // Given — the canonical exec-tool names this runner's allowlist is built from
        // When — each one is passed as a single-tool override
        // Then — it must resolve back to itself; a dropped name means the tables diverged
        for name in tddy_sandbox::workspace_exec_tool_names() {
            let replaced =
                tddy_discovery::subagent::resolve_replaced_tools("fastcontext", Some(name));
            assert_eq!(
                replaced,
                vec![name.to_string()],
                "tddy-discovery's canonical exec-tool table is missing {name} — \
                 update CANONICAL_EXEC_TOOL_NAMES in packages/tddy-discovery/src/subagent.rs"
            );
        }
    }

    // ─── subagents_json_replaced_tools ───────────────────────────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (array model)
    //
    // Pure resolution (no env access) of the `TDDY_SUBAGENTS_JSON` payload — the array-of-agents
    // counterpart to `resolve_subagent_replaced_tools`'s single-name path above.

    /// A JSON array of specialized-agent defs unions every def's own `replaces` list.
    #[test]
    fn subagents_json_replaced_tools_unions_from_json() {
        // Given — two defs, one replacing Grep+Glob, the other replacing ReadLints
        let raw_json = serde_json::json!([
            {
                "name": "fastcontext",
                "model": "some-model",
                "base_url": "http://localhost:30000",
                "tools": ["READ"],
                "max_turns": 6,
                "replaces": ["Grep", "Glob"]
            },
            {
                "name": "my-linter",
                "model": "some-model",
                "base_url": "http://localhost:30001",
                "tools": ["READ"],
                "max_turns": 6,
                "replaces": ["ReadLints"]
            }
        ])
        .to_string();

        // When
        let replaced = subagents_json_replaced_tools(Some(&raw_json));

        // Then
        assert_eq!(
            replaced,
            vec![
                "Grep".to_string(),
                "Glob".to_string(),
                "ReadLints".to_string()
            ]
        );
    }

    /// Absent or blank JSON resolves to an empty set — no panic, no fallback fabrication.
    #[test]
    fn subagents_json_replaced_tools_returns_empty_for_absent_or_blank_json() {
        assert_eq!(subagents_json_replaced_tools(None), Vec::<String>::new());
        assert_eq!(
            subagents_json_replaced_tools(Some("   ")),
            Vec::<String>::new()
        );
    }

    /// Unparseable JSON resolves to an empty set rather than panicking — a malformed
    /// `TDDY_SUBAGENTS_JSON` must degrade to "nothing replaced", not crash the sandbox runner.
    #[test]
    fn subagents_json_replaced_tools_returns_empty_for_unparseable_json() {
        assert_eq!(
            subagents_json_replaced_tools(Some("not valid json")),
            Vec::<String>::new()
        );
    }

    /// **sandbox_runner_args_parse_with_stdio_flag_and_no_grpc_socket**: once the daemon's real
    /// session lifecycle switches to stdio (see docs/dev/TODO.md), it stops passing
    /// `--grpc-socket`/`--grpc-listen-port`/`--grpc-uds` entirely — `grpc_socket` must become
    /// optional so argv built without any gRPC flags still parses.
    #[test]
    fn sandbox_runner_args_parse_with_stdio_flag_and_no_grpc_socket() {
        // Given argv with --stdio and no --grpc-socket/--grpc-listen-port/--grpc-uds at all
        let args = SandboxRunnerArgs::try_parse_from([
            "tddy-sandbox-runner",
            "--session-id",
            "sess",
            "--context-dir",
            "/tmp/ctx",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--model",
            "",
            "--ready-marker",
            "/tmp/ready",
            "--stdio",
        ])
        .expect("argv must parse without --grpc-socket when --stdio is set");

        // Then grpc_socket is absent and the stdio transport is requested
        assert_eq!(args.grpc_socket, None);
        assert!(args.stdio);
    }

    /// Without `--initial-cols`/`--initial-rows`, the PTY still opens at the same 80x24 default
    /// the hardcoded `openpty` call used before these flags existed — callers that don't know or
    /// care about the host terminal size keep today's behavior unchanged.
    #[test]
    fn sandbox_runner_args_default_initial_size_matches_the_historical_hardcoded_pty_size() {
        // Given argv with none of the new sizing flags
        let args = SandboxRunnerArgs::try_parse_from([
            "tddy-sandbox-runner",
            "--session-id",
            "sess",
            "--context-dir",
            "/tmp/ctx",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--model",
            "",
            "--ready-marker",
            "/tmp/ready",
            "--stdio",
        ])
        .expect("argv must parse without --initial-cols/--initial-rows");

        // Then the defaults match the pre-existing hardcoded openpty(PtySize{rows:24,cols:80,..})
        assert_eq!(args.initial_cols, 80);
        assert_eq!(args.initial_rows, 24);
    }

    /// A host that knows its real terminal size (e.g. `tddy-sandbox-app`, before it even spawns
    /// this process) can open the PTY at that size from the start, instead of relying entirely on
    /// a live resize to correct an initially-wrong size after the fact.
    #[test]
    fn sandbox_runner_args_parses_explicit_initial_terminal_size() {
        // Given argv with an explicit, non-default terminal size
        let args = SandboxRunnerArgs::try_parse_from([
            "tddy-sandbox-runner",
            "--session-id",
            "sess",
            "--context-dir",
            "/tmp/ctx",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--model",
            "",
            "--ready-marker",
            "/tmp/ready",
            "--stdio",
            "--initial-cols",
            "170",
            "--initial-rows",
            "57",
        ])
        .expect("argv must parse with explicit --initial-cols/--initial-rows");

        // Then the parsed values match the caller's real terminal size, not the defaults
        assert_eq!(args.initial_cols, 170);
        assert_eq!(args.initial_rows, 57);
    }

    // -----------------------------------------------------------------------
    // strip_resize_escape — in-jail counterpart of
    // `tddy_daemon::claude_cli_session::strip_resize`, applied to the sandboxed Claude CLI's PTY
    // stdin so a live host terminal resize (see docs on `tddy_sandbox_app::bridge`) can reach the
    // jail instead of only sizing the PTY once at `SubscribeTerminal` time.
    // -----------------------------------------------------------------------

    /// A well-formed `\x1b]resize;{cols};{rows}\x07` sequence with nothing else in the buffer is
    /// fully consumed: the dimensions are extracted and no bytes are forwarded to the child PTY.
    #[test]
    fn strip_resize_escape_extracts_cols_and_rows_from_a_well_formed_sequence() {
        // Given
        let data = b"\x1b]resize;170;57\x07";

        // When
        let (resize, remaining) = strip_resize_escape(data);

        // Then
        assert_eq!(resize, Some((170, 57)));
        assert!(
            remaining.is_empty(),
            "expected no bytes left after stripping the whole escape sequence, got {remaining:?}"
        );
    }

    /// The escape sequence is removed even when surrounded by real keystrokes, and the
    /// surrounding bytes are stitched back together untouched.
    #[test]
    fn strip_resize_escape_removes_the_sequence_but_preserves_surrounding_keystrokes() {
        // Given
        let mut data = b"ab".to_vec();
        data.extend_from_slice(b"\x1b]resize;80;24\x07");
        data.extend_from_slice(b"cd");

        // When
        let (resize, remaining) = strip_resize_escape(&data);

        // Then
        assert_eq!(resize, Some((80, 24)));
        assert_eq!(remaining.as_ref(), b"abcd");
    }

    /// Plain input with no resize escape sequence at all passes through byte-for-byte.
    #[test]
    fn strip_resize_escape_returns_none_and_original_bytes_when_no_escape_is_present() {
        // Given
        let data = b"hello world";

        // When
        let (resize, remaining) = strip_resize_escape(data);

        // Then
        assert_eq!(resize, None);
        assert_eq!(remaining.as_ref(), data);
    }

    /// A truncated sequence missing its BEL (`\x07`) terminator is not a valid resize request —
    /// it must not be parsed, and must not be silently swallowed from the stream either.
    #[test]
    fn strip_resize_escape_returns_none_when_the_bel_terminator_is_missing() {
        // Given
        let data = b"\x1b]resize;80;24";

        // When
        let (resize, remaining) = strip_resize_escape(data);

        // Then
        assert_eq!(resize, None);
        assert_eq!(remaining.as_ref(), data);
    }

    // ─── rewrite_http_proxy_request (egress shim plain-HTTP forward proxy) ───────────

    /// An absolute-form POST (what reqwest sends through HTTP_PROXY for an http:// target, e.g. the
    /// subagent reaching a local Ollama) is rewritten to origin form, and host:port is extracted.
    /// Headers and any already-read body bytes are preserved verbatim.
    #[test]
    fn rewrite_http_proxy_request_rewrites_absolute_post_to_origin_form() {
        // Given
        let raw = b"POST http://localhost:11434/v1/chat/completions HTTP/1.1\r\n\
                    host: localhost:11434\r\ncontent-length: 5\r\n\r\nhello";

        // When
        let (head, host, port) =
            rewrite_http_proxy_request(raw).expect("absolute-form request must rewrite");

        // Then
        assert_eq!(host, "localhost");
        assert_eq!(port, 11434);
        assert_eq!(
            String::from_utf8(head).unwrap(),
            "POST /v1/chat/completions HTTP/1.1\r\nhost: localhost:11434\r\ncontent-length: 5\r\n\r\nhello",
            "request-target must become origin-form; headers + body preserved"
        );
    }

    /// A target with no explicit port defaults to 80, and a schemeless authority root maps to `/`.
    #[test]
    fn rewrite_http_proxy_request_defaults_port_80_and_root_path() {
        // Given
        let raw = b"GET http://example.com HTTP/1.1\r\n\r\n";

        // When
        let (head, host, port) = rewrite_http_proxy_request(raw).expect("must rewrite");

        // Then
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
        assert_eq!(String::from_utf8(head).unwrap(), "GET / HTTP/1.1\r\n\r\n");
    }

    /// Non-absolute-form requests are not forward-proxy candidates: a `CONNECT` (handled as a
    /// tunnel) and an origin-form probe both return `None`.
    #[test]
    fn rewrite_http_proxy_request_ignores_connect_and_origin_form() {
        assert!(
            rewrite_http_proxy_request(b"CONNECT api.anthropic.com:443 HTTP/1.1\r\n\r\n").is_none()
        );
        assert!(rewrite_http_proxy_request(b"GET /probe HTTP/1.1\r\n\r\n").is_none());
    }

    // ─── egress shim: plain-HTTP forward proxy (end-to-end over a loopback socket) ───
    //
    // Regression cover for the subagent → local model server (Ollama) 404: the shim was
    // CONNECT-only, so a plain-HTTP absolute-form request hit the "everything else → 404" branch.

    use std::sync::Arc;

    /// A minimal stand-in for the host end of the `SessionChannel`: acks every tunnel the shim
    /// opens, records the bytes the shim relays upstream (jail → host), and — once the request head
    /// arrives — replies with a canned response and closes the tunnel. Lets a test drive the real
    /// `handle_egress_shim_connection` over a loopback socket with no real host relay.
    struct FakeHost {
        opened: Arc<std::sync::Mutex<Option<(String, u16)>>>,
        upstream: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl FakeHost {
        fn attach(relay: Arc<SandboxSessionRelay>, canned_response: Vec<u8>) -> Self {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            relay.set_outbound(tx);
            let opened = Arc::new(std::sync::Mutex::new(None));
            let upstream = Arc::new(std::sync::Mutex::new(Vec::new()));
            let opened_task = Arc::clone(&opened);
            let upstream_task = Arc::clone(&upstream);
            tokio::spawn(async move {
                let mut replied = false;
                while let Some(Ok(frame)) = rx.recv().await {
                    match frame.payload {
                        Some(SessionPayload::TunnelOpen(open)) => {
                            *opened_task.lock().unwrap() = Some((open.host, open.port as u16));
                            relay.deliver_tunnel_ack(TunnelOpenAck {
                                tunnel_id: open.tunnel_id,
                                ok: true,
                                error: String::new(),
                            });
                        }
                        Some(SessionPayload::TunnelData(data)) => {
                            upstream_task.lock().unwrap().extend_from_slice(&data.data);
                            if !replied {
                                replied = true;
                                relay.deliver_tunnel_data(TunnelData {
                                    tunnel_id: data.tunnel_id.clone(),
                                    data: canned_response.clone(),
                                });
                                relay.deliver_tunnel_close(TunnelClose {
                                    tunnel_id: data.tunnel_id,
                                    error: String::new(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
            Self { opened, upstream }
        }

        fn tunnel_target(&self) -> Option<(String, u16)> {
            self.opened.lock().unwrap().clone()
        }

        fn upstream_text(&self) -> String {
            String::from_utf8_lossy(&self.upstream.lock().unwrap()).into_owned()
        }
    }

    async fn send_to_shim(port: u16, request: &[u8]) -> Vec<u8> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect to shim");
        client.write_all(request).await.expect("send request");
        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .await
            .expect("read response");
        response
    }

    /// A plain-HTTP absolute-form request (what reqwest sends via `HTTP_PROXY` for an http:// target
    /// — the specialized subagent reaching a local Ollama) is relayed to the target host:port in
    /// origin form, and the origin's response is returned verbatim — not a shim 404.
    #[tokio::test]
    async fn egress_shim_forwards_a_plain_http_request_to_the_relay_tunnel() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let canned = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec();
        let host = FakeHost::attach(Arc::clone(&relay), canned.clone());
        let port = start_egress_shim(Arc::clone(&relay), None)
            .await
            .expect("shim binds");

        // When
        let response = send_to_shim(
            port,
            b"POST http://localhost:11434/v1/chat/completions HTTP/1.1\r\ncontent-length: 5\r\n\r\nhello",
        )
        .await;

        // Then
        assert_eq!(
            host.tunnel_target(),
            Some(("localhost".to_string(), 11434)),
            "the shim must open a relay tunnel to the target authority"
        );
        let upstream = host.upstream_text();
        assert!(
            upstream.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "the relayed request-target must be rewritten to origin form; got: {upstream:?}"
        );
        assert!(
            upstream.ends_with("hello"),
            "the request body must be forwarded through the tunnel; got: {upstream:?}"
        );
        assert_eq!(
            response, canned,
            "the client must receive the origin's response, not a shim 404"
        );
    }

    /// An unrecognized request (neither CONNECT, absolute-form http://, nor the probe) still gets a
    /// 404 — the forward-proxy path must not swallow genuinely unroutable requests.
    #[tokio::test]
    async fn egress_shim_still_returns_404_for_an_unrecognized_request() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let _host = FakeHost::attach(Arc::clone(&relay), Vec::new());
        let port = start_egress_shim(Arc::clone(&relay), None)
            .await
            .expect("shim binds");

        // When
        let response = send_to_shim(port, b"GET /nonsense HTTP/1.1\r\n\r\n").await;

        // Then
        assert!(
            String::from_utf8_lossy(&response).starts_with("HTTP/1.1 404 Not Found"),
            "an unrecognized request must 404; got: {:?}",
            String::from_utf8_lossy(&response)
        );
    }
}
