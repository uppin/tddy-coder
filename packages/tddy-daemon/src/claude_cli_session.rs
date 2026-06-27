//! Claude Code CLI session manager — spawns `claude` CLI in a PTY, plumbs I/O via
//! tokio channels. Designed for raw-terminal forwarding via `StreamSessionTerminalIO` gRPC.
//!
//! Uses `portable-pty` so that `claude` (a TUI) sees a controlling terminal (TTY), allowing it to
//! run interactively. Without a PTY, `claude` detects no TTY and exits immediately.
//!
//! The acceptance tests use `/bin/cat` as the stub binary; for production, `claude` is used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use prost::Message as _;
use tddy_livekit::{LiveKitParticipant, RpcResult, RpcService, TokenGenerator};
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tokio::sync::{broadcast, mpsc, watch, RwLock};

/// Default terminal size for spawned claude sessions.
const DEFAULT_TERM_ROWS: u16 = 24;
const DEFAULT_TERM_COLS: u16 = 220;

/// Capacity for the broadcast output channel (bytes chunks from PTY master to all subscribers).
const OUTPUT_BROADCAST_CAPACITY: usize = 256;

/// Rolling capture of PTY output for replay to late-connecting subscribers.
///
/// Broadcast channels only buffer for *current* subscribers — any output emitted before the first
/// `stream_terminal_output` call is silently dropped (send returns `Err` with 0 receivers).
/// The capture buffer accumulates all raw bytes from session start so that when a subscriber
/// later connects it can replay the full screen state from the beginning.
// Ratatui full-screen redraws are ~4–8 KB per frame; 64 KB only covers ~8–16 frames.
// 512 KB gives ~64–128 frames of replay context for reconnecting clients.
const CAPTURE_LIMIT_BYTES: usize = 524288; // 512 KB

/// Reserved terminal id for the original `claude` terminal of a session.
///
/// Every terminal in a session is identified; the `claude` terminal spawned at session start is
/// always addressable under this id, while started shell terminals receive fresh unique ids.
pub const MAIN_TERMINAL_ID: &str = "main";

/// Handle to a running process in a PTY (the `claude` CLI for the main terminal, or a login shell
/// for started terminals).
pub struct PtyHandle {
    /// Stable identifier within the session; [`MAIN_TERMINAL_ID`] for the main `claude` terminal.
    pub terminal_id: String,
    /// Tool kind label: `"claude-cli"` for the main terminal, `"bash"` for started Bash tools.
    pub kind: String,
    pub worktree_path: PathBuf,
    pub model: String,
    /// Send bytes to the child process via PTY master (stdin).
    pub stdin_tx: mpsc::UnboundedSender<Bytes>,
    /// Subscribe to bytes from the child process via PTY master (stdout+stderr combined).
    pub stdout_tx: broadcast::Sender<Bytes>,
    /// Rolling capture of all PTY output since session start, for replay to late subscribers.
    pub capture: Arc<std::sync::Mutex<Vec<u8>>>,
    /// PID of the spawned process.
    pub pid: u32,
    /// PTY master — kept alive for the session's lifetime to avoid SIGHUP; also allows resize.
    master: Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    /// Becomes true (and sender drops) when the PTY reader thread exits — signals no more output.
    pub pty_done: watch::Receiver<bool>,
    /// Current PTY dimensions, updated by `resize()`.
    current_size: Arc<std::sync::Mutex<PtySize>>,
}

impl PtyHandle {
    /// Resize the PTY to the given dimensions and signal the child with SIGWINCH.
    pub fn resize(&self, rows: u16, cols: u16) {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(size);
        }
        if let Ok(mut s) = self.current_size.lock() {
            *s = size;
        }
    }

    /// Send a SIGWINCH (window resize) to the child process to force a full-screen redraw.
    ///
    /// Useful when a new `streamSessionTerminalIO` subscriber connects and has missed the
    /// initial render: after subscribing, call this so claude repaints to the live channel.
    pub fn trigger_redraw(&self) {
        if let Ok(m) = self.master.lock() {
            let size = self.current_size.lock().map(|s| *s).unwrap_or(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            });
            let _ = m.resize(size);
        }
    }

    /// Forward input data to the PTY stdin, stripping any embedded resize escape sequence.
    ///
    /// When `\x1b]resize;{cols};{rows}\x07` is found, the PTY is resized (SIGWINCH sent)
    /// and the escape bytes are not forwarded to the subprocess. Used by both the bidi and
    /// unary input paths so resize always works regardless of which transport the client uses.
    pub fn send_input(&self, data: bytes::Bytes) {
        let (resize, remaining) = strip_resize(&data);
        if let Some((cols, rows)) = resize {
            self.resize(rows, cols);
        }
        if !remaining.is_empty() {
            let _ = self.stdin_tx.send(remaining);
        }
    }
}

