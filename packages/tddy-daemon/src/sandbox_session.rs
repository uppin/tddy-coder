//! Manages sandboxed claude-cli sessions: spawn, dial, PTY bridge, tool-exec loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tddy_rpc::Status;

use tddy_sandbox::{SandboxContextDir, SandboxError, SandboxSpec};
use tddy_service::proto::connection::ExecuteToolResponse;
use tddy_service::tonic_sandbox::session_frame::Payload as SessionPayload;
use tddy_service::tonic_sandbox::{
    EgressRequest, EgressResponse, HostPoll, SandboxInput, SessionFrame, SubscribeTerminal,
};
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;

use crate::tool_engine;

/// Active sandbox session state on the host daemon.
pub struct SandboxSessionState {
    pub pid: u32,
    pub worktree_path: PathBuf,
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// Rolling PTY output for late `StreamTerminalOutput` subscribers (broadcast drops when idle).
    pub capture: Arc<StdMutex<Vec<u8>>>,
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    pub grpc_socket: PathBuf,
    pub ready_marker: PathBuf,
}

/// Registry of sandbox sessions keyed by session_id.
#[derive(Default)]
pub struct SandboxSessionManager {
    inner: Mutex<HashMap<String, Arc<SandboxSessionState>>>,
}

impl SandboxSessionManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, session_id: String, state: Arc<SandboxSessionState>) {
        self.inner.lock().await.insert(session_id, state);
    }

    pub async fn get(&self, session_id: &str) -> Option<Arc<SandboxSessionState>> {
        self.inner.lock().await.get(session_id).cloned()
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<SandboxSessionState>> {
        self.inner.lock().await.remove(session_id)
    }
}

/// Wait until the sandbox writes its ready marker.
///
/// `handle` is polled each tick so that a child which dies before writing the marker
/// (e.g. a `dyld` SIGABRT in the jail) fails fast with the decoded exit reason instead
/// of blocking until `timeout`.
pub async fn wait_for_sandbox_ready(
    handle: &mut tddy_sandbox::SandboxHandle,
    ready_marker: &Path,
    timeout: Duration,
    egress_dir: &Path,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if ready_marker.exists() {
            log::info!(
                target: "tddy_daemon::sandbox_session",
                "sandbox ready marker appeared at {}",
                ready_marker.display()
            );
            return Ok(());
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            let project_root = ready_marker.parent();
            let logs = tddy_sandbox::format_sandbox_diagnostics(egress_dir, project_root);
            log::error!(
                target: "tddy_daemon::sandbox_session",
                "sandbox child died before ready marker: {reason}"
            );
            return Err(format!(
                "sandbox child died before ready marker at {}: {reason}\n{logs}",
                ready_marker.display(),
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            let project_root = ready_marker.parent();
            let logs = tddy_sandbox::format_sandbox_diagnostics(egress_dir, project_root);
            return Err(format!(
                "timed out waiting for sandbox ready marker at {} ({}s)\n{logs}",
                ready_marker.display(),
                timeout.as_secs(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

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

/// Dial the sandbox gRPC server and start the host-driven session channel loop.
pub async fn dial_and_bridge(
    session_id: &str,
    worktree_path: PathBuf,
    ready_marker: PathBuf,
    task_registry: tddy_task::TaskRegistry,
    stdout_tx: broadcast::Sender<Bytes>,
    capture: Arc<StdMutex<Vec<u8>>>,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<(), String> {
    let mut client = connect_sandbox_client(&ready_marker).await?;

    log::info!(
        target: "tddy_daemon::sandbox_session",
        "opening sandbox SessionChannel for session {session_id} (ready_marker={})",
        ready_marker.display()
    );

    let (host_tx, host_rx) = mpsc::channel(64);
    let host_stream = ReceiverStream::new(host_rx);
    let mut session = client
        .session_channel(host_stream)
        .await
        .map_err(|e| format!("open session channel: {e}"))?
        .into_inner();

    host_tx
        .send(SessionFrame {
            payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                session_id: session_id.to_string(),
                terminal_id: "main".to_string(),
                initial_cols: 80,
                initial_rows: 24,
            })),
        })
        .await
        .map_err(|_| "session channel closed before subscribe".to_string())?;

    let session_id_out = session_id.to_string();
    let worktree_out = worktree_path.clone();
    let registry_out = task_registry.clone();
    let stdout_out = stdout_tx.clone();
    let capture_out = Arc::clone(&capture);
    let host_tx_out = host_tx.clone();

    tokio::spawn(async move {
        while let Some(Ok(frame)) = session.next().await {
            match frame.payload {
                Some(SessionPayload::ToolRequest(req)) => {
                    log::debug!(
                        target: "tddy_daemon::sandbox_session",
                        "sandbox tool request session={session_id_out} tool={}",
                        req.tool_name
                    );
                    let outcome = tool_engine::execute_tool(
                        &worktree_out,
                        &req.tool_name,
                        &req.args_json,
                        &registry_out,
                        &session_id_out,
                    )
                    .await;
                    let resp = ExecuteToolResponse {
                        result_json: outcome.result_json,
                        is_error: outcome.is_error,
                        error_message: outcome.error_message,
                        job_id: outcome.job_id,
                        job_running: outcome.job_running,
                    };
                    let _ = host_tx_out
                        .send(SessionFrame {
                            payload: Some(SessionPayload::ToolResponse(resp)),
                        })
                        .await;
                }
                Some(SessionPayload::EgressRequest(req)) => {
                    log::debug!(
                        target: "tddy_daemon::sandbox_session",
                        "sandbox egress request session={session_id_out} url={}",
                        req.url
                    );
                    let resp = relay_egress_request(req).await;
                    let _ = host_tx_out
                        .send(SessionFrame {
                            payload: Some(SessionPayload::EgressResponse(resp)),
                        })
                        .await;
                }
                Some(SessionPayload::TerminalOutput(out)) => {
                    if !out.data.is_empty() {
                        if let Ok(mut cap) = capture_out.lock() {
                            cap.extend_from_slice(&out.data);
                        }
                        let _ = stdout_out.send(Bytes::from(out.data));
                    }
                }
                _ => {}
            }
        }
    });

    let session_id_in = session_id.to_string();
    tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_millis(25));
        loop {
            tokio::select! {
                chunk = stdin_rx.recv() => {
                    let Some(chunk) = chunk else { break };
                    let _ = host_tx.send(SessionFrame {
                        payload: Some(SessionPayload::TerminalInput(SandboxInput {
                            session_id: session_id_in.clone(),
                            terminal_id: "main".to_string(),
                            data: chunk.to_vec(),
                        })),
                    }).await;
                }
                _ = poll.tick() => {
                    let _ = host_tx.send(SessionFrame {
                        payload: Some(SessionPayload::HostPoll(HostPoll {})),
                    }).await;
                }
            }
        }
    });

    Ok(())
}

async fn relay_egress_request(req: EgressRequest) -> EgressResponse {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return EgressResponse {
                request_id: req.request_id,
                error_message: format!("build http client: {e}"),
                ..Default::default()
            };
        }
    };

    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &req.url);
    for header in &req.headers {
        builder = builder.header(&header.name, &header.value);
    }
    if !req.body.is_empty() {
        builder = builder.body(req.body.clone());
    }

    match builder.send().await {
        Ok(resp) => {
            let status_code = resp.status().as_u16() as u32;
            let body = resp.bytes().await.unwrap_or_default();
            EgressResponse {
                request_id: req.request_id,
                status_code,
                body: body.to_vec(),
                ..Default::default()
            }
        }
        Err(e) => EgressResponse {
            request_id: req.request_id,
            error_message: format!("outbound fetch failed: {e}"),
            ..Default::default()
        },
    }
}

