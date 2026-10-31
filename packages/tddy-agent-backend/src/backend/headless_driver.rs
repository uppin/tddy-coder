//! A coding agent driven headless — `claude -p`, `cursor agent -p`, `codex exec --json` — behind the
//! same conversation shape the ACP driver gives.
//!
//! One turn is one CLI invocation resuming the conversation's own CLI session (`--resume <id>`,
//! `exec resume <thread>`); its streamed events become the same [`AcpUpdate`]s an ACP turn
//! produces, so a caller cannot tell which transport took the turn. ACP is preferred wherever a
//! harness offers both.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

mod argv;
mod codex_jsonl;
mod oz_ndjson;
mod stream_json;
mod transcript;

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::adapters::HeadlessRenderer;
use crate::error::BackendError;

use super::acp_driver::{AcpDriver, AcpTurnEnd, AcpUpdateSink};
use transcript::{Ending, Finished, Transcript};

/// How long a failed turn waits for its stderr to close before it reads the tail it kept.
const STDERR_GRACE: Duration = Duration::from_secs(2);
/// The last stderr lines a failed turn keeps for its error message.
const STDERR_TAIL_LINES: usize = 20;

/// The CLIs this driver runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessHarness {
    /// `claude -p --output-format stream-json --verbose`
    Claude,
    /// `agent -p --output-format stream-json --stream-partial-output`
    Cursor,
    /// `codex exec --json`
    Codex,
    /// Warp's headless runner: `oz agent run -p <prompt> --output-format ndjson -C <dir>`, a later turn
    /// adding `--conversation <id>` from the run's `conversation_started` event. Distinct from the
    /// interactive `warp-cli` session type, which runs the `warp` TUI.
    Oz,
}

impl HeadlessHarness {
    /// The harness id a profile or `allowed_agents` entry names it by.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Codex => "codex",
            Self::Oz => "oz",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
            "oz" => Some(Self::Oz),
            _ => None,
        }
    }
}

/// How to run the CLI for every turn of one conversation.
#[derive(Debug, Clone, Default)]
pub struct HeadlessCommand {
    pub program: PathBuf,
    /// Added to the inherited environment (a provider route's variables).
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub model: Option<String>,
    /// An MCP config handed to the CLI (`--mcp-config` / `.cursor/mcp.json`).
    pub mcp_config: Option<PathBuf>,
    /// claude only: the MCP tool its permission prompts are routed to (`--permission-prompt-tool`).
    pub permission_prompt_tool: Option<String>,
    /// Native tools withdrawn from the CLI, where it can withdraw them.
    pub disallowed_tools: Vec<String>,
    /// oz only: the agent profile the run executes under (`--profile`) — what decides its
    /// permissions, since a headless `oz` run never asks.
    pub profile: Option<String>,
    /// codex only: an MCP server handed to the CLI inline (`-c mcp_servers.<name>.*`), for a CLI that
    /// reads no MCP config file. Its environment is the launcher's to put in [`Self::env`]: the argv
    /// names its variables, never their values.
    pub mcp_server: Option<super::acp_driver::McpServerSpec>,
    /// The agent's declaration each turn's argv is rendered from (`coding_agent.headless`), and the
    /// adapter its stream is read through. `None` builds the argv by hand (`argv::turn_argv`).
    ///
    /// TODO(claude-headless): green makes this the Claude arm's only path and deletes
    /// `argv::claude()`; Cursor, Codex and Oz follow in `#agent-tools 9/12`.
    ///
    /// TODO(headless-others): green makes it every arm's only path — `cursor.yaml`, `codex.yaml`
    /// and `oz.yaml` — and deletes `argv.rs` (`cursor()`, `codex()`, `oz()`,
    /// `codex_mcp_server_overrides`). A rendered turn binds [`Self::profile`] as the param
    /// `profile` (Oz's `--profile`) and nothing as `force`, `plan` or `sandbox_mode`, so Cursor runs
    /// read-only and Codex `workspace-write`, as by hand. `HeadlessHarness` stays: the adapters'
    /// `turn_format` returns it, and `#agent-tools 12/12` collapses it.
    pub renderer: Option<Arc<dyn HeadlessRenderer>>,
    /// With a [`Self::renderer`]: where each turn's rendered registration files (the MCP config)
    /// are written — the conversation's own directory, never the worktree, since the registration
    /// carries the agent's credential. A rendered turn without one is refused.
    pub scratch_dir: Option<PathBuf>,
}

/// One headless conversation: its CLI, and the CLI's own session id once the first turn named it.
#[derive(Debug)]
pub struct HeadlessDriver {
    harness: HeadlessHarness,
    command: HeadlessCommand,
    session_id: Mutex<Option<String>>,
    /// The turn in flight, if any. At most one: a second prompt is refused, never queued.
    running: Mutex<Option<RunningTurn>>,
}

/// The turn in flight: how to wake it for a cancel, and its CLI's process group while the child has
/// not been reaped. Per conversation, so a cancel reaches this conversation's CLI and no other.
#[derive(Debug)]
struct RunningTurn {
    cancel: Arc<Notify>,
    pgid: Option<u32>,
}

