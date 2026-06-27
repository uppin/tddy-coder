//! Sandbox runner — in-jail gRPC server + claude PTY + MCP tool-exec bridge.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::{Request, Response, Status, Streaming};

use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse, SessionTerminalOutput};
use tddy_service::tonic_sandbox::sandbox_service_server::{SandboxService, SandboxServiceServer};
use tddy_service::tonic_sandbox::session_frame::Payload as SessionPayload;
use tddy_service::tonic_sandbox::{
    EchoRequest, EchoResponse, EchoStreamFrame, EgressRequest, EgressResponse, SessionFrame,
};

use tddy_sandbox::{
    append_line, egress_log_path, SANDBOX_RUNNER_FAILURE, SANDBOX_RUNNER_LOG,
};

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

/// Args for `tddy-tools sandbox-runner` (runs inside the darwin sandbox).
#[derive(Parser, Debug)]
pub struct SandboxRunnerArgs {
    #[arg(long)]
    pub session_id: String,
    #[arg(long)]
    pub context_dir: PathBuf,
    #[arg(long)]
    pub grpc_socket: PathBuf,
    #[arg(long)]
    pub tool_ipc_socket: PathBuf,
    #[arg(long, default_value = "claude")]
    pub claude_binary: String,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub ready_marker: PathBuf,
    #[arg(long, default_value = "auto")]
    pub permission_mode: String,
    /// When set, bind sandbox gRPC on this loopback port (required inside Seatbelt).
    #[arg(long)]
    pub grpc_listen_port: Option<u16>,
    /// When set, bind the in-jail egress HTTP shim on this loopback port.
    #[arg(long)]
    pub egress_shim_port: Option<u16>,
}

struct PendingToolCall {
    response_tx: oneshot::Sender<ExecuteToolResponse>,
    request: ExecuteToolRequest,
}

struct PendingEgressCall {
    response_tx: oneshot::Sender<EgressResponse>,
    request: EgressRequest,
}

/// Host-poll session relay: MCP tool calls and egress requests queue until the host sends `HostPoll`.
#[derive(Default)]
struct SandboxSessionRelay {
    queued_tools: Mutex<VecDeque<PendingToolCall>>,
    awaiting_tool: Mutex<Option<PendingToolCall>>,
    queued_egress: Mutex<VecDeque<PendingEgressCall>>,
    awaiting_egress: Mutex<Option<PendingEgressCall>>,
    terminal_subscribed: Mutex<bool>,
    egress_seq: AtomicU64,
}

