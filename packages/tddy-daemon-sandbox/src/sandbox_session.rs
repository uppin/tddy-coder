//! Manages sandboxed claude-cli sessions: spawn, dial, PTY bridge, tool-exec loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use bytes::Bytes;
use tddy_rpc::Status;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::jail_exit_watch::{watch_jail_exit, JailBridge, JailEnded, JailProcess};
pub use crate::jail_host_bridge::*;
pub use crate::runner_paths::*;
pub use crate::runner_spawn::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tddy_sandbox::{SandboxContextDir, SandboxError};
use tddy_task::ChildExit;
use tddy_task::TerminalCapture;
use tokio::sync::watch;

/// A resource whose lifetime is tied to a sandbox session and whose cleanup happens on `Drop`.
///
/// The daemon's managed-workflow wiring is the only implementor: a per-session toolcall listener
/// plus its workflow controller, held by [`SandboxSessionState`] purely so the listener's AF_UNIX
/// socket is unlinked when the session ends. Nothing in this crate ever calls it.
///
/// It is a trait rather than the concrete type because `ManagedWorkflow` stays in `tddy-daemon`,
/// which depends on this crate — naming it here would be a cycle. Dropping a
/// `Box<dyn SessionScopedResource>` runs the concrete type's drop glue through the vtable, so the
/// cleanup that happens is exactly the one that happened before the type was erased.
pub trait SessionScopedResource: Send + Sync {}

/// Active sandbox session state on the host daemon.
pub struct SandboxSessionState {
    pub pid: u32,
    pub worktree_path: PathBuf,
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// Rolling PTY output for late `StreamTerminalOutput` subscribers (broadcast drops when idle).
    pub capture: Arc<StdMutex<TerminalCapture>>,
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    pub ready_marker: PathBuf,
    /// Kept so delete/resume can SIGKILL the sandbox-exec tree reliably.
    process: StdMutex<Option<JailProcess>>,
    /// Set by [`Self::stop`] before it kills: a runner that ends after this was stopped by request.
    stopped: AtomicBool,
    /// How the runner ended, once known (`#tab-hardening` 3/4).
    pub(crate) exit_tx: watch::Sender<Option<ChildExit>>,
    /// The stdio endpoint, relay and forwarder tasks, aborted on stop.
    pub(crate) bridge: StdMutex<Option<JailBridge>>,
    /// Managed-workflow wiring (per-session toolcall listener + controller) when this is a managed
    /// session; kept here so its lifetime is tied to the session and its socket is cleaned up on drop.
    ///
    /// Type-erased as [`SessionScopedResource`]: the concrete `ManagedWorkflow` lives in
    /// `tddy-daemon`, which depends on this crate, and nothing here ever calls it.
    _managed_workflow: Option<Box<dyn SessionScopedResource>>,
}

