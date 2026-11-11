//! A coding agent driven headless — `claude -p`, `cursor agent -p`, `codex exec --json`, `oz agent
//! run`, or any agent a spec declares — behind the same conversation shape the ACP driver gives.
//!
//! One turn is one CLI invocation resuming the conversation's own CLI session (`--resume <id>`,
//! `exec resume <thread>`); its streamed events become the same [`AcpUpdate`]s an ACP turn
//! produces, so a caller cannot tell which transport took the turn. ACP is preferred wherever a
//! harness offers both.
//!
//! What a turn is spawned with comes from the agent's [`HeadlessRenderer`] — its spec — and what its
//! stdout means from the adapter the spec names ([`crate::adapters::StreamAdapter::transcript`]); this
//! driver names no agent. It spawns as the workflow backend does ([`super::turn_process`]): a process
//! group of its own, no inherited `TDDY_*` variable, killed with its group on a cancel.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::adapters::headless::{
    render_env, turn_workdir, write_rendered_files, EnvTemplate, HeadlessMcpServer, HeadlessTurn,
};
use crate::adapters::{HeadlessRenderer, TurnTranscript};
use crate::error::BackendError;
use crate::usage_tap::UsageTap;

use super::acp_driver::{AcpDriver, AcpTurnEnd, AcpUpdateSink, McpServerSpec};
use super::spec_headless::DEFAULT_TURN_DEADLINE;
use super::turn_process::{
    describe_deadline, kill_process_group, spawn_turn_command, RunningGroup, TurnCommand,
    STDERR_GRACE,
};
use super::SessionMode;
use transcript::{Ending, Finished};

pub(crate) mod codex_jsonl;
pub(crate) mod oz_ndjson;
pub(crate) mod stream_json;
pub(crate) mod transcript;

/// The last stderr lines a failed turn keeps for its error message.
const STDERR_TAIL_LINES: usize = 20;

/// How to run the CLI for every turn of one conversation.
#[derive(Debug, Clone, Default)]
pub struct HeadlessCommand {
    /// Added to the inherited environment (a provider route's variables), on every turn, before the
    /// spec's own `headless.env`.
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub model: Option<String>,
    /// claude only: the MCP tool its permission prompts are routed to (`--permission-prompt-tool`).
    pub permission_prompt_tool: Option<String>,
    /// Native tools withdrawn from the CLI, where it can withdraw them.
    pub disallowed_tools: Vec<String>,
    /// oz only: the agent profile the run executes under (`--profile`) — what decides its
    /// permissions, since a headless `oz` run never asks.
    pub profile: Option<String>,
    /// The tool server handed to the CLI through the spec's MCP slot; the renderer's own
    /// ([`HeadlessRenderer::tool_server`]) when `None`.
    pub mcp_server: Option<McpServerSpec>,
    /// Where each turn's rendered registration files (the MCP config) are written — the
    /// conversation's own directory, never the worktree, since the registration carries the agent's
    /// credential. A turn without one is refused. It is also the `{session_dir}` the spec's
    /// `headless.env` values name.
    pub scratch_dir: Option<PathBuf>,
    /// The agent's declaration each turn's argv is rendered from (`coding_agent.headless`), and the
    /// adapter its stream is read through. A turn without one is refused: no argv is built by hand.
    pub renderer: Option<Arc<dyn HeadlessRenderer>>,
    /// How long one turn may run before its process group is killed and the turn fails;
    /// [`DEFAULT_TURN_DEADLINE`] when `None`. No headless wait is unbounded.
    pub turn_deadline: Option<Duration>,
    /// Where the turn's usage events are tapped (`#usage-binding 3/5`): the events the spec's
    /// `headless.usage_events` declares, appended verbatim. `None` taps nothing.
    pub usage_tap: Option<crate::usage_tap::UsageTapSpec>,
}

/// One headless conversation: its CLI, and the CLI's own session id once the first turn named it.
#[derive(Debug)]
pub struct HeadlessDriver {
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
    /// A conversation run as `command` says.
    pub fn new(command: HeadlessCommand) -> Self {
        Self {
            command,
            session_id: Mutex::new(None),
            running: Mutex::new(None),
        }
    }