/// The outcome of a [`ClaudeCliSessionManager::claim_control`] call.
pub enum ClaimOutcome {
    /// The caller is now the controller. `control_token` must be presented in subsequent control RPCs.
    Granted { control_token: String },
    /// Another screen holds the lease. `holder_screen_id` identifies them.
    Denied { holder_screen_id: String },
}

/// Per-session control lease snapshot.
#[derive(Debug, Clone)]
pub struct ControlLeaseInfo {
    pub control_token: String,
    pub holder_screen_id: String,
}

/// Broadcast payload emitted when a session's control lease changes.
#[derive(Debug, Clone)]
pub struct ControlChangeEvent {
    pub session_id: String,
    pub holder_screen_id: String,
}

/// Registry of active session tools: `session_id → (terminal_id → PtyHandle)`.
///
/// Each session may run multiple identified tools — the main `claude` terminal under
/// [`MAIN_TERMINAL_ID`] plus additional Bash tools under fresh ids.
type TerminalRegistry = Arc<RwLock<HashMap<String, HashMap<String, Arc<PtyHandle>>>>>;

/// Per-session control leases: `session_id → ControlLeaseInfo`.
type ControlRegistry = Arc<RwLock<HashMap<String, ControlLeaseInfo>>>;

/// Manages the [`TerminalRegistry`] of active session tools and the per-session control leases.
pub struct ClaudeCliSessionManager {
    registry: TerminalRegistry,
    /// Exclusive control lease per session.
    control: ControlRegistry,
    /// Fan-out channel for control-change events. Subscribers call [`Self::subscribe_control`].
    control_tx: broadcast::Sender<ControlChangeEvent>,
}

