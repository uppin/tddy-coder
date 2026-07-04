//! Manages sandboxed claude-cli sessions: spawn, dial, PTY bridge, tool-exec loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_rpc::Status;
use tokio::sync::{broadcast, mpsc, Mutex};

use tddy_sandbox::{MountSpec, SandboxContextDir, SandboxError, SandboxPlan};
use tddy_service::proto::connection::ExecuteToolResponse;
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
    pub ready_marker: PathBuf,
    /// Kept so delete/resume can SIGKILL the sandbox-exec tree reliably.
    handle: StdMutex<Option<tddy_sandbox::SandboxHandle>>,
    /// Managed-workflow wiring (per-session toolcall listener + controller) when this is a managed
    /// session; kept here so its lifetime is tied to the session and its socket is cleaned up on drop.
    _managed_workflow: Option<crate::session_toolcall::ManagedWorkflow>,
}

/// Fields required to register an active sandbox session on the host.
pub struct SandboxSessionStateInit {
    pub pid: u32,
    pub worktree_path: PathBuf,
    pub stdout_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<StdMutex<Vec<u8>>>,
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    pub ready_marker: PathBuf,
    pub handle: tddy_sandbox::SandboxHandle,
    pub managed_workflow: Option<crate::session_toolcall::ManagedWorkflow>,
}

impl SandboxSessionState {
    pub fn new(init: SandboxSessionStateInit) -> Self {
        Self {
            pid: init.pid,
            worktree_path: init.worktree_path,
            stdout_tx: init.stdout_tx,
            capture: init.capture,
            stdin_tx: init.stdin_tx,
            ready_marker: init.ready_marker,
            handle: StdMutex::new(Some(init.handle)),
            _managed_workflow: init.managed_workflow,
        }
    }