    /// The agent's id (`claude`); `headless` for a command that names no renderer.
    pub fn agent_id(&self) -> &str {
        self.command
            .renderer
            .as_ref()
            .map_or("headless", |renderer| renderer.agent_id())
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
                self.agent_id()
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

    /// One turn rendered from the agent's spec: the prompt, the command's model, `resume` as a
    /// `Resume` session, the command's permission-prompt tool, withdrawn tools and MCP server as the
    /// slots' inputs, its `cwd` as the workdir and its `scratch_dir` as the scratch directory; the
    /// rendered files written, the program spawned in a process group of its own, and its stdout
    /// decoded by the transcript the spec's adapter makes.
    async fn run_turn(
        &self,
        prompt: &str,
        updates: Option<AcpUpdateSink>,
        cancel: Arc<Notify>,
    ) -> Result<AcpTurnEnd, BackendError> {
        let resume = self.session_id();
        let agent = self.agent_id().to_string();
        let renderer = self.command.renderer.as_ref().ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "a headless {agent} turn is rendered from the agent's spec, and the conversation names none"
            ))
        })?;
        let scratch_dir = self.command.scratch_dir.clone().ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "a rendered {agent} turn writes its registration files to a scratch directory, and the conversation names none"
            ))
        })?;
        let workdir = turn_workdir(self.command.cwd.as_deref())?;
        let turn = HeadlessTurn {
            prompt: prompt.to_string(),
            model: self.command.model.clone(),
            session: resume.clone().map(SessionMode::Resume),
            permission_mode: None,
            append_system_prompt_file: None,
            allowed_tools: Vec::new(),
            denied_tools: self.command.disallowed_tools.clone(),
            permission_prompt_tool: self.command.permission_prompt_tool.clone(),
            mcp_server: self
                .command
                .mcp_server
                .as_ref()
                .map(headless_server)
                .or_else(|| renderer.tool_server()),
            workdir: workdir.clone(),
            scratch_dir: scratch_dir.clone(),
            params: self
                .command
                .profile
                .iter()
                .map(|profile| ("profile".to_string(), profile.clone()))
                .collect(),
            route_args: Vec::new(),
        };
        let invocation = renderer.render(&turn)?;
        let transcript = invocation.adapter.transcript(updates).ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "the {} adapter reads no conversation transcript",
                invocation.adapter.name()
            ))
        })?;
        write_rendered_files(&invocation.files)?;
        let self_exe = std::env::current_exe().map_err(|e| {
            BackendError::InvocationFailed(format!("this executable cannot be named: {e}"))
        })?;
        let env: Vec<(String, String)> = self
            .command
            .env
            .iter()
            .cloned()
            .chain(render_env(
                &invocation.env,
                &EnvTemplate {
                    session_dir: Some(scratch_dir),
                    self_exe,
                },
            )?)
            .collect();
        log::debug!(
            "[headless] {agent} turn started, resuming: {}",
            resume.is_some()
        );
        let child = tokio::process::Command::from(spawn_turn_command(&TurnCommand {
            program: &invocation.program,
            argv: &invocation.argv,
            env: &env,
            cwd: &workdir,
            inherit_stdin: false,
        }))
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            let program = invocation.program.display();
            if e.kind() == io::ErrorKind::NotFound {
                BackendError::BinaryNotFound(format!("{program}: {e}"))
            } else {
                BackendError::InvocationFailed(format!("could not start {program}: {e}"))
            }
        })?;
        self.read_turn(&agent, transcript, child, cancel).await
    }

    /// Maps a spawned turn's stdout as it arrives, through `transcript`, and ends the turn on exit or
    /// cancel.
    async fn read_turn(
        &self,
        agent: &str,
        mut transcript: Box<dyn TurnTranscript>,
        mut child: Child,
        cancel: Arc<Notify>,
    ) -> Result<AcpTurnEnd, BackendError> {
        let pgid = child.id();
        self.set_pgid(pgid);
        // Registered as running, so a Ctrl+C in the TUI reaches this CLI's group too.
        let _running_group = pgid.map(RunningGroup::new);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::InvocationFailed(format!("{agent} turn has no stdout")))?;
        let stderr_reader = child
            .stderr
            .take()
            .map(|stderr| tokio::spawn(drain_stderr(stderr)));
        let mut lines = BufReader::new(stdout).lines();
        let mut usage_tap = self.command.usage_tap.clone().map(UsageTap::open);
        let deadline = self.command.turn_deadline.unwrap_or(DEFAULT_TURN_DEADLINE);
        let overrun = tokio::time::sleep(deadline);
        tokio::pin!(overrun);

        let mut cancelled = loop {
            tokio::select! {
                biased;
                () = cancel.notified() => break true,
                () = &mut overrun => {
                    return Err(self.end_overrun(agent, &mut child, pgid, deadline).await);
                }
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        if let Some(tap) = usage_tap.as_mut() {
                            tap.offer_line(&line);
                        }
                        transcript.push_line(&line);
                    }
                    Ok(None) => break false,
                    Err(e) => {
                        return Err(BackendError::InvocationFailed(format!(
                            "reading {agent} output: {e}"
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
                () = &mut overrun => {
                    return Err(self.end_overrun(agent, &mut child, pgid, deadline).await);
                }
                _ = child.wait() => {}
            }
        }

        if cancelled {
            kill_turn(&mut child, pgid);
        }
        // Reaps the child (or returns its cached status when it has already exited).
        let status = child.wait().await.map_err(|e| {
            BackendError::InvocationFailed(format!("waiting for {agent} turn: {e}"))
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

    /// Ends a turn that outlived its deadline: its process group killed, its child reaped, the
    /// turn slot's group cleared. Returns the error the turn fails with.
    async fn end_overrun(
        &self,
        agent: &str,
        child: &mut Child,
        pgid: Option<u32>,
        deadline: Duration,
    ) -> BackendError {
        kill_turn(child, pgid);
        let _ = child.wait().await;
        self.set_pgid(None);
        BackendError::InvocationFailed(format!(
            "{agent} turn did not end within its {} deadline; its process group was killed",
            describe_deadline(deadline)
        ))
    }
}

/// `server` as the port's MCP server: its name, program, arguments and environment.
fn headless_server(server: &McpServerSpec) -> HeadlessMcpServer {
    HeadlessMcpServer {
        name: server.name.clone(),
        command: server.command.clone(),
        args: server.args.clone(),
        env: server.env.iter().cloned().collect(),
        cwd: server.cwd.clone(),
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