impl Default for ClaudeCliSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCliSessionManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (control_tx, _) = broadcast::channel(64);
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            control: Arc::new(RwLock::new(HashMap::new())),
            control_tx,
        }
    }

    /// Build the argv for the `claude` process.
    ///
    /// Exported so tests can assert the argument list without spawning a real PTY.
    ///
    /// Arg order: `[binary, "--model", model, "--session-id", id, "--permission-mode", mode, prompt?]`.
    /// `model` is omitted when empty. `initial_prompt` is appended as a positional arg only
    /// when non-empty (trimmed); an empty/whitespace prompt is treated as absent so the
    /// process is started interactively without an injected first turn.
    /// `permission_mode` defaults to `"auto"` when `None` or empty/whitespace.
    pub fn build_claude_argv(
        binary_path: &str,
        model: &str,
        session_id: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
    ) -> Vec<String> {
        let mut argv = vec![binary_path.to_string()];
        if !model.is_empty() {
            argv.push("--model".to_string());
            argv.push(model.to_string());
        }
        argv.push("--session-id".to_string());
        argv.push(session_id.to_string());
        let mode = permission_mode
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("auto");
        argv.push("--permission-mode".to_string());
        argv.push(mode.to_string());
        if let Some(p) = initial_prompt {
            let p = p.trim();
            if !p.is_empty() {
                argv.push(p.to_string());
            }
        }
        argv
    }

    /// Spawn a new claude CLI process for `session_id` in `worktree_path`.
    ///
    /// Returns an `Arc<PtyHandle>` on success. The child process is monitored in a background
    /// std thread; when it exits the session is removed from the registry.
    ///
    /// `initial_prompt` — when `Some` and non-empty, appended as a positional CLI argument so
    /// that `claude` receives it as the first user turn. Pass `None` (or `Some("")`) for an
    /// interactive session with no seeded prompt. **Resume** (`resume()`) always passes `None`
    /// because the session is continued via `--session-id`; re-injecting the original prompt
    /// would create a duplicate user turn.
    ///
    /// `permission_mode` — forwarded as `--permission-mode <mode>` to the claude binary.
    /// `None` or empty/whitespace defaults to `"auto"`.
    pub async fn start(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let argv = Self::build_claude_argv(
            binary_path,
            model,
            session_id,
            initial_prompt,
            permission_mode,
        );
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "claude-cli",
            worktree_path,
            model,
            argv,
        )
        .await
    }

    /// Spawn `argv` in a PTY as an identified tool and register it under `(session_id, terminal_id)`.
    ///
    /// Shared by [`start`](Self::start) (the `claude` tool) and
    /// [`start_terminal`](Self::start_terminal) (Bash tools).
    async fn spawn_tool(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let (stdout_tx, _stdout_rx) = broadcast::channel::<Bytes>(OUTPUT_BROADCAST_CAPACITY);
        let capture = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

        let session_id_owned = session_id.to_string();
        let terminal_id_owned = terminal_id.to_string();
        let worktree_clone = worktree_path.clone();
        let stdout_tx_clone = stdout_tx.clone();
        let capture_clone = Arc::clone(&capture);
        let reg = Arc::clone(&self.registry);

        // portable-pty I/O is blocking; spawn everything from a dedicated OS thread.
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let res = Self::spawn_in_pty(
                &session_id_owned,
                &terminal_id_owned,
                worktree_clone,
                argv,
                stdin_rx,
                stdout_tx_clone,
                capture_clone,
                reg,
            );
            let _ = result_tx.send(res);
        });

        let (pid, master, pty_done) = result_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY spawn thread did not respond"))??;

        let handle = Arc::new(PtyHandle {
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            worktree_path,
            model: model.to_string(),
            stdin_tx,
            stdout_tx,
            capture,
            pid,
            master,
            pty_done,
            current_size: Arc::new(std::sync::Mutex::new(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })),
        });

        // Insert into the registry BEFORE returning so that a racing streamSessionTerminalIO
        // call (browser connects immediately after startSession returns) can find the handle.
        self.registry
            .write()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(terminal_id.to_string(), Arc::clone(&handle));

        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "spawn_tool: registered session={} terminal={} kind={} pid={}",
            session_id, terminal_id, kind, handle.pid
        );

        Ok(handle)
    }

    /// Resume (relaunch) an existing session by spawning a new process in the same worktree.
    ///
    /// Always passes `initial_prompt = None`: a resumed session continues via `--session-id`
    /// and must not replay the original prompt (that would inject a duplicate user turn).
    pub async fn resume(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        // Start a fresh process in the same worktree; never replay the initial prompt on resume,
        // and never carry over a prior permission_mode (resume always uses the default "auto").
        self.start(session_id, worktree_path, model, binary_path, None, None)
            .await
    }

    /// Look up the **main** (`claude`) terminal of a session by id.
    ///
    /// Back-compat convenience: equivalent to `get_terminal(session_id, MAIN_TERMINAL_ID)`.
    pub async fn get(&self, session_id: &str) -> Option<Arc<PtyHandle>> {
        self.get_terminal(session_id, MAIN_TERMINAL_ID).await
    }

    /// Start a **Bash tool** attached to `session_id`: a shell (`shell_path`, resolved from
    /// `$SHELL` at the RPC layer) in `worktree_path`, taking no inputs. Returns the new handle with
    /// a fresh `terminal_id` (never the reserved `MAIN_TERMINAL_ID`) and kind `"bash"`.
    pub async fn start_terminal(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        shell_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let argv = vec![shell_path.to_string()];
        self.spawn_tool(session_id, &terminal_id, "bash", worktree_path, "", argv)
            .await
    }

    /// Look up a specific tool of a session by `terminal_id` (use `MAIN_TERMINAL_ID` for the
    /// `claude` terminal).
    pub async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<PtyHandle>> {
        self.registry
            .read()
            .await
            .get(session_id)
            .and_then(|tools| tools.get(terminal_id).cloned())
    }

    /// List all running tools of a session, including the `MAIN_TERMINAL_ID` terminal.
    pub async fn list_terminals(&self, session_id: &str) -> Vec<Arc<PtyHandle>> {
        self.registry
            .read()
            .await
            .get(session_id)
            .map(|tools| tools.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Stop a started tool: terminate its process and remove it from the registry.
    /// Returns `true` if the tool existed. The reserved `MAIN_TERMINAL_ID` is not stoppable
    /// here (callers must reject it before calling).
    pub async fn stop_terminal(&self, session_id: &str, terminal_id: &str) -> bool {
        let handle = {
            let mut reg = self.registry.write().await;
            let removed = reg
                .get_mut(session_id)
                .and_then(|tools| tools.remove(terminal_id));
            // Drop the session entry once its last tool is gone.
            if reg.get(session_id).is_some_and(|tools| tools.is_empty()) {
                reg.remove(session_id);
            }
            removed
        };

        match handle {
            Some(handle) => {
                // POSIX interactive shells ignore SIGTERM, so escalate to SIGKILL to guarantee the
                // process exits. `signal_pid` treats an already-dead pid (ESRCH) as success.
                #[cfg(unix)]
                {
                    let pid = handle.pid as i32;
                    let _ = crate::session_deletion::signal_pid(pid, libc::SIGTERM);
                    let _ = crate::session_deletion::signal_pid(pid, libc::SIGKILL);
                }
                true
            }
            None => false,
        }
    }

    /// Stop all PTY terminals belonging to `session_id`: SIGTERM each and remove from registry.
    ///
    /// Called when a session ends (natural exit or explicit termination) so its processes don't
    /// outlive the session. This is the per-session counterpart to [`kill_all`].
    pub async fn stop_session(&self, session_id: &str) {
        let handles = {
            let mut reg = self.registry.write().await;
            reg.remove(session_id).unwrap_or_default()
        };

        if handles.is_empty() {
            return;
        }

        #[cfg(unix)]
        for (terminal_id, handle) in &handles {
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "stop_session: session={} terminal={} pid={} — sending SIGTERM",
                session_id, terminal_id, handle.pid
            );
            let _ = crate::session_deletion::signal_pid(handle.pid as i32, libc::SIGTERM);
        }
    }

    /// Kill all tracked PTY processes across every session: SIGTERM each, wait up to 5 s,
    /// then SIGKILL any that remain. Clears the registry on completion.
    ///
    /// Called during daemon shutdown so that spawned `claude` / shell processes do not
    /// outlive the daemon as orphans.
    pub async fn kill_all(&self) {
        let pids: Vec<u32> = {
            let mut reg = self.registry.write().await;
            let pids = reg
                .values()
                .flat_map(|tools| tools.values().map(|h| h.pid))
                .collect();
            reg.clear();
            pids
        };

        if pids.is_empty() {
            log::debug!(
                target: "tddy_daemon::claude_cli_session",
                "kill_all: no registered sessions — nothing to terminate"
            );
            return;
        }

        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: sending SIGTERM to {} process(es): {:?}",
            pids.len(),
            pids
        );

        tokio::task::spawn_blocking(move || {
            #[cfg(unix)]
            {
                for &pid in &pids {
                    let _ = crate::session_deletion::signal_pid(pid as i32, libc::SIGTERM);
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while std::time::Instant::now() < deadline {
                    let any_alive = pids
                        .iter()
                        .any(|&pid| unsafe { libc::kill(pid as i32, 0) } == 0);
                    if !any_alive {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let still_alive: Vec<u32> = pids
                    .iter()
                    .copied()
                    .filter(|&pid| unsafe { libc::kill(pid as i32, 0) } == 0)
                    .collect();
                if still_alive.is_empty() {
                    log::info!(
                        target: "tddy_daemon::claude_cli_session",
                        "kill_all: all processes exited cleanly after SIGTERM"
                    );
                } else {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "kill_all: {} process(es) still alive after 5 s — sending SIGKILL: {:?}",
                        still_alive.len(),
                        still_alive
                    );
                    for &pid in &still_alive {
                        let _ = crate::session_deletion::signal_pid(pid as i32, libc::SIGKILL);
                    }
                }
            }
        })
        .await
        .ok();
    }

    // --- terminal control lease (single-screen mutex) ---

    /// Attempt to claim exclusive input control of a session's terminals.
    ///
    /// - `steal = false`: grants only when unheld or already held by `screen_id`.
    /// - `steal = true`: always grants, evicting the previous holder and broadcasting a
    ///   [`ControlChangeEvent`] to all [`Self::subscribe_control`] subscribers.
    ///
    /// Returns a [`ClaimOutcome`] the RPC handler maps to [`ClaimTerminalControlResponse`].
    pub async fn claim_control(
        &self,
        session_id: &str,
        screen_id: &str,
        steal: bool,
    ) -> ClaimOutcome {
        let mut control = self.control.write().await;
        match control.get(session_id) {
            None => {
                let token = uuid::Uuid::new_v4().to_string();
                control.insert(
                    session_id.to_string(),
                    ControlLeaseInfo {
                        control_token: token.clone(),
                        holder_screen_id: screen_id.to_string(),
                    },
                );
                ClaimOutcome::Granted {
                    control_token: token,
                }
            }
            Some(lease) if lease.holder_screen_id == screen_id => ClaimOutcome::Granted {
                control_token: lease.control_token.clone(),
            },
            Some(lease) if !steal => ClaimOutcome::Denied {
                holder_screen_id: lease.holder_screen_id.clone(),
            },
            Some(_) => {
                let token = uuid::Uuid::new_v4().to_string();
                control.insert(
                    session_id.to_string(),
                    ControlLeaseInfo {
                        control_token: token.clone(),
                        holder_screen_id: screen_id.to_string(),
                    },
                );
                drop(control);
                let _ = self.control_tx.send(ControlChangeEvent {
                    session_id: session_id.to_string(),
                    holder_screen_id: screen_id.to_string(),
                });
                ClaimOutcome::Granted {
                    control_token: token,
                }
            }
        }
    }

    /// Return `true` iff `control_token` matches the active control lease for `session_id`.
    ///
    /// An empty `control_token` is accepted when the session has no active lease (uncontrolled).
    /// A session with no active lease is considered uncontrolled: all inputs are accepted.
    pub async fn verify_control(&self, session_id: &str, control_token: &str) -> bool {
        let control = self.control.read().await;
        match control.get(session_id) {
            None => true,
            Some(lease) => lease.control_token == control_token,
        }
    }

    /// Return the current control lease for `session_id`, or `None` if uncontrolled.
    pub async fn current_control(&self, session_id: &str) -> Option<ControlLeaseInfo> {
        let control = self.control.read().await;
        control.get(session_id).cloned()
    }

    /// Subscribe to control-change events across all sessions.
    ///
    /// Each [`ControlChangeEvent`] identifies the affected `session_id` and the new holder's
    /// `holder_screen_id`. The displaced screen should render the "Claim terminal" CTA.
    pub fn subscribe_control(&self) -> broadcast::Receiver<ControlChangeEvent> {
        self.control_tx.subscribe()
    }

    // --- private helpers ---

    /// Spawn the child in a PTY. Runs in a dedicated OS thread (portable-pty I/O is blocking).
    ///
    /// Returns `(pid, Arc<Mutex<MasterPty>>)` on success. Launches background threads for I/O
    /// forwarding and exit monitoring. The registry `Arc` is used for cleanup on exit.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn spawn_in_pty(
        session_id: &str,
        terminal_id: &str,
        worktree_path: PathBuf,
        argv: Vec<String>,
        stdin_rx: mpsc::UnboundedReceiver<Bytes>,
        stdout_tx: broadcast::Sender<Bytes>,
        capture: Arc<std::sync::Mutex<Vec<u8>>>,
        reg: TerminalRegistry,
    ) -> anyhow::Result<(
        u32,
        Arc<std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
        watch::Receiver<bool>,
    )> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_TERM_ROWS,
                cols: DEFAULT_TERM_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow::anyhow!("openpty failed: {}", e))?;

        // `argv` is prebuilt by the caller (claude argv via `build_claude_argv`, or `[shell]` for a
        // Bash tool) so the command is testable without a real PTY.
        let mut cmd = CommandBuilder::new(&argv[0]);
        for arg in &argv[1..] {
            cmd.arg(arg);
        }
        cmd.cwd(&worktree_path);
        // Ensure the child sees a proper terminal type for TUI rendering.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        // Spawn the child on the slave side. The slave is consumed/closed after spawn so the
        // master sees EOF when the child exits.
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow::anyhow!("failed to spawn tool {:?}: {}", argv.first(), e))?;
        // Drop slave so master sees EOF on child exit.
        drop(pair.slave);

        let pid = child
            .process_id()
            .ok_or_else(|| anyhow::anyhow!("spawned child has no pid"))?;

        let master = Arc::new(std::sync::Mutex::new(pair.master));

        // Watch channel: reader thread holds the sender; drops it on exit so receivers know
        // the PTY is done and no more output will arrive.
        let (pty_done_tx, pty_done_rx) = watch::channel(false);

        // Reader thread: PTY master → capture buffer + broadcast channel.
        let master_for_reader = Arc::clone(&master);
        let stdout_tx_reader = stdout_tx.clone();
        std::thread::spawn(move || {
            let reader = {
                let m = master_for_reader.lock().unwrap();
                m.try_clone_reader()
            };
            match reader {
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "PTY reader: try_clone_reader failed: {}",
                        e
                    );
                }
                Ok(mut r) => {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match std::io::Read::read(&mut r, &mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                log::trace!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "PTY output: {} bytes: {:?}",
                                    n,
                                    String::from_utf8_lossy(&buf[..n])
                                );
                                // Append to the capture ring (trimmed to CAPTURE_LIMIT_BYTES)
                                // so late-connecting subscribers can replay all output so far.
                                if let Ok(mut cap) = capture.lock() {
                                    cap.extend_from_slice(&buf[..n]);
                                    if cap.len() > CAPTURE_LIMIT_BYTES {
                                        let excess = cap.len() - CAPTURE_LIMIT_BYTES;
                                        cap.drain(0..excess);
                                        log::debug!(
                                            target: "tddy_daemon::claude_cli_session",
                                            "PTY capture trimmed: dropped {} bytes, buffer={} bytes",
                                            excess,
                                            cap.len()
                                        );
                                    }
                                }
                                let chunk = Bytes::copy_from_slice(&buf[..n]);
                                let _ = stdout_tx_reader.send(chunk);
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            // Signal that no more output will be produced. Dropping the sender is equivalent
            // to sending `true` — receivers see the channel closed.
            drop(pty_done_tx);
        });

        // Writer thread: mpsc channel → PTY master.
        let master_for_writer = Arc::clone(&master);
        let mut stdin_rx_thread = stdin_rx;
        std::thread::spawn(move || {
            let writer = {
                let m = master_for_writer.lock().unwrap();
                m.take_writer()
            };
            match writer {
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "PTY writer: take_writer failed: {}",
                        e
                    );
                }
                Ok(mut w) => {
                    // Use a blocking recv loop (we're already on a std thread).
                    let rt = tokio::runtime::Handle::try_current();
                    // If inside a tokio context, use block_in_place; otherwise create a local runtime.
                    if let Ok(handle) = rt {
                        tokio::task::block_in_place(|| {
                            loop {
                                // block_on a single recv at a time
                                let data = handle.block_on(stdin_rx_thread.recv());
                                match data {
                                    None => break,
                                    Some(bytes) => {
                                        log::trace!(
                                            target: "tddy_daemon::claude_cli_session",
                                            "PTY input: {} bytes: {:?}",
                                            bytes.len(),
                                            String::from_utf8_lossy(&bytes)
                                        );
                                        if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        });
                    } else {
                        // No tokio context — use a simple busy approach
                        while let Some(bytes) = stdin_rx_thread.blocking_recv() {
                            log::trace!(
                                target: "tddy_daemon::claude_cli_session",
                                "PTY input: {} bytes: {:?}",
                                bytes.len(),
                                String::from_utf8_lossy(&bytes)
                            );
                            if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Exit monitor thread: wait for child exit → remove this tool from the registry.
        let sid = session_id.to_string();
        let tid = terminal_id.to_string();
        let mut child_monitor = child;
        std::thread::spawn(move || {
            if let Err(e) = child_monitor.wait() {
                log::debug!(
                    target: "tddy_daemon::claude_cli_session",
                    "child wait error for session {} terminal {}: {}",
                    sid,
                    tid,
                    e
                );
            }
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "tool process exited for session {} terminal {}",
                sid,
                tid
            );
            // Remove from registry using a lightweight tokio runtime block. Dropping the session
            // entry once its last tool is gone is idempotent with `stop_terminal`.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let reg_clone = Arc::clone(&reg);
                let sid_clone = sid.clone();
                let tid_clone = tid.clone();
                handle.spawn(async move {
                    let mut reg = reg_clone.write().await;
                    if let Some(tools) = reg.get_mut(&sid_clone) {
                        tools.remove(&tid_clone);
                        if tools.is_empty() {
                            reg.remove(&sid_clone);
                        }
                    }
                });
            }
        });

        Ok((pid, master, pty_done_rx))
    }
}

// ---------------------------------------------------------------------------
// Resize escape parsing
// ---------------------------------------------------------------------------

/// Strip an OSC resize sequence (`\x1b]resize;{cols};{rows}\x07`) from `data`.
///
/// Returns `(Some((cols, rows)), remaining)` when found, or `(None, original)` otherwise.
/// The escape sequence is removed from the returned bytes so it is not forwarded to the PTY stdin.
fn strip_resize(data: &[u8]) -> (Option<(u16, u16)>, Bytes) {
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

// ---------------------------------------------------------------------------
// LiveKit bridge: expose a PtyHandle as a LiveKit RPC server
// ---------------------------------------------------------------------------

/// LiveKit RPC service that bridges `terminal.TerminalService/StreamTerminalIO` to a PTY handle.
struct PtyLiveKitService {
    handle: Arc<PtyHandle>,
}

#[async_trait]
impl RpcService for PtyLiveKitService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "terminal.TerminalService" && method == "StreamTerminalIO"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, _msg: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(tddy_rpc::Status::unimplemented("use bidi stream")))
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, tddy_rpc::Status> {
        if service != "terminal.TerminalService" || method != "StreamTerminalIO" {
            return Err(tddy_rpc::Status::not_found(format!(
                "{}/{}",
                service, method
            )));
        }

        let (out_tx, out_rx) = mpsc::channel::<Result<Vec<u8>, tddy_rpc::Status>>(256);

        // Replay capture buffer so the client sees all output since session start.
        if let Ok(cap) = self.handle.capture.lock() {
            if !cap.is_empty() {
                let frame = TerminalOutput { data: cap.clone() }.encode_to_vec();
                let _ = out_tx.try_send(Ok(frame));
            }
        }

        // PTY stdout → bidi output stream. Breaks when:
        // - broadcast sender closed (session dropped)
        // - pty_done watch fires (reader exited; no more output coming)
        // - out_tx_clone send fails (client disconnected)
        let mut stdout_rx = self.handle.stdout_tx.subscribe();
        let mut pty_done = self.handle.pty_done.clone();
        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stdout_rx.recv() => {
                        match result {
                            Ok(bytes) => {
                                let frame = TerminalOutput { data: bytes.to_vec() }.encode_to_vec();
                                if out_tx_clone.send(Ok(frame)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "LiveKit bridge: PTY output lagged {} messages",
                                    n
                                );
                            }
                        }
                    }
                    // PTY reader exited — drain any remaining broadcast items then stop.
                    _ = pty_done.changed() => break,
                }
            }
        });

        // Bidi input stream → PTY stdin. Resize escape sequences are intercepted and applied
        // to the PTY via SIGWINCH rather than forwarded as raw bytes.
        let handle_for_input = Arc::clone(&self.handle);
        tokio::spawn(async move {
            while let Some(msg) = input_rx.recv().await {
                if let Ok(input) = TerminalInput::decode(&msg.payload[..]) {
                    if !input.data.is_empty() {
                        handle_for_input.send_input(bytes::Bytes::from(input.data));
                    }
                }
            }
        });

        // Trigger a SIGWINCH so the TUI repaints for the new subscriber.
        self.handle.trigger_redraw();

        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(out_rx),
        })
    }
}

