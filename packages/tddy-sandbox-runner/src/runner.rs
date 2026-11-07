//! Sandbox runner — in-jail gRPC server + PTY for the process a tool plan names + MCP tool-exec bridge.

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

use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{
    EchoRequest, EchoResponse, EchoStreamFrame, EgressRequest, EgressResponse,
    SandboxTerminalOutput, SandboxTerminalResized, SessionEnded, SessionFrame, TunnelClose,
    TunnelData, TunnelOpen, TunnelOpenAck,
};
use tddy_service::tonic_sandbox::sandbox_service_server::{
    SandboxService as TonicSandboxService, SandboxServiceServer as TonicSandboxServiceServer,
};

use tddy_sandbox::{
    append_line, egress_log_path, session_id_from_env, EgressPolicy, SANDBOX_RUNNER_FAILURE,
    SANDBOX_RUNNER_LOG,
};
// The in-jail PTY decodes the resize escape the same way every other PTY host does.
use tddy_pty::strip_resize as strip_resize_escape;

use crate::egress_policy::EgressGuard;

/// The terminal id a sandbox session's single PTY is exposed under: the reserved main terminal (the
/// daemon serves no other `terminal_id` for a sandbox session). Duplicated from
/// `tddy_terminal_rpc::MAIN_TERMINAL_ID`, which this in-jail crate does not depend on.
const MAIN_TERMINAL_ID: &str = "main";

/// Whether the in-jail tool-IPC socket relays `service`/`method` to the facilitating daemon: the
/// roster and conversation RPCs (`tddy_service::session_agents::IN_JAIL_RELAYABLE`), the
/// conversation-worktree exec tools (`tddy_tool_engine::IN_JAIL_RELAYABLE_EXEC_TOOLS`), and the
/// hook reports (`tddy_service::session_activity::IN_JAIL_RELAYABLE_HOOK_REPORTS`,
/// `#agent-tools 7/12`). Anything else is answered `not_found` in the jail.
pub fn relays_to_the_host(service: &str, method: &str) -> bool {
    tddy_service::session_agents::IN_JAIL_RELAYABLE
        .iter()
        .chain(tddy_tool_engine::IN_JAIL_RELAYABLE_EXEC_TOOLS.iter())
        .chain(tddy_service::session_activity::IN_JAIL_RELAYABLE_HOOK_REPORTS.iter())
        .any(|(s, m)| *s == service && *m == method)
}

/// Hosts `exec_tools.ExecToolService/ExecuteTool` over the tool-IPC socket, using `tddy-rpc`'s
/// length-prefixed framing instead of the old unframed single-`read()`/`write_all()` JSON
/// protocol (which silently truncated payloads that didn't arrive in one syscall).
///
/// Also forwards the roster and conversation RPCs — `StreamSessionAgents`,
/// `OpenAgentConversation`, `PromptAgentConversation`, `ResumeAgentConversation`,
/// `CancelAgentConversation` and `ReportAgentConversationState`, the six
/// `tddy_service::session_agents::IN_JAIL_RELAYABLE` names — to the
/// facilitating daemon over the `SessionChannel`, multiplexed by `request_id` so a
/// lifetime-long `StreamSessionAgents` shares the channel with the tool calls behind it. The
/// host side (`run_host_relay_with_rpc`) dispatches them to its `HostRpcHandler`, which the
/// daemon implements over its family-B surface. `ExecuteTool` still rides the
/// poll-gated `ToolRequest`/`ToolResponse` pair (one at a time, as it was built for); the new
/// RPCs ride `RpcRequest`/`RpcStreamFrame` on the outbound stream directly, the way tunnel
/// frames do, so they never block a tool call.
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
        // `ExecuteTool` rides the poll-gated `ToolRequest`/`ToolResponse` pair, unchanged — the
        // one call at a time it was built for.
        if service == tddy_tool_engine::EXEC_TOOL_SERVICE && method == "ExecuteTool" {
            let req = match ExecuteToolRequest::decode(message.payload.as_ref()) {
                Ok(r) => r,
                Err(e) => {
                    return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::invalid_argument(
                        format!("decode ExecuteToolRequest: {e}"),
                    )));
                }
            };
            let resp = self
                .relay
                .call_tool(&req.conversation_id, &req.tool_name, &req.args_json)
                .await;
            return tddy_rpc::RpcResult::Unary(Ok(resp.encode_to_vec()));
        }
        // The roster and conversation RPCs live on the facilitating daemon's
        // `session_agents.SessionAgentService`, not on the runner — forward them over the
        // `SessionChannel` as a multiplexed `RpcRequest` and hand the caller the response stream.
        // `StreamSessionAgents`, `PromptAgentConversation` and `ResumeAgentConversation` are
        // server streams (held for the turn / the process lifetime); `OpenAgentConversation` and
        // `CancelAgentConversation` are unary.
        // The runner does not know which is which, and does not need to: a unary RPC is a stream
        // that ends after one frame, and the caller drains the receiver the same way either way.
        //
        // Read from `tddy-service` rather than spelled out here, so this allowlist and the
        // coordinate the daemon actually serves cannot drift: a tuple naming a service nobody
        // answers fails *closed*, silently, at runtime. Conversation-worktree operations are
        // exec-tool RPCs with a typed request; they ride the same channel, to the same daemon.
        if !relays_to_the_host(service, method) {
            return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "unknown {service}/{method}"
            ))));
        }
        let payload = match crate::conversation_root::bind_to_this_session(
            service,
            method,
            &message.payload,
            &session_id_from_env(),
        ) {
            Ok(payload) => payload,
            Err(status) => return tddy_rpc::RpcResult::Unary(Err(status)),
        };
        let Some((request_id, mut rx)) = self.relay.call_rpc(service, method, payload).await else {
            return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unavailable(
                "the sandbox session channel is not connected to the host daemon yet",
            )));
        };
        // Adapt the unbounded receiver to the `mpsc::Receiver` shape `RpcResult::ServerStream`
        // expects. A unary RPC arrives as exactly one terminal frame; a server stream as many
        // followed by one. The caller (tddy-tools' roster stream / conversation dispatch) drains
        // it the same way either way. On early close (the caller dropped the stream), the relay's
        // registration is cancelled so a host still forwarding frames does not find a stale sender.
        let (out_tx, out_rx) = tokio::sync::mpsc::channel(16);
        let relay = Arc::clone(&self.relay);
        let request_id_for_cleanup = request_id.clone();
        let _join = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if out_tx.send(frame).await.is_err() {
                    // The caller dropped the stream — cancel the relay registration so the host's
                    // forwarder does not keep pushing frames to a dead receiver, and stop.
                    relay.cancel_rpc(&request_id_for_cleanup);
                    break;
                }
            }
        });
        tddy_rpc::RpcResult::ServerStream(Ok(out_rx))
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

pub(crate) fn boot_log_error(step: &str, err: impl std::fmt::Display) {
    boot_log("ERROR", &format!("step={step} error={err}"));
}