impl HeadlessDriver {
    pub fn new(harness: HeadlessHarness, command: HeadlessCommand) -> Self {
        Self {
            harness,
            command,
            session_id: Mutex::new(None),
            running: Mutex::new(None),
        }
    }

    pub fn harness(&self) -> HeadlessHarness {
        self.harness
    }

    /// The CLI's session (claude, cursor) or thread (codex) id, once a turn has named it.
    pub fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }

    fn running(&self) -> MutexGuard<'_, Option<RunningTurn>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Claims the turn slot, returning the cancel signal the turn waits on.
    fn begin_turn(&self) -> Result<Arc<Notify>, BackendError> {
        let mut running = self.running();
        if running.is_some() {
            return Err(BackendError::InvocationFailed(format!(
                "a {} turn is already running on this conversation",
                self.harness.id()
            )));
        }
        let cancel = Arc::new(Notify::new());
        *running = Some(RunningTurn {
            cancel: Arc::clone(&cancel),
            pgid: None,
        });
        Ok(cancel)
    }

    fn set_pgid(&self, pgid: Option<u32>) {
        if let Some(turn) = self.running().as_mut() {
            turn.pgid = pgid;
        }
    }

    fn store_session(&self, id: String) {
        *self
            .session_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(id);
    }

    fn spawn(&self, argv: &[String]) -> Result<Child, BackendError> {
        let mut command = Command::new(&self.command.program);
        command
            .args(argv)
            .envs(self.command.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Its own process group, so a cancel kills the CLI's children too.
        #[cfg(unix)]
        command.process_group(0);
        if let Some(cwd) = &self.command.cwd {
            command.current_dir(cwd);
        }
        command.spawn().map_err(|e| {
            let program = self.command.program.display();
            if e.kind() == io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(format!("{program}: {e}"))
            } else {
                BackendError::InvocationFailed(format!("could not start {program}: {e}"))
            }
        })
    }

    /// One turn: spawn the CLI for it, map its stream as it arrives, and end it on exit or cancel.
    async fn run_turn(
        &self,
        prompt: &str,
        updates: Option<AcpUpdateSink>,
        cancel: Arc<Notify>,
    ) -> Result<AcpTurnEnd, BackendError> {
        let resume = self.session_id();
        if let Some(renderer) = &self.command.renderer {
            return self
                .run_rendered_turn(renderer, resume.as_deref(), prompt, updates, cancel)
                .await;
        }
        let argv = argv::turn_argv(self.harness, &self.command, resume.as_deref(), prompt);
        let mut child = self.spawn(&argv)?;
        let pgid = child.id();
        self.set_pgid(pgid);
        log::debug!(
            "[headless] {} turn started, resuming: {}",
            self.harness.id(),
            resume.is_some()
        );

        let stdout = child.stdout.take().ok_or_else(|| {
            BackendError::InvocationFailed(format!("{} turn has no stdout", self.harness.id()))
        })?;
        let stderr_reader = child
            .stderr
            .take()
            .map(|stderr| tokio::spawn(drain_stderr(stderr)));
        let mut lines = BufReader::new(stdout).lines();
        let mut transcript = Transcript::new(self.harness, updates);

        let mut cancelled = loop {
            tokio::select! {
                biased;
                () = cancel.notified() => break true,
                line = lines.next_line() => match line {
                    Ok(Some(line)) => transcript.push_line(&line),
                    Ok(None) => break false,
                    Err(e) => {
                        return Err(BackendError::InvocationFailed(format!(
                            "reading {} output: {e}",
                            self.harness.id()
                        )));
                    }
                },
            }
        };
        if !cancelled {
            // The CLI closed its stdout but may still be running; a cancel must end it all the same.
            tokio::select! {
                biased;
                () = cancel.notified() => cancelled = true,
                _ = child.wait() => {}
            }
        }

        if cancelled {
            kill_turn(&mut child, pgid);
        }
        // Reaps the child (or returns its cached status when it has already exited).
        let status = child.wait().await.map_err(|e| {
            BackendError::InvocationFailed(format!("waiting for {} turn: {e}", self.harness.id()))
        })?;
        self.set_pgid(None);

        let stderr = if cancelled || status.success() {
            String::new()
        } else {
            stderr_tail(stderr_reader).await
        };
        let ending = if cancelled {
            Ending::Cancelled
        } else {
            Ending::Exited {
                status,
                stderr: &stderr,
            }
        };
        let Finished { session_id, result } = transcript.finish(ending);
        if let Some(id) = session_id {
            self.store_session(id);
        }
        result
    }
}