/// Map [`SandboxError`] to gRPC status for StartSession failures.
pub fn sandbox_error_to_status(err: SandboxError) -> Status {
    match err {
        SandboxError::Unsupported { platform, message } => Status::failed_precondition(format!(
            "sandbox unsupported on {platform}: {message}"
        )),
        SandboxError::Io(msg) | SandboxError::InvalidSpec(msg) => {
            Status::internal(format!("sandbox error: {msg}"))
        }
    }
}

/// Build env map for sandbox-runner inside `sandbox-exec`.
pub fn build_sandbox_runner_env(
    scratch_home: &Path,
    scratch_tmp: &Path,
    session_id: &str,
    tool_ipc_socket: &Path,
    egress_dir: &Path,
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    env.insert("HOME".into(), scratch_home.to_string_lossy().to_string());
    env.insert(
        "TMPDIR".into(),
        scratch_tmp.to_string_lossy().to_string(),
    );
    env.insert(
        "TDDY_SANDBOX_SESSION_ID".into(),
        session_id.to_string(),
    );
    env.insert(
        "TDDY_SANDBOX_TOOL_IPC".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "RUST_LOG".into(),
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into());
    for key in [
        "TDDY_EGRESS_PROBE_HOST",
        "TDDY_EGRESS_PROBE_PORT",
        "TDDY_EGRESS_PROBE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                env.insert(key.into(), value);
            }
        }
    }
    if let Ok(probe_target) = std::env::var("TDDY_EGRESS_PROBE_TARGET") {
        if !probe_target.trim().is_empty() {
            env.insert("TDDY_EGRESS_PROBE_TARGET".into(), probe_target);
        }
    }
    env
}

/// Recursively copy a directory tree.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = dst.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Prepare read-only context dir from worktree docs/skills.
pub fn prepare_context_dir(worktree_path: &Path) -> Result<SandboxContextDir, String> {
    SandboxContextDir::create(worktree_path).map_err(|e| e.to_string())
}