/// Spawn a LiveKit participant that bridges a PTY session to the LiveKit room.
///
/// Returns the identity string used (`daemon-<instance_id>-<session_id>` or
/// `daemon-<session_id>`), which the caller should return in `StartSessionResponse`.
pub async fn spawn_livekit_bridge(
    handle: Arc<PtyHandle>,
    livekit_url: &str,
    room_name: &str,
    api_key: &str,
    api_secret: &str,
    server_identity: &str,
) -> anyhow::Result<()> {
    let token = TokenGenerator::new(
        api_key.to_string(),
        api_secret.to_string(),
        room_name.to_string(),
        server_identity.to_string(),
        std::time::Duration::from_secs(86400),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("token generate: {}", e))?;

    let service = PtyLiveKitService { handle };
    let participant =
        LiveKitParticipant::connect(livekit_url, &token, service, Default::default(), None, None)
            .await
            .map_err(|e| anyhow::anyhow!("LiveKitParticipant::connect: {}", e))?;

    let identity_owned = server_identity.to_string();
    tokio::spawn(async move {
        participant.run().await;
        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "LiveKit bridge participant exited for identity {}",
            identity_owned
        );
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid_is_alive(pid: u32) -> bool {
        // kill -0 checks existence without sending a signal; ESRCH means dead
        let ret = unsafe { libc::kill(pid as i32, 0) };
        ret == 0
    }

    fn wait_for_pid_to_die(pid: u32, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if !pid_is_alive(pid) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        !pid_is_alive(pid)
    }

    /// kill_all terminates a running PTY process registered under a session.
    ///
    /// AC: After kill_all() returns, the process is dead (kill -0 → ESRCH).
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_terminates_registered_pty_processes() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        let handle = manager
            .start_terminal(
                "kill-all-test-session-1",
                worktree.path().to_path_buf(),
                "/bin/sh",
            )
            .await
            .expect("start_terminal should spawn a PTY process");
        let pid = handle.pid;

        assert!(pid_is_alive(pid), "process should be alive before kill_all");

        manager.kill_all().await;

        assert!(
            wait_for_pid_to_die(pid, std::time::Duration::from_secs(5)),
            "process pid={pid} should be dead after kill_all"
        );
    }

    /// kill_all kills all sessions when multiple are tracked simultaneously.
    ///
    /// AC: Every PID in every registered session is dead after kill_all().
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_terminates_all_sessions() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");
        let mut pids = Vec::new();

        for i in 0..3 {
            let session_id = format!("kill-all-multi-{i}");
            let handle = manager
                .start_terminal(&session_id, worktree.path().to_path_buf(), "/bin/sh")
                .await
                .expect("start_terminal should succeed");
            pids.push(handle.pid);
        }

        for &pid in &pids {
            assert!(
                pid_is_alive(pid),
                "pid {pid} should be alive before kill_all"
            );
        }

        manager.kill_all().await;

        for &pid in &pids {
            assert!(
                wait_for_pid_to_die(pid, std::time::Duration::from_secs(5)),
                "pid {pid} should be dead after kill_all"
            );
        }
    }

    /// kill_all empties the registry so subsequent lookups return nothing.
    ///
    /// AC: list_terminals returns empty for all sessions after kill_all().
    #[tokio::test]
    #[cfg(unix)]
    async fn kill_all_clears_the_registry() {
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        manager
            .start_terminal("registry-clear-a", worktree.path().to_path_buf(), "/bin/sh")
            .await
            .expect("start session a");
        manager
            .start_terminal("registry-clear-b", worktree.path().to_path_buf(), "/bin/sh")
            .await
            .expect("start session b");

        assert!(
            !manager.list_terminals("registry-clear-a").await.is_empty(),
            "session-a should have terminals before kill_all"
        );

        manager.kill_all().await;

        assert!(
            manager.list_terminals("registry-clear-a").await.is_empty(),
            "session-a should have no terminals after kill_all"
        );
        assert!(
            manager.list_terminals("registry-clear-b").await.is_empty(),
            "session-b should have no terminals after kill_all"
        );
    }

    /// kill_all on an empty manager is safe and does not panic.
    ///
    /// AC: No panic or error on a freshly constructed manager.
    #[tokio::test]
    async fn kill_all_is_safe_on_empty_manager() {
        let manager = ClaudeCliSessionManager::new();
        manager.kill_all().await; // must not panic
    }
}