impl SandboxSessionRelay {
    async fn call_tool(&self, tool_name: &str, args_json: &str) -> ExecuteToolResponse {
        let (tx, rx) = oneshot::channel();
        let mut req = ExecuteToolRequest::default();
        req.session_id = std::env::var("TDDY_SANDBOX_SESSION_ID").unwrap_or_default();
        req.tool_name = tool_name.to_string();
        req.args_json = args_json.to_string();
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

    async fn call_egress(&self, method: &str, url: &str) -> EgressResponse {
        let (tx, rx) = oneshot::channel();
        let request_id = format!("egress-{}", self.egress_seq.fetch_add(1, Ordering::Relaxed));
        let request = EgressRequest {
            request_id: request_id.clone(),
            method: method.to_string(),
            url: url.to_string(),
            ..Default::default()
        };
        self.queued_egress.lock().unwrap().push_back(PendingEgressCall {
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

    fn handle_host_poll(
        &self,
        out_tx: &tokio::sync::mpsc::UnboundedSender<Result<SessionFrame, Status>>,
        terminal_rx: &mut broadcast::Receiver<Bytes>,
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

        if !*self.terminal_subscribed.lock().unwrap() {
            return;
        }
        loop {
            match terminal_rx.try_recv() {
                Ok(chunk) if !chunk.is_empty() => {
                    let frame = SessionFrame {
                        payload: Some(SessionPayload::TerminalOutput(SessionTerminalOutput {
                            data: chunk.to_vec(),
                        })),
                    };
                    if out_tx.send(Ok(frame)).is_err() {
                        break;
                    }
                }
                Ok(_) => continue,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed | broadcast::error::TryRecvError::Empty) => {
                    break;
                }
            }
        }
    }
}

struct SandboxRunnerService {
    session_id: String,
    stdout_tx: broadcast::Sender<Bytes>,
    stdin_tx: std::sync::mpsc::Sender<Bytes>,
    relay: Arc<SandboxSessionRelay>,
}

#[tonic::async_trait]
impl SandboxService for SandboxRunnerService {
    type SessionChannelStream =
        tokio_stream::wrappers::UnboundedReceiverStream<Result<SessionFrame, Status>>;

    async fn session_channel(
        &self,
        request: Request<Streaming<SessionFrame>>,
    ) -> Result<Response<Self::SessionChannelStream>, Status> {
        let mut inbound = request.into_inner();
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        let relay = Arc::clone(&self.relay);
        let session_id = self.session_id.clone();
        let stdin_tx = self.stdin_tx.clone();
        let mut terminal_rx = self.stdout_tx.subscribe();

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
                    Some(SessionPayload::HostPoll(_)) => {
                        relay.handle_host_poll(&out_tx, &mut terminal_rx);
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

struct PtyState {
    stdout_tx: broadcast::Sender<Bytes>,
    stdin_tx: std::sync::mpsc::Sender<Bytes>,
}

fn run_claude_pty_thread(
    argv: Vec<String>,
    context_dir: PathBuf,
    egress_shim: String,
    stdout_tx: broadcast::Sender<Bytes>,
    stdin_rx: std::sync::mpsc::Receiver<Bytes>,
) -> Result<()> {
    boot_log(
        "INFO",
        &format!(
            "pty: openpty claude={} cwd={} argv={argv:?}",
            argv.first().map(String::as_str).unwrap_or("<missing>"),
            context_dir.display(),
        ),
    );
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("openpty")?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&context_dir);
    cmd.env("TERM", "xterm-256color");
    cmd.env("TDDY_EGRESS_SHIM", &egress_shim);
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
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("spawn claude in pty")?;
    boot_log("INFO", "pty: claude spawned");
    drop(pair.slave);
    let master = Arc::new(Mutex::new(pair.master));

    let master_reader = Arc::clone(&master);
    std::thread::spawn(move || {
        if let Ok(mut r) = master_reader.lock().unwrap().try_clone_reader() {
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut r, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = stdout_tx.send(Bytes::copy_from_slice(&buf[..n]));
                    }
                    Err(e) => {
                        boot_log("ERROR", &format!("pty: stdout read failed: {e}"));
                        break;
                    }
                }
            }
        } else {
            boot_log("ERROR", "pty: try_clone_reader failed");
        }
    });

    let master_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        while let Ok(chunk) = stdin_rx.recv() {
            if let Ok(mut w) = master_writer.lock().unwrap().take_writer() {
                if let Err(e) = std::io::Write::write_all(&mut w, &chunk) {
                    boot_log("ERROR", &format!("pty: stdin write failed: {e}"));
                    break;
                }
            } else {
                boot_log("ERROR", "pty: take_writer failed");
                break;
            }
        }
    });

    match child.wait() {
        Ok(status) => boot_log("INFO", &format!("pty: claude exited {status}")),
        Err(e) => boot_log("ERROR", &format!("pty: wait failed: {e}")),
    }
    Ok(())
}

fn spawn_claude_pty(
    context_dir: &Path,
    claude_binary: &str,
    model: &str,
    permission_mode: &str,
    session_id: &str,
    egress_shim: &str,
) -> Result<PtyState> {
    let (stdout_tx, _) = broadcast::channel(256);
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

    let context_dir = context_dir.to_path_buf();
    let stdout_tx_thread = stdout_tx.clone();
    let egress_shim = egress_shim.to_string();

    std::thread::spawn(move || {
        if let Err(e) = run_claude_pty_thread(argv, context_dir, egress_shim, stdout_tx_thread, stdin_rx)
        {
            boot_log_error("spawn_claude_pty", format!("{e:#}"));
            write_failure_marker(&format!("spawn_claude_pty failed: {e:#}"));
        }
    });

    Ok(PtyState {
        stdout_tx,
        stdin_tx,
    })
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
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let relay = Arc::clone(&relay);
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 65536];
                if let Ok(n) = stream.read(&mut buf).await {
                    if n == 0 {
                        return;
                    }
                    let req: serde_json::Value = match serde_json::from_slice(&buf[..n]) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    let tool_name = req
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args_json = req
                        .get("args_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");
                    let resp = relay.call_tool(tool_name, args_json).await;
                    let out = serde_json::json!({
                        "result_json": resp.result_json,
                        "is_error": resp.is_error,
                        "error_message": resp.error_message,
                    });
                    let _ = stream.write_all(out.to_string().as_bytes()).await;
                }
            });
        }
    });
    ready_rx
        .await
        .map_err(|_| anyhow::anyhow!("tool ipc server exited before bind"))?;
    Ok(())
}