/// Fields required to register an active sandbox session on the host.
pub struct SandboxSessionStateInit {
    pub pid: u32,
    pub worktree_path: PathBuf,
    pub stdout_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<StdMutex<TerminalCapture>>,
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    pub ready_marker: PathBuf,
    pub handle: tddy_sandbox::SandboxHandle,
    pub managed_workflow: Option<Box<dyn SessionScopedResource>>,
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
            process: StdMutex::new(Some(init.handle.into())),
            stopped: AtomicBool::new(false),
            exit_tx: watch::channel(None).0,
            bridge: StdMutex::new(None),
            _managed_workflow: init.managed_workflow,
        }
    }

    /// Whether the jail's runner is still running, read from the process this state owns — never
    /// from [`Self::pid`], which the OS may have reissued once the runner was reaped. Reads without
    /// reaping, so the exit status stays for the watch.
    pub fn is_running(&self) -> bool {
        self.runner_pid().is_some()
    }

    /// The pid of the runner this state owns while it is running; `None` once it has exited or been
    /// stopped. Read off the owned process, so it can never name a reissued pid.
    pub fn runner_pid(&self) -> Option<u32> {
        if self.exit().is_some() {
            return None;
        }
        let mut process = self.lock_process();
        let process = process.as_mut()?;
        process.try_exit().is_none().then(|| process.pid())
    }

    /// The runner's pid while it is unreaped (running or dead and not yet reaped), for the watch.
    pub(crate) fn runner_pid_unreaped(&self) -> Option<u32> {
        if self.stopped.load(Ordering::SeqCst) || self.exit().is_some() {
            return None;
        }
        self.lock_process().as_ref().map(JailProcess::pid)
    }

    /// The runner ended on its own with `exit`: reap it, record the exit. `None` when the jail was
    /// stopped by request meanwhile.
    pub(crate) fn record_death(&self, exit: ChildExit) -> Option<ChildExit> {
        if self.stopped.load(Ordering::SeqCst) {
            return None;
        }
        if let Some(mut process) = self.lock_process().take() {
            process.kill_and_reap();
        }
        self.record_exit(exit);
        Some(exit)
    }

    /// Record how the runner ended, once: the first exit recorded stands.
    fn record_exit(&self, exit: ChildExit) {
        self.exit_tx.send_if_modified(|current| {
            current.is_none() && {
                *current = Some(exit);
                true
            }
        });
    }

    fn lock_process(&self) -> std::sync::MutexGuard<'_, Option<JailProcess>> {
        self.process
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn stop(&self) {
        // Marked before the kill: a watch that sees the runner end must find the stop already
        // recorded, or it would announce a death nobody suffered.
        self.stopped.store(true, Ordering::SeqCst);
        // A poisoned lock is a thread that panicked while holding the handle, not a reason to
        // leave the jail running: `stop` is reached from `Drop`, and panicking here would orphan a
        // `tddy-sandbox-runner` onto the host — the very thing this method exists to prevent.
        let taken = self.lock_process().take();
        // Only the owned process is ever signalled: once it is gone, `self.pid` may name an
        // unrelated process the OS reissued.
        let exit = taken.and_then(|mut process| process.kill_and_reap());
        if let Some(bridge) = self
            .bridge
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            bridge.abort();
        }
        self.record_exit(exit.unwrap_or(ChildExit {
            code: None,
            signal: None,
        }));
    }
}

/// Stop the session's jail when the state itself goes away.
///
/// `DeleteSession` calls [`Self::stop`] explicitly, but nothing else does: a session state that is
/// simply dropped — every harness that starts a sandboxed session and lets its daemon fall out of
/// scope — would otherwise leave the `tddy-sandbox-runner` alive, reparented to `launchd` in its
/// own process group. `stop` is idempotent (the handle is taken out of the mutex), so the explicit
/// delete path and this one do not fight.
///
/// Mirrors `JailedWorkspaceSandbox`'s `Drop` in `workspace_tool_sandbox.rs`.
impl Drop for SandboxSessionState {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Registry of sandbox sessions keyed by session_id.
pub struct SandboxSessionManager {
    inner: Arc<Mutex<HashMap<String, Arc<SandboxSessionState>>>>,
    pub(crate) recorder: Arc<crate::main_recording::JailRecorder>,
    pub(crate) ended_tx: broadcast::Sender<JailEnded>,
}

impl Default for SandboxSessionManager {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            recorder: Arc::default(),
            ended_tx: broadcast::channel(64).0,
        }
    }
}

