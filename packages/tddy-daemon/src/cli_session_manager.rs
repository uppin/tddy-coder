//! CLI session manager — spawns agent CLIs (`claude`, Cursor `agent`) in PTYs and plumbs I/O via
//! tokio channels. Shared by `claude-cli` and `cursor-cli` session types (`CliSessionManager`).
//!
//! Uses `portable-pty` so TUI agents see a controlling terminal (TTY). Without a PTY, the child
//! detects no TTY and exits immediately.
//!
//! Historical import path `claude_cli_session` re-exports this module for compatibility.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::PtySize;
use prost::Message as _;
use tddy_livekit::{LiveKitParticipant, RpcResult, RpcService, TokenGenerator};
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tddy_task::{TaskHandle, TaskId, TaskRegistry};
use tokio::sync::{broadcast, mpsc, oneshot, watch, RwLock};

use crate::pty_registry::PtyRegistry;
use crate::pty_runtime::{
    PtyReady, PtyRuntime, PtySpawnSpec, DEFAULT_TERM_COLS, DEFAULT_TERM_ROWS,
};

/// Reserved terminal id for the original `claude` terminal of a session.
///
/// Every terminal in a session is identified; the `claude` terminal spawned at session start is
/// always addressable under this id, while started shell terminals receive fresh unique ids.
pub const MAIN_TERMINAL_ID: &str = "main";

/// Handle to a running process in a PTY (the `claude` CLI for the main terminal, or a login shell
/// for started terminals).
///
/// Backed by a [`TaskHandle`] in the shared [`TaskRegistry`]; I/O is plumbed through the task's
/// PTY channel and resize control lives in [`PtyRegistry`].
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
    /// Becomes true (and sender drops) when the task reaches a terminal status.
    pub pty_done: watch::Receiver<bool>,
    /// Current PTY dimensions, updated by `resize()`.
    current_size: Arc<std::sync::Mutex<PtySize>>,
    /// Owning task in the shared registry.
    task_id: TaskId,
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

/// Per-terminal metadata: maps a terminal id to its backing task.
#[derive(Debug, Clone)]
struct TerminalEntry {
    task_id: TaskId,
    worktree_path: PathBuf,
    model: String,
}

/// `session_id → (terminal_id → TerminalEntry)`.
type TerminalIndex = Arc<RwLock<HashMap<String, HashMap<String, TerminalEntry>>>>;

/// Per-session control leases: `session_id → ControlLeaseInfo`.
type ControlRegistry = Arc<RwLock<HashMap<String, ControlLeaseInfo>>>;

/// Per-session managed-workflow wiring (kept alive for the session's lifetime): `session_id → ManagedWorkflow`.
type ManagedWorkflowRegistry =
    Arc<RwLock<HashMap<String, crate::session_toolcall::ManagedWorkflow>>>;

/// Manages PTY session tools via the shared [`TaskRegistry`] and per-session control leases.
pub struct CliSessionManager {
    task_registry: TaskRegistry,
    pty_registry: PtyRegistry,
    /// Maps `(session_id, terminal_id)` to the backing task.
    terminals: TerminalIndex,
    /// Exclusive control lease per session.
    control: ControlRegistry,
    /// Fan-out channel for control-change events. Subscribers call [`Self::subscribe_control`].
    control_tx: broadcast::Sender<ControlChangeEvent>,
    /// Managed-workflow wiring per session — its toolcall listener + controller must outlive the
    /// spawned process, so the manager owns it and drops it when the main terminal exits.
    managed_workflows: ManagedWorkflowRegistry,
}

/// Backward-compatible alias for [`CliSessionManager`].
pub type ClaudeCliSessionManager = CliSessionManager;