/// Resolve the `tddy-tools` binary for sandbox spawn and hook wiring.
///
/// Priority: explicit config → `CARGO_BIN_EXE_tddy-tools` (cargo test) → sibling of
/// `current_exe()` (handles integration tests living in `target/debug/deps/`) → `"tddy-tools"`.
pub fn resolve_tddy_tools_path(configured: Option<&str>) -> String {
    if let Some(path) = configured.filter(|s| !s.trim().is_empty()) {
        return path.to_string();
    }
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-tools") {
        if !bin.trim().is_empty() {
            return bin;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|p| p.to_path_buf()) {
            if bin_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join("tddy-tools");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("tddy-tools");
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    "tddy-tools".to_string()
}

#[cfg(target_os = "macos")]
pub fn detect_allow_read_paths() -> Vec<PathBuf> {
    tddy_sandbox_darwin::detect_allow_read_paths()
}

#[cfg(not(target_os = "macos"))]
pub fn detect_allow_read_paths() -> Vec<PathBuf> {
    vec![]
}

fn push_parent_allow_path(paths: &mut Vec<PathBuf>, binary: &str) {
    let path = Path::new(binary);
    if !path.is_absolute() {
        return;
    }
    if let Some(parent) = path.parent() {
        if !paths.iter().any(|existing| existing == parent) {
            paths.push(parent.to_path_buf());
        }
    }
}

/// Paths from `otool -L` needed to load a Mach-O binary inside Seatbelt.
#[cfg(target_os = "macos")]
fn detect_binary_load_paths(binary: &str) -> Vec<PathBuf> {
    let Ok(output) = std::process::Command::new("otool")
        .args(["-L", binary])
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut paths = Vec::new();
    for line in text.lines().skip(1) {
        let lib = line.split_whitespace().next().unwrap_or("");
        if lib.is_empty() || !lib.starts_with('/') {
            continue;
        }
        let lib_path = Path::new(lib);
        if let Some(parent) = lib_path.parent() {
            if !paths.iter().any(|existing| existing == parent) {
                paths.push(parent.to_path_buf());
            }
        }
    }
    paths
}

#[cfg(not(target_os = "macos"))]
fn detect_binary_load_paths(_binary: &str) -> Vec<PathBuf> {
    vec![]
}

fn canonical_binary_path(binary: &str) -> Option<PathBuf> {
    let path = Path::new(binary);
    if path.is_absolute() {
        std::fs::canonicalize(path).ok()
    } else {
        std::env::current_dir()
            .ok()?
            .join(path)
            .canonicalize()
            .ok()
    }
}

/// Toolchain paths plus parent dirs of the sandbox-runner and claude binaries.
pub fn build_allow_read_paths(runner_argv: &[String]) -> Vec<PathBuf> {
    let mut paths = detect_allow_read_paths();
    if let Some(tool) = runner_argv.first() {
        if let Some(path) = canonical_binary_path(tool) {
            push_parent_allow_path(&mut paths, &path.to_string_lossy());
            paths.extend(detect_binary_load_paths(&path.to_string_lossy()));
        }
    }
    if let Some(idx) = runner_argv.iter().position(|arg| arg == "--claude-binary") {
        if let Some(claude) = runner_argv.get(idx + 1) {
            if let Some(path) = canonical_binary_path(claude) {
                push_parent_allow_path(&mut paths, &path.to_string_lossy());
                paths.extend(detect_binary_load_paths(&path.to_string_lossy()));
            }
        }
    }
    paths
}

/// Pick an ephemeral loopback TCP port on the host (for Seatbelt allow-listing before spawn).
pub fn pick_free_loopback_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    Ok(port)
}

/// Spawn sandbox-runner inside Seatbelt jail.
#[cfg(target_os = "macos")]
pub fn spawn_sandbox_runner(
    project_root: PathBuf,
    scratch_dir: PathBuf,
    egress_dir: PathBuf,
    profile_path: PathBuf,
    runner_argv: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
    loopback_allow_ports: Vec<u16>,
    ipc_socket: Option<PathBuf>,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    let spec = SandboxSpec {
        project_root,
        scratch_dir,
        egress_dir,
        allow_read_paths: build_allow_read_paths(&runner_argv),
        command: runner_argv,
        env,
        profile_path,
        loopback_allow_ports,
        ipc_socket,
    };
    tddy_sandbox_darwin::spawn(spec)
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_sandbox_runner(
    _project_root: PathBuf,
    _scratch_dir: PathBuf,
    _egress_dir: PathBuf,
    _profile_path: PathBuf,
    _runner_argv: Vec<String>,
    _env: std::collections::BTreeMap<String, String>,
    _loopback_allow_ports: Vec<u16>,
    _ipc_socket: Option<PathBuf>,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "darwin Seatbelt sandboxes are not available on this OS".to_string(),
    })
}