impl SandboxSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, session_id: String, state: Arc<SandboxSessionState>) {
        self.inner
            .lock()
            .await
            .insert(session_id.clone(), Arc::clone(&state));
        self.watch_for_death(session_id, state);
    }

    /// A jail that exits on its own leaves the registry (when still the one registered) and is
    /// announced on [`Self::subscribe_ended`]; one stopped by request is neither.
    fn watch_for_death(&self, session_id: String, state: Arc<SandboxSessionState>) {
        // Held weakly: the watch waits as long as the jail lives, and must not keep a dropped
        // registry (and the states in it) alive.
        let inner = Arc::downgrade(&self.inner);
        let recorder = Arc::clone(&self.recorder);
        let ended_tx = self.ended_tx.clone();
        let registered = Arc::downgrade(&state);
        tokio::spawn(async move {
            let Some(exit) = watch_jail_exit(state).await else {
                return;
            };
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let mut inner = inner.lock().await;
            let still_registered = inner
                .get(&session_id)
                .is_some_and(|current| std::ptr::eq(Arc::as_ptr(current), registered.as_ptr()));
            if still_registered {
                inner.remove(&session_id);
                drop(inner);
                recorder.finish(&session_id).await;
            }
            let _ = ended_tx.send(JailEnded { session_id, exit });
        });
    }

    pub async fn get(&self, session_id: &str) -> Option<Arc<SandboxSessionState>> {
        self.inner.lock().await.get(session_id).cloned()
    }

    /// The sandboxed sessions whose runner is still running — an entry outlives its runner until
    /// resume, relaunch or delete replaces it, so presence alone is not liveness.
    pub async fn live_sessions(&self) -> tddy_daemon_kernel::live_sessions::LiveSessions {
        let live: Vec<String> = self
            .inner
            .lock()
            .await
            .iter()
            .filter(|(_, state)| state.is_running())
            .map(|(session_id, _)| session_id.clone())
            .collect();
        tddy_daemon_kernel::live_sessions::LiveSessions::of(live)
    }

    /// Stop every session's main jail and forget it — a daemon shutting down leaves none behind;
    /// the session ids stopped, sorted.
    pub async fn stop_all(&self) -> Vec<String> {
        let mut drained: Vec<_> = self.inner.lock().await.drain().collect();
        drained.sort_by(|(left, _), (right, _)| left.cmp(right));
        let mut stopped = Vec::with_capacity(drained.len());
        for (session_id, state) in drained {
            state.stop();
            self.recorder.finish(&session_id).await;
            stopped.push(session_id);
        }
        stopped
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<SandboxSessionState>> {
        let removed = self.inner.lock().await.remove(session_id);
        self.recorder.finish(session_id).await;
        removed
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
                target: "tddy_daemon_sandbox::sandbox_session",
                "sandbox ready marker appeared at {}",
                ready_marker.display()
            );
            return Ok(());
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            let project_root = ready_marker.parent();
            let logs = tddy_sandbox::format_sandbox_diagnostics(egress_dir, project_root);
            log::error!(
                target: "tddy_daemon_sandbox::sandbox_session",
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

/// Build env map for sandbox-runner inside the jail: the scratch-rooted baseline every jail shares.
/// What a tool adds on top — its tmp redirects, its credentials — comes from its definition
/// (`tddy_sandbox_recipes::lower_files`'s overlay env), not from here.
pub fn build_sandbox_runner_env(
    scratch_home: &Path,
    scratch_tmp: &Path,
    session_id: &str,
    tool_ipc_socket: &Path,
    egress_dir: &Path,
) -> std::collections::BTreeMap<String, String> {
    tddy_sandbox::scratch_runner_env(
        scratch_home,
        scratch_tmp,
        session_id,
        tool_ipc_socket,
        egress_dir,
    )
}

/// Recursively copy a directory tree (follows symlinks).
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    tddy_sandbox::copy_tree(src, dst).map_err(|e| e.to_string())
}

/// Prepare the jail's context dir from the worktree's agent configuration.
///
/// `globs` is the session backend's allow-list
/// (`tddy_daemon::context_files::context_globs_for_session_type`), so a Cursor session gets `.cursor/`
/// and a Claude one does not, and neither gets this repo's `docs/` conventions.
pub fn prepare_context_dir(
    worktree_path: &Path,
    globs: &[&str],
    guidance_files: &[std::path::PathBuf],
) -> Result<SandboxContextDir, String> {
    SandboxContextDir::create(worktree_path, globs, guidance_files).map_err(|e| e.to_string())
}

/// Like [`prepare_context_dir`], but the prepended preamble names each entry in `replacements`
/// next to the exec tools it replaces for this session.
pub fn prepare_context_dir_with_subagent(
    worktree_path: &Path,
    replacements: &[tddy_sandbox::SubagentReplacement<'_>],
    globs: &[&str],
    guidance_files: &[std::path::PathBuf],
) -> Result<SandboxContextDir, String> {
    SandboxContextDir::create_with_subagent(worktree_path, replacements, globs, guidance_files)
        .map_err(|e| e.to_string())
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

// Stacked (not `all(test, unix)`) so the fs-port guard's file-local scan reads this module as
// test code; the conjunction keeps the same gate as before.
#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::jail_host_bridge::DaemonToolHandler;
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
                "--exec-plan",
                tddy_testing_commons::sandbox_fixtures::write_exec_plan(
                    tmp.path(),
                    &["/bin/sleep", "--model", "claude-opus-4-8"],
                )
                .to_str()
                .unwrap(),
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
        let capture = Arc::new(StdMutex::new(TerminalCapture::new()));
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
                tmp.path().join("session"),
                Arc::new(tddy_daemon_kernel::AgentActivityHub::default()),
                Arc::new(tddy_sandbox_runner::NullRpcHandler),
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

    fn a_tool_handler(
        worktree: PathBuf,
        session_dir: PathBuf,
        hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    ) -> DaemonToolHandler {
        DaemonToolHandler {
            worktree,
            task_registry: tddy_task::TaskRegistry::default(),
            session_env: Arc::new(Vec::new()),
            session_dir,
            agent_activity_hub: hub,
        }
    }

    /// Running one tool call through `DaemonToolHandler::execute` appends a `running` row before the
    /// work and a terminal (`completed`) row after it, sharing one call_id (source = "sandbox").
    #[tokio::test]
    async fn execute_appends_a_running_then_a_completed_agent_activity_row() {
        use tddy_sandbox_runner::HostToolHandler;

        // Given a worktree holding a file the Read tool can return.
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join("greeting.txt"), "hello sandbox").unwrap();
        let session_dir = tmp.path().join("session");
        let hub = Arc::new(tddy_daemon_kernel::AgentActivityHub::default());
        let handler = a_tool_handler(worktree, session_dir.clone(), Arc::clone(&hub));

        // When the agent reads that file through the sandbox tool handler.
        let outcome = handler
            .execute(
                "sandbox-activity-1",
                "",
                "Read",
                r#"{"path":"greeting.txt"}"#,
            )
            .await;
        assert!(!outcome.is_error, "Read of an existing file must succeed");

        // Then two rows were appended — a running row then a completed row for the same call.
        let raw = std::fs::read_to_string(
            session_dir.join(tddy_core::agent_activity::AGENT_ACTIVITY_FILENAME),
        )
        .unwrap();
        let rows: Vec<tddy_core::agent_activity::AgentActivityRecord> = raw
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(rows.len(), 2, "one running row and one terminal row");
        assert_eq!(rows[0].status, tddy_core::agent_activity::STATUS_RUNNING);
        assert_eq!(rows[0].source, "sandbox");
        assert_eq!(rows[1].status, tddy_core::agent_activity::STATUS_COMPLETED);
        assert_eq!(
            rows[0].call_id, rows[1].call_id,
            "both rows must share one call_id so they coalesce"
        );
    }

    /// A subagent conversation's call arriving over the jail's session channel runs in that
    /// conversation's own worktree, not in the session worktree, and its result carries the change.
    /// Feature: docs/ft/coder/managed-codebase-subagents.md
    #[tokio::test]
    async fn a_conversations_call_runs_in_the_conversations_worktree() {
        use tddy_sandbox_runner::HostToolHandler;

        // Given a session worktree that is a git checkout
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        for args in [
            &["init", "-q", "-b", "master"][..],
            &["config", "user.email", "developer@example.com"][..],
            &["config", "user.name", "Developer"][..],
            &["commit", "-q", "--allow-empty", "-m", "initial"][..],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&worktree)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
        let hub = Arc::new(tddy_daemon_kernel::AgentActivityHub::default());
        let handler = a_tool_handler(worktree.clone(), tmp.path().join("session"), hub);

        // When a conversation writes through the sandbox tool handler
        let outcome = handler
            .execute(
                "sandbox-conversation-1",
                "explore",
                "Write",
                r#"{"path":"a.txt","contents":"a\n"}"#,
            )
            .await;

        // Then
        let result: serde_json::Value = serde_json::from_str(&outcome.result_json).unwrap();
        assert_eq!(
            (
                worktree
                    .join("tmp/subagent-worktrees/explore/a.txt")
                    .exists(),
                worktree.join("a.txt").exists(),
                result["worktreeChange"]["files"].clone(),
            ),
            (
                true,
                false,
                serde_json::json!({ "created": 1, "updated": 0, "removed": 0 })
            )
        );
    }

    /// A failing tool call records a terminal `error` row (not `completed`).
    #[tokio::test]
    async fn execute_appends_an_error_row_when_the_tool_fails() {
        use tddy_sandbox_runner::HostToolHandler;

        // Given a worktree with no such file.
        let tmp = tempfile::tempdir().unwrap();
        let worktree = tmp.path().join("worktree");
        std::fs::create_dir_all(&worktree).unwrap();
        let session_dir = tmp.path().join("session");
        let hub = Arc::new(tddy_daemon_kernel::AgentActivityHub::default());
        let handler = a_tool_handler(worktree, session_dir.clone(), hub);

        // When the agent reads a missing file.
        let outcome = handler
            .execute(
                "sandbox-activity-err",
                "",
                "Read",
                r#"{"path":"missing.txt"}"#,
            )
            .await;
        assert!(outcome.is_error, "reading a missing file must be an error");

        // Then the coalesced call is in the error state.
        let records = tddy_core::agent_activity::read_agent_activity(&session_dir).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, tddy_core::agent_activity::STATUS_ERROR);
    }
}