async fn start_egress_shim(
    relay: Arc<SandboxSessionRelay>,
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
    let port = listener.local_addr().context("egress shim local addr")?.port();
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
    let first = req.lines().next().unwrap_or("");
    if !first.starts_with("GET /probe") {
        let _ = stream
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
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
        target: "tddy_tools::sandbox_runner",
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
    boot_log("INFO", &format!("boot: egress shim listening on {egress_shim}"));

    boot_log(
        "INFO",
        &format!(
            "boot: start_tool_ipc_server path={}",
            args.tool_ipc_socket.display()
        ),
    );
    start_tool_ipc_server(args.tool_ipc_socket.clone(), Arc::clone(&relay))
        .await
        .inspect_err(|e| boot_log_error("start_tool_ipc_server", format!("{e:#}")))?;
    boot_log("INFO", "boot: tool ipc server ready");

    boot_log(
        "INFO",
        &format!("boot: spawn_claude_pty binary={}", args.claude_binary),
    );
    let pty = spawn_claude_pty(
        &args.context_dir,
        &args.claude_binary,
        &args.model,
        &args.permission_mode,
        &args.session_id,
        &egress_shim,
    )
    .inspect_err(|e| boot_log_error("spawn_claude_pty", format!("{e:#}")))?;
    boot_log("INFO", "boot: claude pty thread spawned");

    let service = SandboxRunnerService {
        session_id: args.session_id.clone(),
        stdout_tx: pty.stdout_tx,
        stdin_tx: pty.stdin_tx,
        relay,
    };

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
        target: "tddy_tools::sandbox_runner",
        "sandbox gRPC listening on localhost:{port} (ready_marker={})",
        args.ready_marker.display()
    );
    sandbox_log_line("INFO", &format!("gRPC listening on localhost:{port}"));

    boot_log("INFO", "boot: serve sandbox gRPC");
    Server::builder()
        .add_service(SandboxServiceServer::new(service))
        .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
        .await
        .context("serve sandbox grpc")
        .inspect_err(|e| boot_log_error("serve_sandbox_grpc", format!("{e:#}")))?;
    Ok(())
}

/// Connect a tonic client to the sandbox gRPC endpoint (port read from ready marker).
pub async fn connect_sandbox_client(
    ready_marker: &Path,
) -> Result<tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient<tonic::transport::Channel>>
{
    let port_str = std::fs::read_to_string(ready_marker).context("read ready marker")?;
    let port: u16 = port_str.trim().parse().context("parse grpc port")?;
    let endpoint = format!("http://127.0.0.1:{port}");
    // Prefer 127.0.0.1 for host-side dial; server may bind localhost (same loopback on macOS).
    tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient::connect(endpoint)
        .await
        .context("connect sandbox grpc")
}

/// Dispatch a tool call via the sandbox tool IPC socket (used by `tddy-tools --mcp` in sandbox).
pub async fn dispatch_sandbox_tool_ipc(tool_name: &str, args: serde_json::Value) -> String {
    let Some(path) = std::env::var_os("TDDY_SANDBOX_TOOL_IPC") else {
        return serde_json::json!({
            "error": "TDDY_SANDBOX_TOOL_IPC not set",
            "is_error": true
        })
        .to_string();
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = match tokio::net::UnixStream::connect(path).await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({"error": format!("tool ipc connect: {e}"), "is_error": true})
                .to_string();
        }
    };
    let req = serde_json::json!({
        "tool_name": tool_name,
        "args_json": args.to_string(),
    });
    if stream
        .write_all(req.to_string().as_bytes())
        .await
        .is_err()
    {
        return serde_json::json!({"error": "tool ipc write failed", "is_error": true}).to_string();
    }
    let _ = stream.shutdown().await;
    let mut buf = vec![0u8; 65536];
    match stream.read(&mut buf).await {
        Ok(n) if n > 0 => String::from_utf8_lossy(&buf[..n]).to_string(),
        Ok(_) => serde_json::json!({"error": "tool ipc empty response", "is_error": true})
            .to_string(),
        Err(e) => serde_json::json!({"error": format!("tool ipc read: {e}"), "is_error": true})
            .to_string(),
    }
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
        let (_stdout_tx, _) = broadcast::channel(4);
        let mut terminal_rx = _stdout_tx.subscribe();

        let call = tokio::spawn({
            let relay = Arc::clone(&relay);
            async move { relay.call_tool("Read", r#"{"path":"README.md"}"#).await }
        });

        // When — host poll flushes the queued request
        relay.handle_host_poll(&out_tx, &mut terminal_rx);
        let frame = out_rx.recv().await.expect("tool request frame").expect("ok frame");
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
}