impl HeadlessDriver {
    /// One turn rendered from `renderer`: the prompt, the command's model, `resume` as a `Resume`
    /// session, the command's permission-prompt tool, withdrawn tools and MCP server as the slots'
    /// inputs, its `cwd` as the workdir and its `scratch_dir` as the scratch directory; the rendered
    /// files written, the program spawned as [`Self::spawn`] spawns, and its
    /// stdout decoded by the transcript the adapter's `turn_format` names.
    async fn run_rendered_turn(
        &self,
        renderer: &Arc<dyn HeadlessRenderer>,
        resume: Option<&str>,
        prompt: &str,
        updates: Option<AcpUpdateSink>,
        cancel: Arc<Notify>,
    ) -> Result<AcpTurnEnd, BackendError> {
        // TODO(claude-headless): implement — `renderer.render(&HeadlessTurn { … })`, then the loop
        // `run_turn` runs over `Transcript::new(adapter.turn_format(), updates)`.
        let _ = (renderer, resume, prompt, updates, cancel);
        todo!("claude-headless: HeadlessDriver::run_rendered_turn")
    }
}

/// Holds the turn slot for one turn. However the turn ends, it releases the slot; a child not yet
/// reaped (the caller dropped the turn's future) has its whole process group killed.
struct TurnSlot<'a> {
    driver: &'a HeadlessDriver,
}

impl Drop for TurnSlot<'_> {
    fn drop(&mut self) {
        let unreaped = self.driver.running().take().and_then(|turn| turn.pgid);
        if let Some(pgid) = unreaped {
            kill_process_group(pgid);
        }
    }
}

impl Drop for HeadlessDriver {
    fn drop(&mut self) {
        let unreaped = self
            .running
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .and_then(|turn| turn.pgid);
        if let Some(pgid) = unreaped {
            kill_process_group(pgid);
        }
    }
}

/// Kills a running turn: its whole process group where the platform has one, then the child itself.
fn kill_turn(child: &mut Child, pgid: Option<u32>) {
    if let Some(pgid) = pgid {
        kill_process_group(pgid);
    }
    if let Err(e) = child.start_kill() {
        log::debug!("[headless] kill of the turn's child failed: {e}");
    }
}

/// SIGKILLs every process in the group `pgid`.
#[cfg(unix)]
fn kill_process_group(pgid: u32) {
    // SAFETY: `killpg` takes a process-group id and a signal and touches no memory. A group that
    // has already gone makes the call fail with ESRCH, which is logged and otherwise ignored.
    let rc = unsafe { libc::killpg(pgid as libc::pid_t, libc::SIGKILL) };
    if rc != 0 {
        log::debug!(
            "[headless] killpg({pgid}) failed: {}",
            io::Error::last_os_error()
        );
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pgid: u32) {}

/// Reads a CLI's stderr to its end, keeping the last few lines.
async fn drain_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
            Ok(None) => break,
            Err(e) => {
                log::debug!("[headless] reading stderr failed: {e}");
                break;
            }
        }
    }
    tail.into()
}

/// The stderr tail of a CLI that has exited. Waits a bounded time for the reader to see the close,
/// since a grandchild that inherited the pipe can hold it open.
async fn stderr_tail(reader: Option<JoinHandle<Vec<String>>>) -> String {
    let Some(reader) = reader else {
        return String::new();
    };
    match tokio::time::timeout(STDERR_GRACE, reader).await {
        Ok(Ok(lines)) => lines.join("\n"),
        Ok(Err(e)) => {
            log::debug!("[headless] stderr reader failed: {e}");
            String::new()
        }
        Err(_) => {
            log::debug!("[headless] stderr still open after the CLI exited");
            String::new()
        }
    }
}

/// One conversation's turns, whichever transport takes them.
#[async_trait::async_trait]
pub trait ConversationDriver: Send + Sync {
    /// One prompt turn; every update goes to `updates` as it streams.
    async fn prompt_turn(
        &self,
        prompt: String,
        updates: Option<AcpUpdateSink>,
    ) -> Result<AcpTurnEnd, BackendError>;

    /// Cancel the running turn; it then ends as cancelled.
    async fn cancel_turn(&self) -> Result<(), BackendError>;
}

#[async_trait::async_trait]
impl ConversationDriver for HeadlessDriver {
    async fn prompt_turn(
        &self,
        prompt: String,
        updates: Option<AcpUpdateSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        // Claimed before the slot guard exists, so a refused turn never releases another's slot.
        let cancel = self.begin_turn()?;
        let _slot = TurnSlot { driver: self };
        self.run_turn(&prompt, updates, cancel).await
    }

    async fn cancel_turn(&self) -> Result<(), BackendError> {
        let cancel = self.running().as_ref().map(|turn| Arc::clone(&turn.cancel));
        if let Some(cancel) = cancel {
            cancel.notify_one();
        }
        Ok(())
    }
}

/// An ACP driver bound to one of its sessions, as a [`ConversationDriver`].
pub struct AcpConversation {
    pub driver: AcpDriver,
    pub session_id: String,
}

#[async_trait::async_trait]
impl ConversationDriver for AcpConversation {
    async fn prompt_turn(
        &self,
        prompt: String,
        updates: Option<AcpUpdateSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        self.driver
            .prompt(&self.session_id, prompt, updates, None)
            .await
    }

    async fn cancel_turn(&self) -> Result<(), BackendError> {
        self.driver.cancel(&self.session_id).await
    }
}