pub(crate) fn write_failure_marker(message: &str) {
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
            "boot env: session_id={} context_dir={} grpc_listen_port={:?} egress_shim_port={:?} ready_marker={}",
            args.session_id,
            args.context_dir.display(),
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
    /// Working directory for the confined process. Defaults to `context_dir`; set to a mounted
    /// project dir so it starts inside the repo.
    #[arg(long)]
    pub cwd: Option<PathBuf>,
    /// Vestigial: unused once `--stdio` is passed. Kept optional for callers that still pass it
    /// (e.g. `tddy-sandbox-app`'s standalone gRPC demo path).
    #[arg(long)]
    pub grpc_socket: Option<PathBuf>,
    #[arg(long)]
    pub tool_ipc_socket: PathBuf,
    /// Path to `tddy-tools` as the jail reaches it.
    #[arg(long)]
    pub tddy_tools_path: Option<PathBuf>,
    /// Vestigial: no mode reads it. Kept so launchers that still pass it keep parsing.
    // TODO(jail-agnostic): drop `--model` once the launchers stop passing it
    // (docs/dev/todo/2026-11-03-runner-model-flag-is-vestigial.md).
    #[arg(long, default_value = "")]
    pub model: String,
    #[arg(long)]
    pub ready_marker: PathBuf,
    /// When set, bind sandbox gRPC on this loopback port (required inside Seatbelt).
    #[arg(long)]
    pub grpc_listen_port: Option<u16>,
    /// When set, bind the in-jail egress HTTP shim on this loopback port.
    #[arg(long)]
    pub egress_shim_port: Option<u16>,
    /// Which hosts the shim admits, as `tddy_sandbox::EgressPolicy::to_runner_arg` spells it:
    /// `none`, `any` or `hosts:<host:port>,…` (`#agent-tools 3/12`). `any` is what every jail had
    /// before policies existed; a plan lowered from tools always passes its own.
    #[arg(long, default_value = "any")]
    pub egress_policy: String,
    /// The ids of the tools the egress policy was lowered from, named in every refusal. Repeat once
    /// per tool.
    #[arg(long = "egress-tool")]
    pub egress_tool: Vec<String>,
    /// AF_UNIX socket path for the gRPC `SessionChannel` (Linux cgroups path). A UDS bound on a
    /// bind-mounted path crosses the jail's network namespace, where loopback TCP cannot. When set,
    /// it takes precedence over [`grpc_listen_port`](Self::grpc_listen_port).
    #[arg(long)]
    pub grpc_uds: Option<PathBuf>,
    /// When set, spawn this command in a PTY (generic confined action mode).
    /// Repeat the flag once per argv token (`--pty-command=/bin/sh --pty-command=-c …`).
    #[arg(long = "pty-command", allow_hyphen_values = true)]
    pub pty_command: Vec<String>,
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

/// Sender feeding encoded RPC stream frames (or a terminal `Status`) back to the in-jail
/// `tddy-tools` caller that opened the stream. One per in-flight `request_id`.
type RpcStreamSender = tokio::sync::mpsc::UnboundedSender<Result<Vec<u8>, tddy_rpc::Status>>;

/// One entry of the terminal backlog: output, or a size change sitting between two outputs.
enum TerminalBacklogItem {
    Output(Bytes),
    Resized { cols: u16, rows: u16 },
}

/// Host-poll session relay: MCP tool calls and egress requests queue until the host sends `HostPoll`.
#[derive(Default)]
pub(crate) struct SandboxSessionRelay {
    queued_tools: Mutex<VecDeque<PendingToolCall>>,
    awaiting_tool: Mutex<Option<PendingToolCall>>,
    queued_egress: Mutex<VecDeque<PendingEgressCall>>,
    awaiting_egress: Mutex<Option<PendingEgressCall>>,
    terminal_subscribed: Mutex<bool>,
    /// PTY bytes produced before the host sends `SubscribeTerminal` (broadcast drops them).
    terminal_backlog: Mutex<VecDeque<TerminalBacklogItem>>,
    /// Set once the pty command exits; delivered on the next `HostPoll`, after the terminal
    /// backlog is drained. See `signal_session_ended`.
    session_ended: Mutex<Option<i32>>,
    egress_seq: AtomicU64,
    /// Minted `request_id`s for forwarded RPCs (`rpc-{n}`), so two concurrent calls never collide.
    rpc_seq: AtomicU64,
    /// Server-stream sender, captured when the host opens the `SessionChannel`. Tunnel frames are
    /// pushed here directly (not poll-gated) so relayed TLS bytes don't incur the `HostPoll` cadence.
    outbound: Mutex<Option<OutboundSender>>,
    /// Active CONNECT tunnels: tunnel_id → sender feeding host→jail bytes into the agent socket.
    tunnels: Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<Bytes>>>,
    /// Pending `CONNECT` opens awaiting a `TunnelOpenAck` from the host.
    tunnel_acks: Mutex<HashMap<String, oneshot::Sender<TunnelOpenAck>>>,
    tunnel_seq: AtomicU64,
    /// In-flight RPC streams the in-jail `tddy-tools` opened over the tool-IPC socket, keyed by
    /// the `request_id` the runner minted and sent in the `RpcRequest`. Inbound `RpcStreamFrame`s
    /// are routed to the matching sender; a frame with `end_of_stream` or `error` drops the entry.
    /// This is the multiplexing the poll-gated `awaiting_tool` slot cannot do: a lifetime-long
    /// `StreamSessionAgents` lives here alongside the tool calls behind it without blocking them.
    rpc_streams: Mutex<HashMap<String, RpcStreamSender>>,
}

impl SandboxSessionRelay {
    async fn call_tool(
        &self,
        conversation_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse {
        let (tx, rx) = oneshot::channel();
        let req = ExecuteToolRequest {
            session_id: session_id_from_env(),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
            conversation_id: conversation_id.to_string(),
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

    /// Drop the captured server-stream sender once the host's half of *that* channel has closed.
    /// The relay keeps a clone so tunnel frames can bypass the poll cadence; left in place after
    /// the host is gone it keeps the response stream open for good. Compared by channel so a
    /// newer attach is never torn down by an older stream ending.
    fn release_outbound(&self, closed: &OutboundSender) {
        let mut outbound = self.outbound.lock().unwrap();
        if outbound.as_ref().is_some_and(|tx| tx.same_channel(closed)) {
            *outbound = None;
        }
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

    /// Forward an RPC the in-jail `tddy-tools` issued over the tool-IPC socket to the host daemon,
    /// returning the `request_id` and the receiver the caller drains for the response stream. The
    /// `RpcRequest` is pushed on the outbound `SessionChannel` directly (not poll-gated), the way
    /// tunnel frames are, so a server stream that lives for the process lifetime — the roster
    /// stream — does not occupy the single `awaiting_tool` slot the poll path uses.
    ///
    /// Returns `None` when the host has not attached its `SessionChannel` yet, so the caller can
    /// fail the RPC with a "session channel disconnected" error rather than panicking on a dropped
    /// receiver.
    async fn call_rpc(
        &self,
        service: &str,
        method: &str,
        payload: Vec<u8>,
    ) -> Option<(
        String,
        tokio::sync::mpsc::UnboundedReceiver<Result<Vec<u8>, tddy_rpc::Status>>,
    )> {
        // CONNECT tunnels already wait for the host to attach; roster and conversation RPCs
        // must do the same — the jail's tool-IPC socket comes up before `dial_and_bridge` finishes
        // opening the stdio `SessionChannel`, and an immediate refusal looks like a broken relay.
        if self.outbound.lock().unwrap().is_none()
            && !self.wait_for_outbound(Duration::from_secs(10)).await
        {
            return None;
        }

        let request_id = format!("rpc-{}", self.rpc_seq.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.rpc_streams
            .lock()
            .unwrap()
            .insert(request_id.clone(), tx);
        let outbound = self.outbound.lock().unwrap().clone();
        let Some(outbound) = outbound else {
            // The host has not opened the SessionChannel. Drop the registration so a late
            // inbound frame does not find a stale sender, and tell the caller to fail.
            self.rpc_streams.lock().unwrap().remove(&request_id);
            return None;
        };
        let frame = SessionFrame {
            payload: Some(SessionPayload::RpcRequest(
                tddy_service::proto::sandbox::RpcRequest {
                    request_id: request_id.clone(),
                    service: service.to_string(),
                    method: method.to_string(),
                    payload,
                },
            )),
        };
        if outbound.send(Ok(frame)).is_err() {
            self.rpc_streams.lock().unwrap().remove(&request_id);
            return None;
        }
        Some((request_id, rx))
    }

    /// Route an inbound `RpcStreamFrame` (host → jail) to the in-jail caller that minted the
    /// `request_id`. A frame with `end_of_stream` or a non-empty `error` is terminal: the entry
    /// is removed after the delivery, so a later frame for the same id (a duplicate the host
    /// cannot produce, but a reconnecting stream might) finds no sender and is dropped.
    fn deliver_rpc_stream_frame(&self, frame: tddy_service::proto::sandbox::RpcStreamFrame) {
        let terminal = frame.end_of_stream || !frame.error.is_empty();
        let entry = if terminal {
            self.rpc_streams.lock().unwrap().remove(&frame.request_id)
        } else {
            self.rpc_streams
                .lock()
                .unwrap()
                .get(&frame.request_id)
                .cloned()
        };
        let Some(tx) = entry else {
            // No in-jail caller is waiting for this id — either it already terminated, or the
            // host sent a frame for an id this jail never minted. Either way, dropping it is the
            // safe thing; there is nobody to hand it to.
            return;
        };
        let result = if !frame.error.is_empty() {
            Err(tddy_rpc::Status::internal(frame.error))
        } else {
            Ok(frame.payload)
        };
        let _ = tx.send(result);
    }

    /// Drop the in-jail registration for an RPC whose caller closed its receiver early, so a
    /// host that keeps forwarding frames (the daemon's stream has not ended yet) does not find a
    /// stale sender and the entry does not live for the session's whole life. Idempotent: a
    /// terminal frame that arrives after this still finds nothing and is dropped.
    fn cancel_rpc(&self, request_id: &str) {
        self.rpc_streams.lock().unwrap().remove(request_id);
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
        self.terminal_backlog
            .lock()
            .unwrap()
            .push_back(TerminalBacklogItem::Output(chunk));
    }

    /// Report that the jail's PTY is now `cols`×`rows` — after it opens and after every resize the
    /// runner applied. Queued in the terminal backlog behind the output produced before it, so the
    /// host learns of the size at the exact point in the stream where it changed.
    fn push_terminal_size(&self, cols: u16, rows: u16) {
        self.terminal_backlog
            .lock()
            .unwrap()
            .push_back(TerminalBacklogItem::Resized { cols, rows });
    }

    /// Tell the host the pty command exited, so it stops `HostPoll`-ing and lets the session
    /// channel close. Always deferred to the next `HostPoll` (`handle_host_poll` delivers it
    /// exactly once) rather than pushed immediately: an immediate push on `outbound` could race
    /// ahead of terminal output still sitting in `terminal_backlog` awaiting a poll to flush it,
    /// since the reader breaks its loop as soon as it sees `SessionEnded`.
    fn signal_session_ended(&self, exit_code: i32) {
        *self.session_ended.lock().unwrap() = Some(exit_code);
    }

    /// Answer a host's `SubscribeTerminal` with one empty `TerminalOutput` frame, so the host knows
    /// the channel is attached in both directions. Without it a jail that has nothing to say (a
    /// `--workspace-tools` jail spawns no pty) is indistinguishable from a channel that never
    /// attached, and the host's attach wait times out. The host ignores empty chunks.
    fn acknowledge_attach(
        &self,
        out_tx: &tokio::sync::mpsc::UnboundedSender<Result<SessionFrame, Status>>,
    ) {
        let _ = out_tx.send(Ok(SessionFrame {
            payload: Some(SessionPayload::TerminalOutput(SandboxTerminalOutput {
                data: Vec::new(),
                session_id: session_id_from_env(),
                terminal_id: MAIN_TERMINAL_ID.to_string(),
            })),
        }));
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
            // Name the terminal these bytes came from, like every other producer of terminal frames:
            // an unidentified frame cannot be traced back to its terminal, and a client receiving
            // one has no way to tell it is not another terminal's output.
            let session_id = session_id_from_env();
            loop {
                let item = self.terminal_backlog.lock().unwrap().pop_front();
                let Some(item) = item else { break };
                let payload = match item {
                    TerminalBacklogItem::Output(chunk) if chunk.is_empty() => continue,
                    TerminalBacklogItem::Output(chunk) => {
                        SessionPayload::TerminalOutput(SandboxTerminalOutput {
                            data: chunk.to_vec(),
                            session_id: session_id.clone(),
                            terminal_id: MAIN_TERMINAL_ID.to_string(),
                        })
                    }
                    TerminalBacklogItem::Resized { cols, rows } => {
                        SessionPayload::TerminalResized(SandboxTerminalResized {
                            cols: u32::from(cols),
                            rows: u32::from(rows),
                            session_id: session_id.clone(),
                            terminal_id: MAIN_TERMINAL_ID.to_string(),
                        })
                    }
                };
                let frame = SessionFrame {
                    payload: Some(payload),
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
    /// Feeds the in-jail PTY's stdin. `None` in a mode that spawns no PTY (`--workspace-tools`),
    /// where terminal input has nowhere to go.
    stdin_tx: Option<std::sync::mpsc::Sender<Bytes>>,
    relay: Arc<SandboxSessionRelay>,
    /// Runs `in_jail_tool_request`s here in the jail. `Some` only for a workspace tool jail — in
    /// every other mode tool calls travel the other way, from the in-jail agent out to the host.
    in_jail_tools: Option<Arc<InJailToolExecutor>>,
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
        let in_jail_tools = self.in_jail_tools.clone();

        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                match frame.payload {
                    Some(SessionPayload::SubscribeTerminal(sub)) => {
                        if sub.session_id == session_id {
                            *relay.terminal_subscribed.lock().unwrap() = true;
                            relay.acknowledge_attach(&out_tx);
                        }
                    }
                    Some(SessionPayload::TerminalInput(input)) => {
                        if let Some(stdin_tx) = stdin_tx.as_ref() {
                            if !input.data.is_empty() {
                                let _ = stdin_tx.send(Bytes::from(input.data));
                            }
                        }
                    }
                    Some(SessionPayload::InJailToolRequest(req)) => {
                        dispatch_in_jail_tool_request(in_jail_tools.as_ref(), req, &out_tx)
                    }
                    Some(SessionPayload::ToolResponse(resp)) => relay.deliver_tool_response(resp),
                    Some(SessionPayload::EgressResponse(resp)) => {
                        relay.deliver_egress_response(resp)
                    }
                    Some(SessionPayload::TunnelOpenAck(ack)) => relay.deliver_tunnel_ack(ack),
                    Some(SessionPayload::TunnelData(data)) => relay.deliver_tunnel_data(data),
                    Some(SessionPayload::TunnelClose(close)) => relay.deliver_tunnel_close(close),
                    Some(SessionPayload::RpcStreamFrame(frame)) => {
                        relay.deliver_rpc_stream_frame(frame)
                    }
                    Some(SessionPayload::HostPoll(_)) => {
                        relay.handle_host_poll(&out_tx);
                    }
                    _ => {}
                }
            }
            // The host closed its half: stop holding this stream's sender, or the response stream
            // — and with it the server's graceful shutdown — never ends.
            relay.release_outbound(&out_tx);
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
        let in_jail_tools = self.in_jail_tools.clone();

        tokio::spawn(async move {
            while let Some(Ok(frame)) = inbound.next().await {
                match frame.payload {
                    Some(SessionPayload::SubscribeTerminal(sub)) => {
                        if sub.session_id == session_id {
                            *relay.terminal_subscribed.lock().unwrap() = true;
                            relay.acknowledge_attach(&out_tx);
                        }
                    }
                    Some(SessionPayload::TerminalInput(input)) => {
                        if let Some(stdin_tx) = stdin_tx.as_ref() {
                            if !input.data.is_empty() {
                                let _ = stdin_tx.send(Bytes::from(input.data));
                            }
                        }
                    }
                    Some(SessionPayload::InJailToolRequest(req)) => {
                        dispatch_in_jail_tool_request(in_jail_tools.as_ref(), req, &out_tx)
                    }
                    Some(SessionPayload::ToolResponse(resp)) => relay.deliver_tool_response(resp),
                    Some(SessionPayload::EgressResponse(resp)) => {
                        relay.deliver_egress_response(resp)
                    }
                    Some(SessionPayload::TunnelOpenAck(ack)) => relay.deliver_tunnel_ack(ack),
                    Some(SessionPayload::TunnelData(data)) => relay.deliver_tunnel_data(data),
                    Some(SessionPayload::TunnelClose(close)) => relay.deliver_tunnel_close(close),
                    Some(SessionPayload::RpcStreamFrame(frame)) => {
                        relay.deliver_rpc_stream_frame(frame)
                    }
                    Some(SessionPayload::HostPoll(_)) => {
                        relay.handle_host_poll(&out_tx);
                    }
                    _ => {}
                }
            }
            // The host closed its half: stop holding this stream's sender, or the response stream
            // — and with it the server's graceful shutdown — never ends.
            relay.release_outbound(&out_tx);
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

pub(crate) struct PtyState {
    pub(crate) stdin_tx: std::sync::mpsc::Sender<Bytes>,
}

/// The environment a PTY child runs with, when it is not the runner's own.
///
/// `Some` replaces the runner's environment outright (`--exec-plan`: the child sees what its plan
/// declares and nothing of the jail's control variables); `None` inherits it, as the generic
/// confined-action mode always has.
pub(crate) type ChildEnv = Option<std::collections::BTreeMap<String, String>>;

fn run_generic_pty_thread(
    argv: Vec<String>,
    cwd: PathBuf,
    child_env: ChildEnv,
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
    // Before the child can write: the host's screen model starts at the grid its output is drawn for.
    relay.push_terminal_size(initial_cols, initial_rows);
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&cwd);
    if let Some(child_env) = child_env {
        cmd.env_clear();
        for (name, value) in child_env {
            cmd.env(name, value);
        }
    }
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
    let relay_resize = Arc::clone(&relay);
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
                let applied = master_writer.lock().unwrap().resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
                // The applied size only: a resize the PTY refused must not move the host's model.
                if applied.is_ok() {
                    relay_resize.push_terminal_size(cols, rows);
                }
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

pub(crate) fn spawn_generic_pty(
    argv: Vec<String>,
    cwd: PathBuf,
    child_env: ChildEnv,
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
            child_env,
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
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    read_half,
                    write_half,
                    service,
                    tddy_rpc::RequestTransport::UnixSocket,
                );
                endpoint.run().await;
            });
        }
    });
    ready_rx
        .await
        .map_err(|_| anyhow::anyhow!("tool ipc server exited before bind"))?;
    Ok(())
}

/// Where a jailed process must point a proxy to reach the shim [`start_egress_shim`] bound.
///
/// The literal loopback IP, never `localhost`, for the same reason the shim binds it: nothing in
/// the jail can resolve a name (see `start_egress_shim`).
fn egress_shim_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Destinations a jailed process must dial directly instead of through the shim. Its own loopback
/// listeners live inside the jail; tunnelling them would hand them to the host's `127.0.0.1`,
/// which is a different machine's idea of the same address.
const EGRESS_PROXY_BYPASS: &str = "127.0.0.1,localhost";

/// The environment that points a jailed tool call's subprocesses at the CONNECT egress shim.
///
/// The jail's profile denies the network outright, so the shim is the only way out — and a build
/// only finds it if it is told to. The jail's environment comes from `tddy_sandbox::
/// scratch_runner_env`, which sets no proxy, so a `cargo` or `bun` left unconfigured would attempt
/// a direct connect and be denied. Both casings are set because tooling splits on it (`curl` and
/// `cargo` read the lowercase names, much of the Node and Go world the uppercase ones) and reading
/// only the one that was skipped is indistinguishable from having no proxy at all.
///
/// Nothing at all when no shim was asked for: the daemon's `--workspace-tools` jail holds only a
/// checkout, needs no network, and must keep the profile's flat denial rather than a proxy
/// pointing at a port on which nothing listens.
pub(crate) fn egress_proxy_env(shim_port: Option<u16>) -> Vec<(String, String)> {
    let Some(port) = shim_port else {
        return Vec::new();
    };
    let shim = egress_shim_url(port);
    ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"]
        .into_iter()
        .map(|name| (name.to_string(), shim.clone()))
        .chain(
            ["NO_PROXY", "no_proxy"]
                .into_iter()
                .map(|name| (name.to_string(), EGRESS_PROXY_BYPASS.to_string())),
        )
        .collect()
}

/// The guard the shim enforces, read from the runner's `--egress-policy` / `--egress-tool`.
fn runner_egress_guard(args: &SandboxRunnerArgs) -> Result<EgressGuard> {
    let policy = EgressPolicy::from_runner_arg(&args.egress_policy)?;
    Ok(EgressGuard::new(
        policy,
        args.session_id.clone(),
        args.egress_tool.clone(),
    ))
}

async fn start_egress_shim(
    relay: Arc<SandboxSessionRelay>,
    egress: Arc<EgressGuard>,
    port: Option<u16>,
) -> Result<u16> {
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
            let egress = Arc::clone(&egress);
            tokio::spawn(async move {
                handle_egress_shim_connection(stream, &relay, &egress).await;
            });
        }
    });
    Ok(port)
}

/// [`start_egress_shim`] for the policy the jail was started with. A `none` policy starts no shim:
/// a jail that may reach no host has no port for its PTY or its tool subprocesses to be pointed at,
/// and nothing listening that a jailed process could dial.
async fn start_egress_shim_for_policy(
    relay: Arc<SandboxSessionRelay>,
    egress: Arc<EgressGuard>,
    port: Option<u16>,
) -> Result<Option<u16>> {
    if matches!(egress.policy(), EgressPolicy::None) {
        boot_log("INFO", "boot: egress policy is none, no egress shim");
        return Ok(None);
    }
    let bound = start_egress_shim(relay, egress, port)
        .await
        .inspect_err(|e| boot_log_error("start_egress_shim", format!("{e:#}")))?;
    boot_log(
        "INFO",
        &format!("boot: egress shim listening on {}", egress_shim_url(bound)),
    );
    Ok(Some(bound))
}

async fn handle_egress_shim_connection(
    mut stream: tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    egress: &EgressGuard,
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
            handle_connect_tunnel(stream, relay, egress, host, port).await;
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
        handle_http_forward(stream, relay, egress, host, port, head).await;
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
    egress: &EgressGuard,
    host: String,
    port: u16,
) {
    use tokio::io::AsyncWriteExt;

    let Some((tunnel_id, in_rx)) = open_relay_tunnel(&mut stream, relay, egress, host, port).await
    else {
        return; // open_relay_tunnel already answered the client (403 or 502).
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
    egress: &EgressGuard,
    host: String,
    port: u16,
    head: Vec<u8>,
) {
    let Some((tunnel_id, in_rx)) = open_relay_tunnel(&mut stream, relay, egress, host, port).await
    else {
        return;
    };
    pump_tunnel(stream, relay, tunnel_id, in_rx, Some(head)).await;
}

/// Open a relayed outbound tunnel to `host:port`: check the jail's egress policy, wait for the host
/// to attach, register the tunnel, push `TunnelOpen`, and await the ack. A host the policy does not
/// admit is answered `403 Forbidden` naming it, and no `TunnelOpen` is sent; on any other failure a
/// `502 Bad Gateway` is written to `stream`. Either way `None` is returned (the runner never dials
/// out itself — the host owns the outbound socket).
async fn open_relay_tunnel(
    stream: &mut tokio::net::TcpStream,
    relay: &SandboxSessionRelay,
    egress: &EgressGuard,
    host: String,
    port: u16,
) -> Option<(String, tokio::sync::mpsc::UnboundedReceiver<Bytes>)> {
    use tokio::io::AsyncWriteExt;
    const BAD_GATEWAY: &[u8] =
        b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    if let Err(refusal) = egress.check(&host, port) {
        boot_log("WARN", &refusal.to_string());
        let _ = stream.write_all(&refusal.forbidden_response()).await;
        return None;
    }

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

/// Serve `SandboxService` over this process's real stdin/stdout until the pipe closes.
///
/// `--stdio` dedicates stdin/stdout to RPC framing (see `tddy_core::stdio_safety`) — keep stderr
/// off the terminal but stdin/stdout live, the same discipline as `--stdio` on `tddy-coder`.
/// Redirecting stderr is best-effort: no fallback dir means no terminal to redirect away from in
/// the first place.
async fn serve_over_stdio(service: SandboxRunnerService, ready_marker: &Path) -> Result<()> {
    if let Some(fallback_dir) = BOOT_LOG_FALLBACK.get() {
        let _ = tddy_core::stdio_safety::redirect_fd_to_file(
            libc::STDERR_FILENO,
            &fallback_dir.join("sandbox-runner.stdio_stderr.log"),
        );
    }
    boot_log("INFO", "boot: serve sandbox SandboxService over stdio");
    std::fs::write(ready_marker, "stdio")
        .context("write ready marker")
        .inspect_err(|e| boot_log_error("write_ready_marker", format!("{e:#}")))?;
    sandbox_log_line("INFO", "SandboxService serving over stdio");
    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_process_stdio(
        tddy_service::proto::sandbox::SandboxServiceServer::new(service),
    );
    endpoint.run().await;
    Ok(())
}

/// How a `--workspace-tools` jail is reached from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceToolsTransport {
    /// The jail's own piped stdin/stdout — what the daemon drives its workspace jails over.
    Stdio,
    /// A loopback gRPC listener on this port — what `tddy-sandbox-app` attaches over, the same
    /// transport it uses for every other session it spawns.
    LoopbackGrpc(u16),
}

/// Resolve which transport a `--workspace-tools` jail will be served over.
///
/// The mode originally required `--stdio`, because its only caller was the daemon, which pipes the
/// jail's stdio. `tddy-sandbox-app` owns no such pipe — it dials the runner's own gRPC listener —
/// so a jail it spawns is reached the way its other jails are. Neither flag is a default for the
/// other: started with no transport at all, the jail would serve nobody, and failing fast beats a
/// process that comes up and waits forever.
pub fn resolve_workspace_tools_transport(
    stdio: bool,
    grpc_listen_port: Option<u16>,
) -> Result<WorkspaceToolsTransport, String> {
    match (stdio, grpc_listen_port) {
        // Given both, stdio wins: it is the transport whose caller pipes this process, and one of
        // the two has to, since the jail can only be served over one of them.
        (true, _) => Ok(WorkspaceToolsTransport::Stdio),
        (false, Some(port)) => Ok(WorkspaceToolsTransport::LoopbackGrpc(port)),
        (false, None) => Err(
            "--workspace-tools needs a transport to be reached over: --stdio \
             for a host that pipes this jail's stdio, or --grpc-listen-port for one that dials its \
             loopback listener"
                .to_string(),
        ),
    }
}

/// Serve a sandboxed `workspace` session's tool calls (`--workspace-tools <worktree>`), and
/// nothing else.
///
/// The mirror image of every other mode here: there is no agent in this jail to host, so there is
/// no PTY and no `tddy-tools --mcp` server calling back out over the tool-IPC socket. The jail
/// exists so that the tool call itself runs under the kernel's confinement instead of on the host
/// that holds the checkout — the host asks with `in_jail_tool_request`, this answers with
/// `in_jail_tool_response`, and the worktree the engine is pointed at is the one mounted inside
/// the jail.
///
/// Where the caller also asked for `--egress-shim-port`, the jail runs the codebase's build as
/// well as reading it, so the shim is started and the tool calls' subprocesses are pointed at it
/// through [`egress_proxy_env`] — the network that reaches is still the host's socket, over the
/// same `SessionChannel` the tool calls arrive on.
async fn run_workspace_tool_runner_inner(
    args: &SandboxRunnerArgs,
    worktree: PathBuf,
) -> Result<()> {
    let transport = resolve_workspace_tools_transport(args.stdio, args.grpc_listen_port)
        .map_err(|message| anyhow::anyhow!(message))
        .inspect_err(|e| boot_log_error("resolve_workspace_tools_transport", format!("{e:#}")))?;
    init_sandbox_egress_logging();
    boot_log(
        "INFO",
        &format!(
            "boot: workspace tool mode, serving tools against {} over {transport:?}",
            worktree.display()
        ),
    );
    let _ = std::fs::remove_file(&args.ready_marker);

    let relay = Arc::new(SandboxSessionRelay::default());
    // Only a jail that runs the codebase's build needs egress, and then only through the host's
    // CONNECT relay — so the shim exists exactly when the caller asked for one. The daemon's
    // workspace jail asks for none and stays networkless.
    let shim_port = match args.egress_shim_port {
        Some(requested) => {
            let egress = Arc::new(runner_egress_guard(args)?);
            start_egress_shim_for_policy(Arc::clone(&relay), egress, Some(requested)).await?
        }
        None => None,
    };

    let service = SandboxRunnerService {
        session_id: args.session_id.clone(),
        // No PTY in this jail, so terminal input has nowhere to go.
        stdin_tx: None,
        relay,
        in_jail_tools: Some(Arc::new(InJailToolExecutor {
            worktree,
            session_id: args.session_id.clone(),
            registry: tddy_task::TaskRegistry::new(),
            tool_env: egress_proxy_env(shim_port),
        })),
    };
    match transport {
        WorkspaceToolsTransport::Stdio => serve_over_stdio(service, &args.ready_marker).await,
        // Nothing in this jail can end the session — there is no agent to exit — so it serves
        // until the host that spawned it tears it down.
        WorkspaceToolsTransport::LoopbackGrpc(port) => {
            serve_over_loopback_grpc(
                service,
                &args.ready_marker,
                Some(port),
                std::future::pending(),
            )
            .await
        }
    }
}

/// Runs one tool call inside the jail, against the worktree as mounted here.
struct InJailToolExecutor {
    worktree: PathBuf,
    /// The session this jail serves. Used in place of whatever the request names: the jail holds
    /// exactly one session's checkout, so a request naming another session is still that session's
    /// tool call, and its background jobs belong in this runner's registry under this id.
    session_id: String,
    /// Backs `Shell`'s detached (`block_until_ms = 0`) jobs for the lifetime of the jail.
    registry: tddy_task::TaskRegistry,
    /// Laid over the environment of every command the tool engine spawns: the proxy variables that
    /// point a jailed build at the egress shim, or nothing where there is no shim (see
    /// [`egress_proxy_env`]).
    ///
    /// Carried on the executor rather than exported into the runner's own process environment,
    /// because the shim belongs to this jail's tool calls and nothing else here: an overlay says
    /// exactly that, and says it without mutating global state underneath a running runtime.
    tool_env: Vec<(String, String)>,
}

impl InJailToolExecutor {
    async fn execute(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse {
        let root = match crate::conversation_root::tool_root(&self.worktree, &req.conversation_id) {
            Ok(root) => root,
            Err(message) => {
                return ExecuteToolResponse {
                    result_json: serde_json::json!({ "error": message }).to_string(),
                    is_error: true,
                    error_message: message,
                    ..Default::default()
                }
            }
        };
        let outcome = tddy_tool_engine::execute_tool_with_env(
            &root,
            &req.tool_name,
            &req.args_json,
            &self.registry,
            &self.session_id,
            &self.tool_env,
        )
        .await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}

/// Run the tool call the host sent and answer it on the outbound stream.
///
/// Spawned rather than awaited inline so a long `Shell` does not stall the frames behind it; the
/// host keeps one in-jail call outstanding at a time, which is what lets the answer carry no
/// request id.
fn dispatch_in_jail_tool_request(
    executor: Option<&Arc<InJailToolExecutor>>,
    request: ExecuteToolRequest,
    out_tx: &OutboundSender,
) {
    let out_tx = out_tx.clone();
    let Some(executor) = executor.cloned() else {
        // Every other runner mode hosts an agent whose tools the *host* runs; only a
        // `--workspace-tools` jail runs them itself. Refusing loudly beats a silent drop the host
        // would wait out as a hang.
        let _ = out_tx.send(Ok(SessionFrame {
            payload: Some(SessionPayload::InJailToolResponse(ExecuteToolResponse {
                is_error: true,
                error_message: "this sandbox runner does not serve in-jail tool calls (started \
                                without --workspace-tools)"
                    .to_string(),
                ..Default::default()
            })),
        }));
        return;
    };
    tokio::spawn(async move {
        let response = executor.execute(&request).await;
        let _ = out_tx.send(Ok(SessionFrame {
            payload: Some(SessionPayload::InJailToolResponse(response)),
        }));
    });
}

/// Boot-log wiring shared by every entry point: a fallback log directory beside the ready marker,
/// the panic hook that turns an in-jail abort into a readable failure marker, and the startup
/// environment dump.
pub(crate) fn prepare_runner_boot(entry: &str, args: &SandboxRunnerArgs) {
    if let Some(parent) = args.ready_marker.parent() {
        set_boot_log_fallback(parent.to_path_buf());
    }
    install_sandbox_panic_hook();
    boot_log("INFO", &format!("boot: enter {entry}"));
    log_startup_environment(args);
}

/// Record how a run ended. A failure is written to the failure marker the host reads back out of
/// the jail's egress, since a jailed process's stderr is nobody's terminal.
pub(crate) fn finish_runner_run(entry: &str, result: Result<()>) -> Result<()> {
    if let Err(ref err) = result {
        let message = format!("{err:#}");
        boot_log_error(entry, &message);
        write_failure_marker(&message);
        eprintln!("sandbox-runner failed: {message}");
    } else {
        boot_log("INFO", &format!("boot: {entry} finished normally"));
    }
    result
}

/// Shut the jail's server down shortly after its one process exits, once the output it wrote has
/// been delivered.
///
/// The wait for the exit is a detached thread, not a `spawn_blocking` task: the runtime waits for
/// its blocking tasks when it shuts down, so one parked on a process that is still running kept the
/// runner alive — holding the PTY open, so the process never saw a hangup — after its server had
/// already returned (an stdio host going away, or the runner's task being dropped).
fn notify_when_the_process_exits(
    done_rx: std::sync::mpsc::Receiver<i32>,
    shutdown_notify: Arc<tokio::sync::Notify>,
) {
    let (exited_tx, exited_rx) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        let _ = done_rx.recv();
        let _ = exited_tx.send(());
    });
    tokio::spawn(async move {
        let _ = exited_rx.await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        shutdown_notify.notify_one();
    });
}

/// Run the sandbox gRPC server and claude PTY until shutdown.
pub async fn run_sandbox_runner(args: SandboxRunnerArgs) -> Result<()> {
    prepare_runner_boot("run_sandbox_runner", &args);
    let result = run_sandbox_runner_inner(args, None).await;
    finish_runner_run("run_sandbox_runner", result)
}

/// Serve a sandboxed `workspace` session's tool calls against `worktree`, hosting no agent.
///
/// The counterpart to [`run_sandbox_runner`] for a jail with nothing in it but the checkout: the
/// host sends `in_jail_tool_request` over the `SessionChannel` and this answers
/// `in_jail_tool_response`, so the confinement around a tool call is the kernel's rather than the
/// tool engine's path checks (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox).
pub async fn run_workspace_tool_runner(args: SandboxRunnerArgs, worktree: PathBuf) -> Result<()> {
    prepare_runner_boot("run_workspace_tool_runner", &args);
    let result = run_workspace_tool_runner_inner(&args, worktree).await;
    finish_runner_run("run_workspace_tool_runner", result)
}

/// The runner's shared body. With `exec_plan`, the PTY runs the plan's argv and env
/// ([`crate::exec_plan::run_exec_plan`]) instead of one of the agent modes' own; everything around
/// it — the egress shim, the tool-IPC server, the `SessionChannel` — is the same.
pub(crate) async fn run_sandbox_runner_inner(
    args: SandboxRunnerArgs,
    exec_plan: Option<crate::exec_plan::ExecPlan>,
) -> Result<()> {
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
    // A jail that ends with its one process: the confined-action mode and an exec plan.
    let ends_with_its_process = generic_pty || exec_plan.is_some();

    boot_log(
        "INFO",
        &format!(
            "boot: start_egress_shim fixed_port={:?}",
            args.egress_shim_port
        ),
    );
    let relay = Arc::new(SandboxSessionRelay::default());
    let egress = Arc::new(runner_egress_guard(&args)?);
    // A `none` policy starts no shim, and the PTY below is then given no proxy environment.
    let egress_shim_port =
        start_egress_shim_for_policy(Arc::clone(&relay), egress, args.egress_shim_port).await?;

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

    let pty = if let Some(plan) = exec_plan {
        boot_log(
            "INFO",
            &format!("boot: spawn exec plan argv={:?}", plan.argv),
        );
        let (pty_state, done_rx) = crate::exec_plan::spawn_exec_plan_pty(
            &plan,
            cwd,
            egress_shim_port,
            Arc::clone(&relay),
            args.initial_cols,
            args.initial_rows,
        )
        .inspect_err(|e| boot_log_error("spawn_exec_plan_pty", format!("{e:#}")))?;
        notify_when_the_process_exits(done_rx, Arc::clone(&shutdown_notify));
        pty_state
    } else if generic_pty {
        boot_log(
            "INFO",
            &format!("boot: spawn_generic_pty argv={:?}", args.pty_command),
        );
        let (pty_state, done_rx) = spawn_generic_pty(
            args.pty_command.clone(),
            cwd,
            None,
            Arc::clone(&relay),
            args.initial_cols,
            args.initial_rows,
        )
        .inspect_err(|e| boot_log_error("spawn_generic_pty", format!("{e:#}")))?;
        notify_when_the_process_exits(done_rx, Arc::clone(&shutdown_notify));
        boot_log("INFO", "boot: generic pty thread spawned");
        pty_state
    } else {
        // No `--exec-plan` and no `--pty-command`: nothing to run, and no agent to fall back to.
        anyhow::bail!(
            "nothing to run: pass --exec-plan <file>, --pty-command, or --workspace-tools <dir>"
        );
    };

    let service = SandboxRunnerService {
        session_id: args.session_id.clone(),
        stdin_tx: Some(pty.stdin_tx),
        relay,
        in_jail_tools: None,
    };

    if args.stdio {
        return serve_over_stdio(service, &args.ready_marker).await;
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
        let generic_pty_shutdown = ends_with_its_process;
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

    let generic_pty_shutdown = ends_with_its_process;
    let shutdown_notify = Arc::clone(&shutdown_notify);
    serve_over_loopback_grpc(
        service,
        &args.ready_marker,
        args.grpc_listen_port,
        async move {
            if generic_pty_shutdown {
                shutdown_notify.notified().await;
            } else {
                std::future::pending::<()>().await;
            }
        },
    )
    .await
}

/// Bind the runner's loopback gRPC listener, publish its port through the ready marker, and serve
/// `service` on it until `shutdown` completes.
///
/// The one loopback-TCP server in this crate: a jail hosting an agent and a `--workspace-tools`
/// jail are both reached over it, and the host learns the port the same way in either case — by
/// reading the marker written here (see [`connect_sandbox_client`]).
async fn serve_over_loopback_grpc(
    service: SandboxRunnerService,
    ready_marker: &Path,
    listen_port: Option<u16>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    boot_log(
        "INFO",
        &format!("boot: bind sandbox grpc fixed_port={listen_port:?}"),
    );
    // Literal loopback IP (not "localhost") — no resolver inside the jail; see start_egress_shim.
    let listener = match listen_port {
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
    std::fs::write(ready_marker, port.to_string())
        .context("write ready marker")
        .inspect_err(|e| boot_log_error("write_ready_marker", format!("{e:#}")))?;
    boot_log(
        "INFO",
        &format!(
            "boot: ready marker written path={} port={port}",
            ready_marker.display()
        ),
    );
    log::info!(
        target: "tddy_sandbox_runner::runner",
        "sandbox gRPC listening on localhost:{port} (ready_marker={})",
        ready_marker.display()
    );
    sandbox_log_line("INFO", &format!("gRPC listening on localhost:{port}"));

    boot_log("INFO", "boot: serve sandbox gRPC");
    Server::builder()
        .add_service(TonicSandboxServiceServer::new(service))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            shutdown,
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

/// Build a tonic [`Channel`](tonic::transport::Channel) connected over an AF_UNIX socket (Linux;
/// survives a jail/daemon network namespace). The HTTP authority is a required-but-ignored
/// placeholder for the UDS connector. Reused by any AF_UNIX tonic client (the in-jail
/// `SandboxService`, the daemon `ConnectionService`) so the connector pattern lives in one place.
pub async fn connect_uds_channel(uds_path: &Path) -> Result<tonic::transport::Channel> {
    use hyper_util::rt::TokioIo;

    let uds_path = uds_path.to_path_buf();
    tonic::transport::Endpoint::try_from("http://127.0.0.1:50051")
        .context("build uds endpoint")?
        .connect_with_connector(tower::service_fn(move |_| {
            let uds_path = uds_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&uds_path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .context("connect grpc uds")
}

/// Connect a tonic client to the sandbox gRPC server over an AF_UNIX socket (Linux; survives the
/// jail's network namespace).
pub async fn connect_sandbox_client_uds(uds_path: &Path) -> Result<SandboxClient> {
    let channel = connect_uds_channel(uds_path).await?;
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

    // ─── Which transport serves a --workspace-tools jail ────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criterion 4)
    // Changeset: docs/dev/changesets/2026-09-05-sandboxed-codebase-mode-the-jail-holds-the-code.md

    /// The daemon's workspace jails keep today's transport: it pipes the jail's stdio and drives
    /// it over that.
    #[test]
    fn a_workspace_tools_jail_started_with_stdio_is_served_over_its_piped_stdio() {
        // Given / When
        let transport = resolve_workspace_tools_transport(true, None)
            .expect("--stdio must remain a valid transport for a workspace-tools jail");

        // Then
        assert_eq!(transport, WorkspaceToolsTransport::Stdio);
    }

    /// The standalone app owns no pipe into the jail, so it dials the runner's own gRPC listener —
    /// the same transport it attaches to every other jail it spawns over.
    #[test]
    fn a_workspace_tools_jail_started_with_a_grpc_port_is_served_over_loopback_grpc() {
        // Given / When
        let transport = resolve_workspace_tools_transport(false, Some(45_501))
            .expect("a loopback gRPC port must be a valid transport");

        // Then
        assert_eq!(transport, WorkspaceToolsTransport::LoopbackGrpc(45_501));
    }

    /// Given both, stdio wins: it is the transport whose caller pipes the process, and a jail
    /// cannot be served over two.
    #[test]
    fn a_workspace_tools_jail_given_both_transports_is_served_over_stdio() {
        // Given / When
        let transport = resolve_workspace_tools_transport(true, Some(45_501))
            .expect("both flags together must resolve rather than fail");

        // Then
        assert_eq!(transport, WorkspaceToolsTransport::Stdio);
    }

    /// With no transport the jail would come up serving nobody. Failing at startup names the
    /// missing flag; waiting forever names nothing.
    #[test]
    fn a_workspace_tools_jail_with_no_transport_fails_fast_and_names_both_flags() {
        // Given / When
        let message = resolve_workspace_tools_transport(false, None)
            .expect_err("a jail with no transport must be refused");

        // Then
        assert!(
            message.contains("--stdio") && message.contains("--grpc-listen-port"),
            "the refusal must name both transports; message was: {message}"
        );
    }

    // ─── Egress for a --workspace-tools jail that also runs the build ───────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (§ Egress)
    // Changeset: docs/dev/changesets/2026-09-05-sandboxed-codebase-mode-the-jail-holds-the-code.md

    /// The shim as the jail must address it: `localhost` would fail to resolve inside it.
    const SHIM_URL: &str = "http://127.0.0.1:45601";

    /// A jailed `cargo` or `bun` reaches crates.io and npm only through the in-jail CONNECT shim,
    /// and only if it is told where the shim is — the jail's environment sets no proxy of its own,
    /// so an unconfigured build connects directly and the profile denies it.
    #[test]
    fn a_jail_given_an_egress_shim_points_its_tool_subprocesses_at_it_in_both_casings() {
        // Given / When
        let proxy_env = egress_proxy_env(Some(45_601));

        // Then
        assert_eq!(
            proxy_env,
            vec![
                ("HTTPS_PROXY".to_string(), SHIM_URL.to_string()),
                ("HTTP_PROXY".to_string(), SHIM_URL.to_string()),
                ("https_proxy".to_string(), SHIM_URL.to_string()),
                ("http_proxy".to_string(), SHIM_URL.to_string()),
                ("NO_PROXY".to_string(), "127.0.0.1,localhost".to_string()),
                ("no_proxy".to_string(), "127.0.0.1,localhost".to_string()),
            ],
            "the shim must be named by its literal loopback address — the jail has no resolver — \
             in both casings, with loopback kept off the proxy"
        );
    }

    /// The daemon's workspace jail holds a checkout and nothing else, asks for no shim, and must
    /// stay networkless — a proxy pointing at a port nothing listens on would turn the profile's
    /// flat denial into a connection refused from somewhere else entirely.
    #[test]
    fn a_jail_with_no_egress_shim_leaves_its_tool_subprocesses_without_a_proxy() {
        // Given / When
        let proxy_env = egress_proxy_env(None);

        // Then
        assert_eq!(proxy_env, Vec::<(String, String)>::new());
    }

    // ─── What a jail that serves no in-jail tools answers ──────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criterion 6)
    // Changeset: docs/dev/changesets/2026-09-05-sandboxed-codebase-mode-the-jail-holds-the-code.md

    /// Only a `--workspace-tools` jail executes tool calls itself; every other runner mode hosts an
    /// agent whose tools the *host* runs, so a jail started without the flag has nothing to hand
    /// the request to. The host keeps one in-jail call outstanding at a time and the answer carries
    /// no request id, so a silently dropped request is not a failed tool call — it is a host
    /// waiting out its whole timeout for an answer that was never going to come. Answering
    /// `is_error` is what turns it back into a failed call.
    #[test]
    fn a_jail_that_serves_no_in_jail_tools_refuses_the_call_rather_than_dropping_it() {
        // Given — a runner with no in-jail executor, which is how every agent-hosting mode runs
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();

        // When
        dispatch_in_jail_tool_request(
            None,
            ExecuteToolRequest {
                tool_name: "Read".to_string(),
                args_json: r#"{"path":"README.md"}"#.to_string(),
                ..Default::default()
            },
            &out_tx,
        );

        // Then
        let frame = out_rx
            .try_recv()
            .expect("the refusal must be on the outbound stream before the call returns")
            .expect("the refusal travels as an ok frame carrying an is_error response");
        let response = match frame.payload {
            Some(SessionPayload::InJailToolResponse(response)) => response,
            other => panic!("expected an in-jail tool response, got {other:?}"),
        };
        assert!(
            response.is_error,
            "a call this jail cannot serve must be answered as an error"
        );
        assert!(
            response.error_message.contains("--workspace-tools"),
            "the refusal must name the flag whose absence caused it, or an operator is left \
             guessing why their tools do nothing; message was: {}",
            response.error_message
        );
    }

    /// **session_relay_flushes_tool_request_on_host_poll**: queued MCP calls are sent to the
    /// host only after a `HostPoll` inbound frame.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_relay_flushes_tool_request_on_host_poll() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();

        let call = tokio::spawn({
            let relay = Arc::clone(&relay);
            async move { relay.call_tool("", "Read", r#"{"path":"README.md"}"#).await }
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

    /// A subagent conversation's call leaves the jail still naming its conversation: the host runs
    /// it in that conversation's worktree, and a request that lost the id would silently run in the
    /// caller's own.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_jail_relay_keeps_the_conversation_of_a_tool_call() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();
        let _call = tokio::spawn({
            let relay = Arc::clone(&relay);
            async move {
                relay
                    .call_tool("explore", "Write", r#"{"path":"a.txt","contents":"a"}"#)
                    .await
            }
        });

        // When — polled in a loop so the test does not race the spawned push (see above)
        let mut frame = None;
        for _ in 0..400 {
            relay.handle_host_poll(&out_tx);
            if let Ok(f) = out_rx.try_recv() {
                frame = Some(f.expect("ok frame"));
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        // Then
        let Some(SessionPayload::ToolRequest(request)) =
            frame.expect("tool request frame flushed within 2s").payload
        else {
            panic!("expected a tool request frame");
        };
        assert_eq!(request.conversation_id, "explore");
    }

    /// The runner reads no model (`#agent-tools 24/28`): what the confined agent runs on is its
    /// exec plan's argv, so a launcher still passing `--model` is refused rather than ignored.
    #[test]
    fn the_runner_refuses_a_model_flag() {
        // Given
        let argv = [
            "tddy-sandbox-runner",
            "--session-id",
            "sess",
            "--context-dir",
            "/tmp/ctx",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--ready-marker",
            "/tmp/ready",
            "--stdio",
            "--model",
            "opus",
        ];

        // When
        let parsed = SandboxRunnerArgs::try_parse_from(argv);

        // Then
        let refusal = parsed.expect_err("the runner has no --model flag");
        assert_eq!(
            refusal.kind(),
            clap::error::ErrorKind::UnknownArgument,
            "{refusal}"
        );
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
        let port = start_egress_shim(
            Arc::clone(&relay),
            Arc::new(EgressGuard::unrestricted("shim-test")),
            None,
        )
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
        let port = start_egress_shim(
            Arc::clone(&relay),
            Arc::new(EgressGuard::unrestricted("shim-test")),
            None,
        )
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

    // ─── egress shim: the jail's declared hosts (`#agent-tools 3/12`) ───────────

    fn a_guard_admitting(host: &str, port: u16) -> Arc<EgressGuard> {
        Arc::new(EgressGuard::new(
            EgressPolicy::Hosts(vec![tddy_sandbox::HostPattern::new(host, port)]),
            "shim-test",
            vec!["curl".to_string()],
        ))
    }

    /// A `CONNECT` to a host the jail's tools never declared is answered `403` naming it, and the
    /// host is never asked to dial it.
    #[tokio::test]
    async fn egress_shim_answers_an_undeclared_connect_403_naming_the_host_and_opens_no_tunnel() {
        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let host = FakeHost::attach(Arc::clone(&relay), Vec::new());
        let port = start_egress_shim(
            Arc::clone(&relay),
            a_guard_admitting("api.anthropic.com", 443),
            None,
        )
        .await
        .expect("shim binds");

        // When
        let response = send_to_shim(port, b"CONNECT api.openai.com:443 HTTP/1.1\r\n\r\n").await;

        // Then
        let body = "egress to api.openai.com:443 refused for session shim-test: not declared by tool set [curl]";
        assert_eq!(
            String::from_utf8_lossy(&response),
            format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
        );
        assert_eq!(host.tunnel_target(), None);
    }

    /// The positive control: a declared host is tunnelled as before.
    #[tokio::test]
    async fn egress_shim_tunnels_a_connect_to_a_declared_host() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Given
        let relay = Arc::new(SandboxSessionRelay::default());
        let host = FakeHost::attach(Arc::clone(&relay), b"pong".to_vec());
        let port = start_egress_shim(
            Arc::clone(&relay),
            a_guard_admitting("api.anthropic.com", 443),
            None,
        )
        .await
        .expect("shim binds");

        // When
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect to shim");
        client
            .write_all(b"CONNECT API.Anthropic.com:443 HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let mut established = [0u8; 39];
        client.read_exact(&mut established).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).await.unwrap();

        // Then
        assert_eq!(
            String::from_utf8_lossy(&established),
            "HTTP/1.1 200 Connection Established\r\n\r\n"
        );
        assert_eq!(
            host.tunnel_target(),
            Some(("API.Anthropic.com".to_string(), 443))
        );
        assert_eq!(reply, b"pong");
    }

    /// The shim reads the policy the daemon lowered, and the tool set a refusal names, from its
    /// command line.
    #[test]
    fn the_runner_reads_its_egress_policy_and_tool_set_from_its_args() {
        // Given
        let args = SandboxRunnerArgs::try_parse_from([
            "tddy-sandbox-runner",
            "--session-id",
            "sess-1",
            "--context-dir",
            "/tmp/ctx",
            "--tool-ipc-socket",
            "/tmp/ipc.sock",
            "--ready-marker",
            "/tmp/ready",
            "--egress-policy",
            "hosts:127.0.0.1:8080,*.statsig.com:443",
            "--egress-tool",
            "curl",
            "--egress-tool",
            "jq",
        ])
        .expect("argv must parse");

        // When
        let guard = runner_egress_guard(&args).expect("the policy reads");

        // Then
        assert_eq!(
            guard,
            EgressGuard::new(
                EgressPolicy::Hosts(vec![
                    tddy_sandbox::HostPattern::new("127.0.0.1", 8080),
                    tddy_sandbox::HostPattern::new("*.statsig.com", 443),
                ]),
                "sess-1",
                vec!["curl".to_string(), "jq".to_string()],
            )
        );
    }

    // ─── The relay allowlist's refusal half ─────────────────────────────────────
    //
    // Feature: docs/ft/daemon/session-agent-roster.md (§ Prompting an agent)
    // Changeset: docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md
    //
    // `tddy_service::session_agents::IN_JAIL_RELAYABLE` is the list of operations an in-jail agent
    // may perform against its host, and `tddy-session-agents` already pins what is *on* it. What
    // nothing pinned is the boundary itself: that a tuple which is not on the list gets no further
    // than this function. `in_jail_conversation_acceptance.rs` drives the permitting half through a
    // real Seatbelt jail and so runs on macOS only; the refusal needs no jail and runs everywhere.

    use tddy_rpc::RpcService as _;

    /// Attach a host end to `relay` and hand back the frames it receives, so a call that got past
    /// the allowlist is visible as an `RpcRequest` on the wire rather than merely inferred from a
    /// status code.
    fn a_relay_with_a_host_attached() -> (
        Arc<SandboxSessionRelay>,
        tokio::sync::mpsc::UnboundedReceiver<Result<SessionFrame, Status>>,
    ) {
        let relay = Arc::new(SandboxSessionRelay::default());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        relay.set_outbound(tx);
        (relay, rx)
    }

    /// The jail's PTY size reaches the host as a `TerminalResized` frame sitting exactly between
    /// the output produced before the resize and the output produced after it — the host's screen
    /// model reads every byte at the grid it was drawn for only if the order survives the backlog.
    #[tokio::test]
    async fn a_reported_size_reaches_the_host_in_order_with_the_output_around_it() {
        // Given a relay whose host has subscribed to the terminal
        let relay = Arc::new(SandboxSessionRelay::default());
        *relay.terminal_subscribed.lock().unwrap() = true;

        // When output, a resize to 140x40 and more output are produced before the next poll
        relay.push_terminal(Bytes::from_static(b"before"));
        relay.push_terminal_size(140, 40);
        relay.push_terminal(Bytes::from_static(b"after"));
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();
        relay.handle_host_poll(&out_tx);
        drop(out_tx);

        // Then the host receives them in exactly that order
        let mut reported = Vec::new();
        while let Some(Ok(frame)) = out_rx.recv().await {
            match frame.payload {
                Some(SessionPayload::TerminalOutput(out)) => {
                    reported.push(format!("output {}", String::from_utf8_lossy(&out.data)))
                }
                Some(SessionPayload::TerminalResized(size)) => reported.push(format!(
                    "resized {}x{} {}",
                    size.cols, size.rows, size.terminal_id
                )),
                _ => {}
            }
        }
        assert_eq!(
            reported,
            vec![
                "output before".to_string(),
                format!("resized 140x40 {MAIN_TERMINAL_ID}"),
                "output after".to_string(),
            ]
        );
    }

    /// A `(service, method)` the allowlist does not carry is refused here, and never reaches
    /// [`SandboxSessionRelay::call_rpc`].
    ///
    /// The host end is attached deliberately: with no `SessionChannel` open, `call_rpc` fails on
    /// its own and `unavailable` would be indistinguishable from a refusal. Connected, anything the
    /// allowlist let through appears as an `RpcRequest` frame — so an empty channel is the proof
    /// that the boundary held rather than that the transport was missing.
    #[tokio::test]
    async fn refuses_an_rpc_the_jail_allowlist_does_not_carry_without_reaching_the_host() {
        // Given a connected relay, so a forwarded call would succeed
        let (relay, mut from_jail) = a_relay_with_a_host_attached();
        let service = ToolExecService {
            relay: Arc::clone(&relay),
        };

        // When an in-jail caller asks for a method that is not relayable — a real RPC the daemon
        // serves, addressed at the service family the jail may reach, but not one of the five
        let result = service
            .handle_rpc(
                tddy_service::session_agents::SESSION_AGENT_SERVICE,
                "AttachSessionAgent",
                &tddy_rpc::RpcMessage {
                    payload: Vec::new(),
                    metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
                },
            )
            .await;

        // Then it is refused NOT_FOUND, naming what was asked for
        let tddy_rpc::RpcResult::Unary(Err(status)) = result else {
            panic!("a non-relayable RPC must be refused, not answered or streamed");
        };
        assert_eq!(status.code(), tddy_rpc::Code::NotFound);
        assert!(
            status.message().contains("AttachSessionAgent"),
            "the refusal must name the method it refused; message was: {}",
            status.message()
        );

        // …and nothing was forwarded to the host
        assert!(
            from_jail.try_recv().is_err(),
            "a refused RPC must not reach the host: the relay sent a frame anyway"
        );
    }

    /// The permitting half of the same boundary, as far as it can be taken without a jail: a tuple
    /// the allowlist *does* carry is forwarded, arriving at the host as an `RpcRequest` naming it.
    /// Without this, the refusal above would also pass if the allowlist rejected everything.
    #[tokio::test]
    async fn forwards_an_allowlisted_rpc_to_the_host_as_an_rpc_request() {
        // Given
        let (relay, mut from_jail) = a_relay_with_a_host_attached();
        let service = ToolExecService {
            relay: Arc::clone(&relay),
        };
        let (allowed_service, allowed_method) = tddy_service::session_agents::IN_JAIL_RELAYABLE[0];

        // When
        let _stream = service
            .handle_rpc(
                allowed_service,
                allowed_method,
                &tddy_rpc::RpcMessage {
                    payload: Vec::new(),
                    metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
                },
            )
            .await;

        // Then the host was asked, at the coordinate the jail named
        let frame = from_jail
            .try_recv()
            .expect("an allowlisted RPC must be forwarded to the host")
            .expect("the forwarded frame must not be an error");
        let Some(SessionPayload::RpcRequest(request)) = frame.payload else {
            panic!("a forwarded RPC must travel as an RpcRequest frame");
        };
        assert_eq!(request.service, allowed_service);
        assert_eq!(request.method, allowed_method);
    }

    /// A conversation-worktree operation is an exec-tool RPC the jail may relay: it reaches the host
    /// as an `RpcRequest` naming `exec_tools.ExecToolService/ConversationWorktree`.
    #[tokio::test]
    async fn forwards_a_conversation_worktree_call_to_the_host_as_an_rpc_request() {
        // Given
        let (relay, mut from_jail) = a_relay_with_a_host_attached();
        let service = ToolExecService {
            relay: Arc::clone(&relay),
        };
        let (allowed_service, allowed_method) = tddy_tool_engine::IN_JAIL_RELAYABLE_EXEC_TOOLS[0];

        // When
        let _stream = service
            .handle_rpc(
                allowed_service,
                allowed_method,
                &tddy_rpc::RpcMessage {
                    payload: Vec::new(),
                    metadata: tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
                },
            )
            .await;

        // Then
        let frame = from_jail
            .try_recv()
            .expect("a conversation-worktree call must be forwarded to the host")
            .expect("the forwarded frame must not be an error");
        let Some(SessionPayload::RpcRequest(request)) = frame.payload else {
            panic!("a forwarded RPC must travel as an RpcRequest frame");
        };
        assert_eq!(
            (request.service.as_str(), request.method.as_str()),
            ("exec_tools.ExecToolService", "ConversationWorktree")
        );
    }
}