impl Default for CliSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CliSessionManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::with_task_registry(TaskRegistry::new())
    }

    /// Create a manager sharing the given [`TaskRegistry`] (used by `ConnectionServiceImpl`).
    pub fn with_task_registry(task_registry: TaskRegistry) -> Self {
        let (control_tx, _) = broadcast::channel(64);
        Self {
            task_registry,
            pty_registry: PtyRegistry::new(),
            terminals: Arc::new(RwLock::new(HashMap::new())),
            control: Arc::new(RwLock::new(HashMap::new())),
            control_tx,
            managed_workflows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Attach a session's [`ManagedWorkflow`](crate::session_toolcall::ManagedWorkflow) so its
    /// toolcall listener + controller stay alive for the session's lifetime. Dropped when the
    /// session's main terminal exits (see `spawn_terminal_cleanup`).
    pub async fn attach_managed_workflow(
        &self,
        session_id: &str,
        managed: crate::session_toolcall::ManagedWorkflow,
    ) {
        self.managed_workflows
            .write()
            .await
            .insert(session_id.to_string(), managed);
    }

    /// Shared task registry — PTY tools and fast tools use the same instance.
    pub fn task_registry(&self) -> TaskRegistry {
        self.task_registry.clone()
    }

    /// Build the argv for the `claude` process.
    ///
    /// Exported so tests can assert the argument list without spawning a real PTY.
    ///
    /// Arg order: `[binary, "--model", model, <session flag>, id, "--permission-mode", mode, prompt?]`.
    /// `model` is omitted when empty. The session flag is `--resume <id>` when `resume` is true
    /// (continue an existing on-disk transcript) and `--session-id <id>` otherwise (assign the id to
    /// a fresh session). `initial_prompt` is appended as a positional arg only when non-empty
    /// (trimmed); an empty/whitespace prompt is treated as absent so the process is started
    /// interactively without an injected first turn. `permission_mode` defaults to `"auto"` when
    /// `None` or empty/whitespace.
    ///
    /// The base argv (binary, model, session flag, permission mode) is built by the shared
    /// [`tddy_core::claude_argv::build_claude_base_argv`] so this path and the sandboxed runner
    /// stay in lockstep. A managed workflow's `--append-system-prompt-file` is inserted by
    /// [`Self::start_with_options`] (before any positional prompt), not here, so this builder stays
    /// focused on the base argv plus the positional prompt.
    pub fn build_claude_argv(
        binary_path: &str,
        model: &str,
        session_id: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
    ) -> Vec<String> {
        let mode = permission_mode
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("auto");
        let mut argv = tddy_core::claude_argv::build_claude_base_argv(
            binary_path,
            model,
            session_id,
            mode,
            dangerously_skip_permissions,
            resume,
        );
        if let Some(p) = initial_prompt {
            let p = p.trim();
            if !p.is_empty() {
                argv.push(p.to_string());
            }
        }
        argv
    }

    /// Build the argv for the Cursor Agent CLI process.
    ///
    /// Arg order: `[binary, "--model", model, prompt?]`. `model` is omitted when empty.
    /// `initial_prompt` is appended as a positional arg when non-empty (trimmed).
    pub fn build_cursor_argv(
        binary_path: &str,
        model: &str,
        initial_prompt: Option<&str>,
    ) -> Vec<String> {
        let mut argv = vec![binary_path.to_string()];
        if !model.is_empty() {
            argv.push("--model".to_string());
            argv.push(model.to_string());
        }
        if let Some(p) = initial_prompt {
            let p = p.trim();
            if !p.is_empty() {
                argv.push(p.to_string());
            }
        }
        argv
    }

    /// Spawn a new Cursor Agent CLI process for `session_id` in `worktree_path`.
    pub async fn start_cursor(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        initial_prompt: Option<&str>,
        // Extra per-session env pairs applied to the cursor process (e.g. `TDDY_SEMANTIC_INDEX_DB`).
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let argv = Self::build_cursor_argv(binary_path, model, initial_prompt);
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "cursor-cli",
            worktree_path,
            model,
            argv,
            env,
            None,
        )
        .await
    }

    /// Resume a Cursor CLI session (never replays the initial prompt).
    pub async fn resume_cursor(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        self.start_cursor(
            session_id,
            worktree_path,
            model,
            binary_path,
            None,
            Vec::new(),
        )
        .await
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
        self.start_with_options(
            session_id,
            worktree_path,
            model,
            binary_path,
            initial_prompt,
            permission_mode,
            false,
            false,
            None,
            Vec::new(),
            None,
        )
        .await
    }

    /// Like [`Self::start`], plus managed-workflow launch options.
    ///
    /// `resume` — when true, continue the existing on-disk transcript via `--resume <id>` instead of
    /// assigning the id to a fresh session via `--session-id <id>`. Only [`Self::resume_with_options`]
    /// passes true; fresh starts pass false.
    ///
    /// `append_system_prompt_file` — when `Some`, inserted as `--append-system-prompt-file <path>`
    /// (before any positional prompt) so `claude` appends that file to its system prompt (a managed
    /// workflow's orchestration prompt).
    ///
    /// `env` — extra environment variables set on the spawned process (e.g. a managed session's
    /// per-session `TDDY_SOCKET` and a `PATH` that resolves `tddy-tools`).
    #[allow(clippy::too_many_arguments)]
    pub async fn start_with_options(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
        append_system_prompt_file: Option<&Path>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let mut argv = Self::build_claude_argv(
            binary_path,
            model,
            session_id,
            initial_prompt,
            permission_mode,
            dangerously_skip_permissions,
            resume,
        );
        if let Some(path) = append_system_prompt_file {
            // Insert before a trailing positional prompt (if any) so the flag precedes the prompt.
            let has_positional_prompt = initial_prompt.is_some_and(|p| !p.trim().is_empty());
            let insert_at = if has_positional_prompt {
                argv.len() - 1
            } else {
                argv.len()
            };
            argv.splice(
                insert_at..insert_at,
                [
                    "--append-system-prompt-file".to_string(),
                    path.to_string_lossy().into_owned(),
                ],
            );
        }
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "claude-cli",
            worktree_path,
            model,
            argv,
            env,
            os_user,
        )
        .await
    }

    /// Spawn `argv` in a PTY as an identified tool and register it under `(session_id, terminal_id)`.
    ///
    /// Shared by [`start`](Self::start) (the `claude` tool) and
    /// [`start_terminal`](Self::start_terminal) (Bash tools).
    #[allow(clippy::too_many_arguments)]
    async fn spawn_tool(
        &self,
        session_id: &str,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let spec = PtySpawnSpec {
            argv,
            worktree_path: worktree_path.clone(),
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            env,
            os_user: os_user.map(str::to_string),
        };

        let task = PtyRuntime::spawn(&self.task_registry, &self.pty_registry, spec, ready_tx).await;

        let ready = ready_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY runtime did not signal ready"))?
            .map_err(|e| anyhow::anyhow!("PTY spawn failed: {e}"))?;

        let handle = self.build_pty_handle(task, terminal_id, kind, worktree_path, model, ready)?;

        self.terminals
            .write()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(
                terminal_id.to_string(),
                TerminalEntry {
                    task_id: handle.task_id.clone(),
                    worktree_path: handle.worktree_path.clone(),
                    model: handle.model.clone(),
                },
            );

        self.spawn_terminal_cleanup(
            session_id.to_string(),
            terminal_id.to_string(),
            handle.task_id.clone(),
        );

        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "spawn_tool: registered session={} terminal={} kind={} pid={} task_id={}",
            session_id,
            terminal_id,
            kind,
            handle.pid,
            handle.task_id
        );

        Ok(Arc::new(handle))
    }

    fn build_pty_handle(
        &self,
        task: Arc<TaskHandle>,
        terminal_id: &str,
        kind: &str,
        worktree_path: PathBuf,
        model: &str,
        ready: PtyReady,
    ) -> anyhow::Result<PtyHandle> {
        let channel = task
            .channel("0")
            .ok_or_else(|| anyhow::anyhow!("PTY task missing channel 0"))?;
        let stdin_tx = channel
            .stdin_sender()
            .ok_or_else(|| anyhow::anyhow!("PTY channel missing stdin"))?;

        let (pty_done_tx, pty_done_rx) = watch::channel(false);
        let mut status_rx = task.status_watch();
        tokio::spawn(async move {
            loop {
                if status_rx.borrow().is_terminal() {
                    let _ = pty_done_tx.send(true);
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
        });

        Ok(PtyHandle {
            terminal_id: terminal_id.to_string(),
            kind: kind.to_string(),
            worktree_path,
            model: model.to_string(),
            stdin_tx,
            stdout_tx: channel.output_broadcast(),
            capture: channel.capture_arc(),
            pid: ready.pid,
            master: ready.master,
            pty_done: pty_done_rx,
            current_size: ready.current_size,
            task_id: task.id.clone(),
        })
    }

    fn spawn_terminal_cleanup(&self, session_id: String, terminal_id: String, task_id: TaskId) {
        let terminals = Arc::clone(&self.terminals);
        let managed_workflows = Arc::clone(&self.managed_workflows);
        let task_registry = self.task_registry.clone();
        tokio::spawn(async move {
            let task = match task_registry.get(&task_id).await {
                Some(t) => t,
                None => return,
            };
            let mut status_rx = task.status_watch();
            loop {
                if status_rx.borrow().is_terminal() {
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
            let mut reg = terminals.write().await;
            if let Some(tools) = reg.get_mut(&session_id) {
                if tools
                    .get(&terminal_id)
                    .is_some_and(|e| e.task_id == task_id)
                {
                    tools.remove(&terminal_id);
                }
                if tools.is_empty() {
                    reg.remove(&session_id);
                }
            }
            // The main claude terminal exiting ends the session — drop its managed workflow so the
            // per-session toolcall listener socket is cleaned up.
            if terminal_id == MAIN_TERMINAL_ID {
                managed_workflows.write().await.remove(&session_id);
            }
        });
    }

    async fn resolve_pty_handle(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<PtyHandle>> {
        let entry = self
            .terminals
            .read()
            .await
            .get(session_id)?
            .get(terminal_id)?
            .clone();
        let task = self.task_registry.get(&entry.task_id).await?;
        let control = self.pty_registry.get(&entry.task_id).await?;
        let ready = PtyReady {
            pid: task.pid_slot.lock().unwrap().first().copied().unwrap_or(0),
            master: control.master,
            current_size: control.current_size,
        };
        self.build_pty_handle(
            task,
            terminal_id,
            &control.kind,
            entry.worktree_path,
            &entry.model,
            ready,
        )
        .ok()
        .map(Arc::new)
    }

    /// Resume (relaunch) an existing plain session by spawning a new process in the same worktree.
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
        self.resume_with_options(
            session_id,
            worktree_path,
            model,
            binary_path,
            None,
            Vec::new(),
        )
        .await
    }

    /// Resume with managed-workflow options: like [`Self::resume`] but re-wires the orchestration
    /// prompt (`append_system_prompt_file`) and per-session env (`TDDY_SOCKET` + `PATH`) so a resumed
    /// managed session stays workflow-aware. Never replays the initial prompt and never carries over
    /// a prior permission mode (resume uses the default "auto").
    pub async fn resume_with_options(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        append_system_prompt_file: Option<&Path>,
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        self.start_with_options(
            session_id,
            worktree_path,
            model,
            binary_path,
            None,
            None,
            false,
            true,
            append_system_prompt_file,
            env,
            None,
        )
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
        self.spawn_tool(
            session_id,
            &terminal_id,
            "bash",
            worktree_path,
            "",
            argv,
            Vec::new(),
            None,
        )
        .await
    }

    /// Look up a specific tool of a session by `terminal_id` (use `MAIN_TERMINAL_ID` for the
    /// `claude` terminal).
    pub async fn get_terminal(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<PtyHandle>> {
        self.resolve_pty_handle(session_id, terminal_id).await
    }

    /// List all running tools of a session, including the `MAIN_TERMINAL_ID` terminal.
    pub async fn list_terminals(&self, session_id: &str) -> Vec<Arc<PtyHandle>> {
        let ids: Vec<String> = self
            .terminals
            .read()
            .await
            .get(session_id)
            .map(|tools| tools.keys().cloned().collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for terminal_id in ids {
            if let Some(handle) = self.resolve_pty_handle(session_id, &terminal_id).await {
                out.push(handle);
            }
        }
        out
    }

    /// Stop a started tool: cancel its task and remove it from the registry.
    /// Returns `true` if the tool existed. The reserved `MAIN_TERMINAL_ID` is not stoppable
    /// here (callers must reject it before calling).
    pub async fn stop_terminal(&self, session_id: &str, terminal_id: &str) -> bool {
        let task_id = {
            let mut reg = self.terminals.write().await;
            let removed = reg
                .get_mut(session_id)
                .and_then(|tools| tools.remove(terminal_id));
            if reg.get(session_id).is_some_and(|tools| tools.is_empty()) {
                reg.remove(session_id);
            }
            removed.map(|e| e.task_id)
        };

        match task_id {
            Some(id) => {
                self.task_registry.cancel_task(&id).await;
                true
            }
            None => false,
        }
    }

    /// Stop all PTY terminals belonging to `session_id`: cancel each task and remove from registry.
    ///
    /// Called when a session ends (natural exit or explicit termination) so its processes don't
    /// outlive the session. This is the per-session counterpart to [`kill_all`].
    pub async fn stop_session(&self, session_id: &str) {
        let task_ids: Vec<TaskId> = {
            let mut reg = self.terminals.write().await;
            reg.remove(session_id)
                .map(|tools| tools.into_values().map(|e| e.task_id).collect())
                .unwrap_or_default()
        };

        for task_id in task_ids {
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "stop_session: session={} task_id={} — cancelling task",
                session_id,
                task_id
            );
            self.task_registry.cancel_task(&task_id).await;
        }
    }

    /// Kill all tracked PTY processes across every session: cancel each task, wait up to 5 s,
    /// then SIGKILL any child PIDs that remain. Clears the terminal index on completion.
    ///
    /// Called during daemon shutdown so that spawned `claude` / shell processes do not
    /// outlive the daemon as orphans.
    pub async fn kill_all(&self) {
        let task_ids: Vec<TaskId> = {
            let mut reg = self.terminals.write().await;
            let ids = reg
                .values()
                .flat_map(|tools| tools.values().map(|e| e.task_id.clone()))
                .collect();
            reg.clear();
            ids
        };

        if task_ids.is_empty() {
            log::debug!(
                target: "tddy_daemon::claude_cli_session",
                "kill_all: no registered sessions — nothing to terminate"
            );
            return;
        }

        let pids: Vec<u32> = {
            let mut collected = Vec::new();
            for id in &task_ids {
                if let Some(handle) = self.task_registry.get(id).await {
                    collected.extend(handle.pid_slot.lock().unwrap().iter().copied());
                    handle.cancel.cancel();
                }
            }
            collected
        };

        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "kill_all: cancelling {} task(s), {} pid(s): {:?}",
            task_ids.len(),
            pids.len(),
            pids
        );

        for id in &task_ids {
            self.task_registry.cancel_task(id).await;
        }

        tokio::task::spawn_blocking(move || {
            #[cfg(unix)]
            {
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
                        "kill_all: all processes exited cleanly after cancel"
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

    /// The token immediately following `flag` in an argv, or `None` if the flag is absent.
    fn value_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1))
            .map(String::as_str)
    }

    fn contains_flag(argv: &[String], flag: &str) -> bool {
        argv.iter().any(|a| a == flag)
    }

    #[test]
    fn resuming_builds_the_argv_with_resume_not_session_id() {
        // Given — a resume of an existing session
        let resume = true;

        // When
        let argv = ClaudeCliSessionManager::build_claude_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            None,
            None,
            false,
            resume,
        );

        // Then — the id is passed to --resume and --session-id is absent
        assert_eq!(
            value_after(&argv, "--resume"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--session-id"));
    }

    #[test]
    fn a_fresh_start_builds_the_argv_with_session_id_not_resume() {
        // Given — a fresh (non-resume) start
        let resume = false;

        // When
        let argv = ClaudeCliSessionManager::build_claude_argv(
            "claude",
            "claude-opus-4-8",
            "019f5514-c0eb-7893-b32f-a02043a6e5cf",
            None,
            None,
            false,
            resume,
        );

        // Then — the id is assigned via --session-id and --resume is absent
        assert_eq!(
            value_after(&argv, "--session-id"),
            Some("019f5514-c0eb-7893-b32f-a02043a6e5cf")
        );
        assert!(!contains_flag(&argv, "--resume"));
    }

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

    /// When the tool binary cannot be spawned, the caller must see the real reason (e.g. the
    /// `spawn failed: …` from portable_pty), not a generic PTY-plumbing message. Regression for
    /// the masked "PTY runtime did not signal ready" error that hid a missing-binary failure.
    #[tokio::test]
    #[cfg(unix)]
    async fn surfaces_the_underlying_reason_when_the_binary_cannot_be_spawned() {
        // Given
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When
        let result = manager
            .start_terminal(
                "spawn-failure-session",
                worktree.path().to_path_buf(),
                "/nonexistent/definitely-not-a-real-binary",
            )
            .await;

        // Then
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("spawning a missing binary must fail");
        assert!(
            message.contains("spawn failed"),
            "error should surface the real spawn failure, was: {message}"
        );
    }

    /// A session pinned to an OS user that cannot be resolved must fail loudly, naming the user —
    /// never silently spawn under the daemon's own identity (which would defeat multi-user
    /// isolation on a root daemon).
    #[tokio::test]
    #[cfg(unix)]
    async fn fails_to_start_when_the_os_user_cannot_be_resolved() {
        // Given a manager and a session pinned to a user that does not exist on this host
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When starting the session as that unresolvable user
        let result = manager
            .start_with_options(
                "unknown-os-user-session",
                worktree.path().to_path_buf(),
                "some-model",
                "/bin/sh",
                None,
                None,
                false,
                false,
                None,
                Vec::new(),
                Some("nyxzzz-nonexistent-user"),
            )
            .await;

        // Then the start fails and the error names the unresolvable user
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("start must fail for an unresolvable os_user");
        assert!(
            message.contains("nyxzzz-nonexistent-user"),
            "error should name the unresolvable os_user, was: {message}"
        );
    }

    /// The generic "did not signal ready" plumbing message must never stand in for a concrete
    /// spawn failure — otherwise operators cannot tell a missing binary from a hung runtime.
    #[tokio::test]
    #[cfg(unix)]
    async fn does_not_mask_a_spawn_failure_as_a_ready_timeout() {
        // Given
        let manager = ClaudeCliSessionManager::new();
        let worktree = tempfile::tempdir().expect("temp dir");

        // When
        let result = manager
            .start_terminal(
                "spawn-mask-session",
                worktree.path().to_path_buf(),
                "/nonexistent/definitely-not-a-real-binary",
            )
            .await;

        // Then
        let message = result
            .err()
            .map(|e| e.to_string())
            .expect("spawning a missing binary must fail");
        assert!(
            !message.contains("did not signal ready"),
            "a spawn failure must not be reported as a ready-signal timeout, was: {message}"
        );
    }
}