    pub fn stop(&self) {
        if let Some(mut handle) = self.handle.lock().unwrap().take() {
            let _ = handle.child_mut().kill();
            let _ = handle.child_mut().wait();
        } else {
            terminate_sandbox_process(self.pid);
        }
    }
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

/// Bridge a sandboxed process's piped stdio (spawned with `--stdio` in its argv — see
/// `tddy_sandbox_darwin::spawn_plan`) into an RPC endpoint, hosting `service` for inbound calls
/// (none exist in the current `SandboxService` protocol: the runner never initiates calls into
/// the daemon, only sends frames within the bidi `SessionChannel` the daemon itself calls) and
/// returning a client for calling into the runner. Spawns the endpoint's read/dispatch/write loop
/// on the current tokio runtime; the returned `JoinHandle` completes when the pipe closes (the
/// sandboxed process exits).
pub fn bridge_sandbox_stdio<S: tddy_rpc::RpcService>(
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
    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(receiver, sender, service);
    let run_handle = tokio::spawn(endpoint.run());
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

/// Tool handler that runs MCP tool calls in the session worktree via [`tool_engine`].
struct DaemonToolHandler {
    worktree: PathBuf,
    task_registry: tddy_task::TaskRegistry,
    /// Extra env applied to spawned shell commands — for a managed session this carries the
    /// per-session `TDDY_SOCKET` (+ `PATH`) so host-side `tddy-tools transition` reaches the
    /// session's `WorkflowController`.
    session_env: Arc<Vec<(String, String)>>,
}

#[async_trait]
impl tddy_sandbox_runner::HostToolHandler for DaemonToolHandler {
    async fn execute(
        &self,
        session_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse {
        let outcome = tool_engine::execute_tool_with_env(
            &self.worktree,
            tool_name,
            args_json,
            &self.task_registry,
            session_id,
            &self.session_env,
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
struct NoCallbackSandboxService;

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
/// rolling capture buffer (late subscribers); tool calls run in `worktree_path`.
#[allow(clippy::too_many_arguments)]
pub async fn dial_and_bridge(
    session_id: &str,
    worktree_path: PathBuf,
    handle: &mut tddy_sandbox::SandboxHandle,
    task_registry: tddy_task::TaskRegistry,
    stdout_tx: broadcast::Sender<Bytes>,
    capture: Arc<StdMutex<Vec<u8>>>,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    session_env: Arc<Vec<(String, String)>>,
) -> Result<(), String> {
    log::info!(
        target: "tddy_daemon::sandbox_session",
        "opening sandbox SessionChannel for session {session_id}"
    );

    let (client, _run_handle) = bridge_sandbox_stdio(handle, NoCallbackSandboxService)?;
    let stdio_client = tddy_sandbox_runner::StdioSandboxClient::new(client);

    let (term_tx, mut term_rx) = mpsc::unbounded_channel::<Bytes>();
    let stdout_out = stdout_tx.clone();
    let capture_out = Arc::clone(&capture);
    tokio::spawn(async move {
        while let Some(chunk) = term_rx.recv().await {
            if let Ok(mut cap) = capture_out.lock() {
                cap.extend_from_slice(&chunk);
            }
            let _ = stdout_out.send(chunk);
        }
    });

    let handler = DaemonToolHandler {
        worktree: worktree_path,
        task_registry,
        session_env,
    };
    tddy_sandbox_runner::run_host_relay(
        stdio_client,
        handler,
        tddy_sandbox_runner::HostRelayConfig::new(session_id, term_tx),
        stdin_rx,
    )
    .await?;
    Ok(())
}

/// Map [`SandboxError`] to gRPC status for StartSession failures.
pub fn sandbox_error_to_status(err: SandboxError) -> Status {
    match err {
        SandboxError::Unsupported { platform, message } => {
            Status::failed_precondition(format!("sandbox unsupported on {platform}: {message}"))
        }
        SandboxError::Io(msg) | SandboxError::InvalidSpec(msg) => {
            Status::internal(format!("sandbox error: {msg}"))
        }
    }
}

/// Build env map for sandbox-runner inside the jail.
pub fn build_sandbox_runner_env(
    scratch_home: &Path,
    scratch_tmp: &Path,
    session_id: &str,
    tool_ipc_socket: &Path,
    egress_dir: &Path,
) -> std::collections::BTreeMap<String, String> {
    let mut env = tddy_sandbox::scratch_runner_env(
        scratch_home,
        scratch_tmp,
        session_id,
        tool_ipc_socket,
        egress_dir,
    );
    env.extend(tddy_sandbox_recipes::claude_runner_env_overlay(scratch_tmp));
    env
}

/// Recursively copy a directory tree (follows symlinks).
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    tddy_sandbox::copy_tree(src, dst).map_err(|e| e.to_string())
}

/// Prepare read-only context dir from worktree docs/skills.
pub fn prepare_context_dir(worktree_path: &Path) -> Result<SandboxContextDir, String> {
    SandboxContextDir::create(worktree_path).map_err(|e| e.to_string())
}

/// Like [`prepare_context_dir`], but the appended appendix names each entry in `replacements`
/// next to the exec tools it replaces for this session.
pub fn prepare_context_dir_with_subagent(
    worktree_path: &Path,
    replacements: &[tddy_sandbox::SubagentReplacement<'_>],
) -> Result<SandboxContextDir, String> {
    SandboxContextDir::create_with_subagent(worktree_path, replacements).map_err(|e| e.to_string())
}

/// Resolve the `tddy-tools` binary for sandbox MCP and hook wiring.
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

/// Resolve the `tddy-sandbox-runner` binary for in-jail sandbox sessions.
///
/// Priority: `CARGO_BIN_EXE_tddy-sandbox-runner` (cargo test) → sibling of
/// `current_exe()` → `"tddy-sandbox-runner"`.
pub fn resolve_sandbox_runner_path() -> String {
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_tddy-sandbox-runner") {
        if !bin.trim().is_empty() {
            return bin;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(|p| p.to_path_buf()) {
            if bin_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join("tddy-sandbox-runner");
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("tddy-sandbox-runner");
            if sibling.is_file() {
                return sibling.to_string_lossy().into_owned();
            }
        }
    }
    "tddy-sandbox-runner".to_string()
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
        std::env::current_dir().ok()?.join(path).canonicalize().ok()
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

/// Terminate a sandbox-exec process group (leader pid from [`SandboxHandle::pid`]).
#[cfg(unix)]
pub fn terminate_sandbox_process(pid: u32) {
    fn pid_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::sleep(Duration::from_millis(200));
    if pid_alive(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
pub fn terminate_sandbox_process(_pid: u32) {}

/// Parameters for spawning `tddy-sandbox-runner` inside Seatbelt.
pub struct SandboxRunnerSpawn {
    pub project_root: PathBuf,
    pub scratch_dir: PathBuf,
    pub egress_dir: PathBuf,
    pub profile_path: PathBuf,
    pub runner_argv: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub loopback_allow_ports: Vec<u16>,
    pub ipc_socket: Option<PathBuf>,
    /// Host directories made available inside the jail (e.g. the project repo, or the persistent
    /// claude home). Empty for the daemon's remote-codebase sessions.
    pub mounts: Vec<MountSpec>,
    /// Host `$HOME` used by the recipe to auto-copy `.claude/.credentials.json` into the jail each
    /// session. `None` disables that per-session copy — required when a **persistent** claude home
    /// is mounted and seeded separately (a re-copy would clobber the jail's refreshed OAuth token).
    pub host_home: Option<PathBuf>,
}

/// Assemble the explicit [`SandboxPlan`] for a runner spawn via `tddy-sandbox-recipes`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn build_sandbox_plan(params: SandboxRunnerSpawn) -> Result<SandboxPlan, SandboxError> {
    use tddy_sandbox_recipes::{build_runner_plan, RunnerPlanRequest};

    build_runner_plan(RunnerPlanRequest {
        project_root: params.project_root,
        scratch_dir: params.scratch_dir,
        egress_dir: params.egress_dir,
        profile_path: params.profile_path,
        runner_argv: params.runner_argv,
        env: params.env,
        loopback_allow_ports: params.loopback_allow_ports,
        ipc_socket: params.ipc_socket,
        mounts: params.mounts,
        recipe: None,
        host_home: params.host_home,
    })
}

/// Prepare the single daemon-wide persistent jail `$HOME`: ensure it exists, seed
/// `.claude/.credentials.json` once (non-clobbering — a refreshed jail token survives), and mirror
/// the claude install so the in-jail startup self-check passes. Returns the canonical home path to
/// use as HOME and as the read-write mount. Best-effort: failures are logged, not fatal — a session
/// can still run (at worst it re-authenticates or emits an install self-check warning).
pub fn prepare_persistent_claude_home(claude_home_dir: &Path, claude_binary: &str) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(claude_home_dir) {
        log::warn!(
            "persistent claude home {}: create failed: {e}",
            claude_home_dir.display()
        );
    }
    if let Err(e) = tddy_sandbox_recipes::seed_claude_credentials(claude_home_dir) {
        log::warn!(
            "persistent claude home {}: credential seed failed: {e}",
            claude_home_dir.display()
        );
    }
    #[cfg(unix)]
    if let Err(e) = tddy_sandbox_recipes::seed_claude_local_install(claude_home_dir, claude_binary)
    {
        log::warn!(
            "persistent claude home {}: install mirror failed: {e}",
            claude_home_dir.display()
        );
    }
    #[cfg(not(unix))]
    let _ = claude_binary; // install mirror is unix-only
    std::fs::canonicalize(claude_home_dir).unwrap_or_else(|_| claude_home_dir.to_path_buf())
}

/// When set to `qemu`, [`spawn_sandbox_runner`] and [`crate::sandbox_action::spawn_confined_plan`]
/// route to [`tddy_sandbox_qemu::spawn_plan`] instead of the per-OS default (Seatbelt on macOS,
/// cgroups+namespaces on Linux). Unset (or any other value) leaves existing per-OS dispatch
/// unchanged — this is an explicit opt-in, not a fallback.
pub fn qemu_backend_requested() -> bool {
    std::env::var("TDDY_SANDBOX_BACKEND").as_deref() == Ok("qemu")
}

/// Spawn sandbox-runner inside Seatbelt jail (or the QEMU VM backend, if requested).
#[cfg(target_os = "macos")]
pub fn spawn_sandbox_runner(
    params: SandboxRunnerSpawn,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    let plan = build_sandbox_plan(params)?;
    if qemu_backend_requested() {
        return tddy_sandbox_qemu::spawn_plan(plan);
    }
    tddy_sandbox_darwin::spawn_plan(plan)
}

/// Spawn sandbox-runner inside a rootless cgroups + namespaces jail (or the QEMU VM backend, if
/// requested) on Linux.
#[cfg(target_os = "linux")]
pub fn spawn_sandbox_runner(
    params: SandboxRunnerSpawn,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    let plan = build_sandbox_plan(params)?;
    if qemu_backend_requested() {
        return tddy_sandbox_qemu::spawn_plan(plan);
    }
    tddy_sandbox_cgroups::spawn_plan(plan)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn spawn_sandbox_runner(
    _params: SandboxRunnerSpawn,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "platform sandboxes are not available on this OS".to_string(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn sandbox_runner_binary() -> PathBuf {
        std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/debug/tddy-sandbox-runner")
            })
    }

    /// **dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client**: `dial_and_bridge`
    /// must dial the runner via `StdioSandboxClient` — not the tonic `SandboxServiceClient` — so
    /// it works against a directly spawned (unsandboxed) `tddy-sandbox-runner --stdio`, with no
    /// gRPC socket or port anywhere. `sandbox_session_stdio_acceptance.rs` proves the same
    /// function through a real Seatbelt jail with a real tool-call round trip; this unit-level
    /// test isolates `dial_and_bridge`'s own dial/bridge/subscribe wiring, mirroring
    /// `sandbox_runner_stdio_acceptance.rs`'s "usage-error-as-deterministic-PTY-output" technique
    /// so no jail, tool-IPC socket, or real Claude binary is needed.
    #[tokio::test]
    async fn dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client() {
        // Given a directly spawned (unsandboxed) tddy-sandbox-runner --stdio
        let runner = sandbox_runner_binary();
        assert!(runner.exists(), "build tddy-sandbox-runner first");

        let tmp = tempfile::tempdir().unwrap();
        let context_dir = tmp.path().join("context");
        std::fs::create_dir_all(&context_dir).unwrap();

        let child = Command::new(&runner)
            .env_clear()
            .args([
                "--session-id",
                "dial-and-bridge-unit",
                "--context-dir",
                context_dir.to_str().unwrap(),
                "--tool-ipc-socket",
                tmp.path().join("tool_ipc.sock").to_str().unwrap(),
                "--tddy-tools-path",
                "/bin/sleep",
                "--ready-marker",
                tmp.path().join("sandbox.ready").to_str().unwrap(),
                "--claude-binary",
                "/bin/sleep",
                "--model",
                "claude-opus-4-8",
                "--permission-mode",
                "auto",
                "--stdio",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn tddy-sandbox-runner directly");

        let mut handle = tddy_sandbox::SandboxHandle::new(
            child,
            tmp.path().join("profile.sb"),
            tmp.path().join("unused.grpc.sock"),
            tmp.path().join("sandbox.ready"),
        );

        // When driving the production dial_and_bridge over its piped stdio, with the terminal
        // output subscription held before the call so we can observe the relay's own PTY-poll
        // loop deliver something
        let (stdout_tx, mut stdout_rx) = broadcast::channel(16);
        let capture = Arc::new(StdMutex::new(Vec::new()));
        let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        let task_registry = tddy_task::TaskRegistry::default();

        tokio::time::timeout(
            Duration::from_secs(5),
            dial_and_bridge(
                "dial-and-bridge-unit",
                tmp.path().join("worktree"),
                &mut handle,
                task_registry,
                stdout_tx,
                capture,
                stdin_rx,
                Arc::new(Vec::new()),
            ),
        )
        .await
        .expect("dial_and_bridge timed out")
        .expect("dial_and_bridge over stdio");

        // Then terminal output eventually arrives — `/bin/sleep` rejects the claude-style flags
        // it was invoked with and writes a usage error to its controlling PTY; that real output
        // reaching us here is the deterministic proof that dial_and_bridge actually dialed and
        // subscribed over stdio (a tonic dial would have failed immediately: no gRPC socket or
        // port exists anywhere in this test).
        let chunk = tokio::time::timeout(Duration::from_secs(5), stdout_rx.recv())
            .await
            .expect("no terminal output arrived over the stdio-served SessionChannel")
            .expect("terminal broadcast channel closed");
        assert!(!chunk.is_empty(), "expected non-empty terminal output");

        handle.child_mut().kill().ok();
        handle.child_mut().wait().ok();
    }
}
