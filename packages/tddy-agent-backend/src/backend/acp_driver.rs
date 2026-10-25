//! One ACP client driver for every ACP coding agent.
//!
//! [`AcpDriver`] owns one ACP agent child process and its `ClientSideConnection`. The connection is
//! `?Send`, so it lives on a dedicated thread with a current-thread runtime and a `LocalSet`; only
//! channels cross. The driver opens or loads sessions, applies a requested model, passes MCP
//! servers on `session/new`, prompts, cancels, lists models, and kills its child when dropped.
//!
//! Permission requests go to an injected [`PermissionHandler`] rather than being answered here;
//! [`AllowOnce`] is what the workflow backends install. Per-agent behaviour — Codex's `codex login`
//! retry, for one — is an [`AcpAgentHooks`] implementation.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-acp-driver.md

mod client;
mod mapping;
mod session;
mod worker;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::error::BackendError;
use crate::provider_route::HarnessRoute;

use super::routed::route_child_env;
use super::{BackendModels, InvokeRequest, InvokeResponse, ProgressSink, SessionMode};

/// How to start the agent child: program, arguments, extra environment, working directory.
#[derive(Debug, Clone, Default)]
pub struct AcpAgentCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Added to the inherited environment (a provider route's plain and secret variables).
    pub env: Vec<(String, String)>,
    /// The child's working directory; `None` inherits the parent's.
    pub cwd: Option<PathBuf>,
}

/// A stdio MCP server the agent is asked to connect to for a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerSpec {
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// What [`AcpDriver::open_session`] opens.
#[derive(Debug, Clone, Default)]
pub struct OpenSession {
    /// Absolute working directory of the ACP session.
    pub cwd: PathBuf,
    /// An ACP session id to resume with `session/load`; `None` opens a fresh one.
    pub resume: Option<String>,
    /// A model to apply with `session/set_model`. One the agent does not offer is an error naming
    /// the offered ones — never a silent fallback to the agent's default.
    pub model: Option<String>,
    pub mcp_servers: Vec<McpServerSpec>,
    /// Sent verbatim as `_meta` on `session/new` / `session/load` — where an adapter reads options
    /// the protocol has no field for (`claude-agent-acp` reads `disallowedTools` there).
    pub meta: Option<serde_json::Map<String, serde_json::Value>>,
}

/// An opened ACP session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpSession {
    pub session_id: String,
    /// The model ids the agent advertised for the session (empty when it advertised none).
    pub available_models: Vec<String>,
    /// The model in force after opening.
    pub current_model: Option<String>,
}

/// Why a prompt turn ended, in ACP's own terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpStopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
}

/// The end of one prompt turn: why it ended and the agent's text for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpTurnEnd {
    pub stop_reason: AcpStopReason,
    pub text: String,
}

/// A tool call's status as ACP reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// One `session/update`, in the shape callers that keep an ACP transcript want.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpUpdate {
    /// A chunk of the agent's message text.
    Text(String),
    /// A chunk of the agent's reasoning.
    Thought(String),
    ToolCall {
        id: String,
        title: String,
        kind: String,
        raw_input: Option<serde_json::Value>,
    },
    ToolCallUpdate {
        id: String,
        title: Option<String>,
        status: Option<AcpToolStatus>,
        raw_output: Option<serde_json::Value>,
    },
    Plan(Vec<String>),
}

/// Receives every [`AcpUpdate`] of a turn, in order.
pub type AcpUpdateSink = Arc<dyn Fn(AcpUpdate) + Send + Sync>;

/// One option the agent offers on a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOptionView {
    pub option_id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

/// A `session/request_permission` from the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub session_id: String,
    pub tool_call_id: String,
    pub title: Option<String>,
    pub raw_input: Option<serde_json::Value>,
    pub options: Vec<PermissionOptionView>,
}

/// The answer to a [`PermissionRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Selected { option_id: String },
    Cancelled,
}

/// Answers the agent's permission requests. The driver asks it and sends its decision verbatim.
#[async_trait::async_trait]
pub trait PermissionHandler: Send + Sync {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision;
}

/// Picks the first `allow_once` option — what the workflow backends have always answered.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowOnce;

/// The kind ACP gives an option that allows one call.
const ALLOW_ONCE_KIND: &str = "allow_once";

#[async_trait::async_trait]
impl PermissionHandler for AllowOnce {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision {
        match request
            .options
            .into_iter()
            .find(|option| option.kind == ALLOW_ONCE_KIND)
        {
            Some(option) => PermissionDecision::Selected {
                option_id: option.option_id,
            },
            None => PermissionDecision::Cancelled,
        }
    }
}

/// What the driver does after a session call failed, as an agent's hooks decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionErrorVerdict {
    /// Fail the call with the error.
    Fail,
    /// The hook recovered (e.g. ran `codex login`); retry the call once.
    RetryOnce,
}

/// Per-agent behaviour around the shared driver.
#[async_trait::async_trait]
pub trait AcpAgentHooks: Send + Sync {
    /// Called when `session/new` or `session/load` failed with `message`.
    async fn on_session_error(&self, _message: &str) -> SessionErrorVerdict {
        SessionErrorVerdict::Fail
    }
}

/// Hooks that change nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHooks;

impl AcpAgentHooks for NoHooks {}

/// How many requests may wait for the worker before a caller waits to send one.
const COMMAND_QUEUE_DEPTH: usize = 16;

/// The program a relayed driver reports: the agent runs elsewhere, so no path is known here.
const RELAYED_AGENT_PROGRAM: &str = "<relayed agent>";

/// The driver a backend runs its agent on. A backend starts it on first use and replaces it when
/// its provider route changes; dropping the replaced driver ends its child.
pub(super) type DriverSlot = Mutex<Option<Arc<AcpDriver>>>;

/// The driver in `slot`, started by `start` when the slot is empty.
pub(super) fn started(
    slot: &DriverSlot,
    start: impl FnOnce() -> Result<AcpDriver, BackendError>,
) -> Result<Arc<AcpDriver>, BackendError> {
    let mut guard = slot.lock().map_err(|_| {
        BackendError::InvocationFailed("the ACP driver slot is poisoned".to_string())
    })?;
    if let Some(driver) = guard.as_ref() {
        return Ok(Arc::clone(driver));
    }
    let driver = Arc::new(start()?);
    *guard = Some(Arc::clone(&driver));
    Ok(driver)
}

/// A provider route's environment as a child is spawned with it: its plain pairs, then its secret
/// pairs.
pub(super) fn route_env(route: &HarnessRoute) -> Vec<(String, String)> {
    route_child_env(route)
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The working directory a session opens in: the request's, else this process's.
pub(super) fn session_cwd(working_dir: Option<&Path>) -> Result<PathBuf, BackendError> {
    match working_dir {
        Some(dir) => Ok(dir.to_path_buf()),
        None => std::env::current_dir().map_err(|e| {
            BackendError::InvocationFailed(format!("no working directory for the session: {e}"))
        }),
    }
}

/// The session an invocation opens: resumed by the id its request names, or fresh. The request's
/// model is applied unless a provider route is in force; on a route the route's model stands, as it
/// always has.
pub(super) fn open_for_request(
    request: &InvokeRequest,
    on_route: bool,
) -> Result<OpenSession, BackendError> {
    Ok(OpenSession {
        cwd: session_cwd(request.working_dir.as_deref())?,
        resume: match &request.session {
            Some(SessionMode::Resume(id)) => Some(id.clone()),
            _ => None,
        },
        model: if on_route {
            None
        } else {
            request.model.clone()
        },
        mcp_servers: Vec::new(),
        meta: None,
    })
}

/// Run one prompt turn on `session` and report it as the backend's response.
pub(super) async fn answer(
    driver: &AcpDriver,
    session: AcpSession,
    prompt: String,
    progress: Option<ProgressSink>,
) -> Result<InvokeResponse, BackendError> {
    let turn = driver
        .prompt(&session.session_id, prompt, None, progress)
        .await?;
    Ok(InvokeResponse {
        output: turn.text,
        exit_code: 0,
        session_id: Some(session.session_id),
        questions: Vec::new(),
        raw_stream: None,
        stderr: None,
    })
}

/// One ACP agent child and its connection. Dropping it terminates the child.
///
/// Every method is a request to the worker thread that owns the child, so the driver is shared
/// freely: a cancel can reach a turn that another task is still waiting on.
pub struct AcpDriver {
    program: PathBuf,
    commands: mpsc::Sender<worker::Command>,
    /// The child's pid while it runs; `0` once the worker has seen the child exit or has killed it.
    pid: Arc<AtomicU32>,
}

impl std::fmt::Debug for AcpDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpDriver")
            .field("program", &self.program)
            .finish()
    }
}

impl AcpDriver {
    /// Spawn the agent and start serving its connection. The child is spawned before this returns,
    /// so a missing binary is an error here; the connection is initialized by the first request
    /// that needs it.
    pub fn spawn(
        command: AcpAgentCommand,
        permissions: Arc<dyn PermissionHandler>,
        hooks: Arc<dyn AcpAgentHooks>,
    ) -> Result<Self, BackendError> {
        Self::start_worker(worker::AgentIo::Spawn(command), permissions, hooks)
    }

    /// Serve an ACP agent that is already running elsewhere and reachable over `stream` — one a
    /// supervisor started as another OS user and relays — instead of spawning a child here. No pid
    /// is known ([`Self::agent_pid`] is `None`); closing the stream, which dropping the driver does,
    /// is what ends the agent.
    pub fn spawn_over(
        stream: std::os::unix::net::UnixStream,
        permissions: Arc<dyn PermissionHandler>,
        hooks: Arc<dyn AcpAgentHooks>,
    ) -> Result<Self, BackendError> {
        Self::start_worker(worker::AgentIo::Relayed(stream), permissions, hooks)
    }

    /// Start the worker thread over `io` and wait until it reports the agent started (or connected).
    fn start_worker(
        io: worker::AgentIo,
        permissions: Arc<dyn PermissionHandler>,
        hooks: Arc<dyn AcpAgentHooks>,
    ) -> Result<Self, BackendError> {
        let (program, ready) = match &io {
            worker::AgentIo::Spawn(command) => (command.program.clone(), "started"),
            worker::AgentIo::Relayed(_) => (PathBuf::from(RELAYED_AGENT_PROGRAM), "connected"),
        };
        let (commands, receiver) = mpsc::channel(COMMAND_QUEUE_DEPTH);
        let (started, spawned) = std::sync::mpsc::sync_channel(1);
        let pid = Arc::new(AtomicU32::new(0));
        let setup = worker::Setup {
            io,
            permissions,
            hooks,
            commands: receiver,
            started,
            pid: Arc::clone(&pid),
        };
        std::thread::Builder::new()
            .name("acp-driver".to_string())
            .spawn(move || worker::run(setup))
            .map_err(|e| {
                BackendError::InvocationFailed(format!(
                    "could not start the ACP driver thread: {e}"
                ))
            })?;
        spawned.recv().map_err(|_| {
            BackendError::InvocationFailed(format!(
                "the ACP driver thread stopped before its agent {ready}"
            ))
        })??;
        Ok(Self {
            program,
            commands,
            pid,
        })
    }

    /// `session/new` (or `session/load` for `open.resume`), then `session/set_model` if asked.
    pub async fn open_session(&self, open: OpenSession) -> Result<AcpSession, BackendError> {
        self.request(|reply| worker::Command::OpenSession { open, reply })
            .await
    }

    /// One `session/prompt` with `prompt` as its text. Every update goes to `updates` and, mapped
    /// to [`crate::ProgressEvent`]s, to `progress`.
    pub async fn prompt(
        &self,
        session_id: &str,
        prompt: String,
        updates: Option<AcpUpdateSink>,
        progress: Option<ProgressSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        self.request(|reply| worker::Command::Prompt {
            session_id: session_id.to_string(),
            prompt,
            updates,
            progress,
            reply,
        })
        .await
    }

    /// `session/cancel` for a running turn; that turn then ends as [`AcpStopReason::Cancelled`].
    pub async fn cancel(&self, session_id: &str) -> Result<(), BackendError> {
        self.request(|reply| worker::Command::Cancel {
            session_id: session_id.to_string(),
            reply,
        })
        .await
    }

    /// The models a fresh session would offer.
    pub async fn list_models(&self) -> Result<BackendModels, BackendError> {
        self.request(|reply| worker::Command::ListModels { reply })
            .await
    }

    /// The agent child's process id, while it runs.
    pub fn agent_pid(&self) -> Option<u32> {
        match self.pid.load(Ordering::SeqCst) {
            0 => None,
            pid => Some(pid),
        }
    }

    /// Send one command to the worker and wait for its answer.
    async fn request<T>(
        &self,
        command: impl FnOnce(worker::Reply<T>) -> worker::Command,
    ) -> Result<T, BackendError> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(command(reply))
            .await
            .map_err(|_| agent_stopped())?;
        answer.await.map_err(|_| agent_stopped())?
    }
}

fn agent_stopped() -> BackendError {
    BackendError::InvocationFailed("the ACP agent is no longer running".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(id: &str, kind: &str) -> PermissionOptionView {
        PermissionOptionView {
            option_id: id.to_string(),
            name: id.to_string(),
            kind: kind.to_string(),
        }
    }

    fn asking(options: Vec<PermissionOptionView>) -> PermissionRequest {
        PermissionRequest {
            session_id: "session-1".to_string(),
            tool_call_id: "call-1".to_string(),
            title: None,
            raw_input: None,
            options,
        }
    }

    #[tokio::test]
    async fn allow_once_picks_the_first_option_that_allows_one_call() {
        // Given
        let request = asking(vec![
            option("reject-once", "reject_once"),
            option("allow-always", "allow_always"),
            option("allow-once", "allow_once"),
            option("allow-once-again", "allow_once"),
        ]);

        // When
        let decision = AllowOnce.decide(request).await;

        // Then
        assert_eq!(
            decision,
            PermissionDecision::Selected {
                option_id: "allow-once".to_string()
            }
        );
    }

    #[tokio::test]
    async fn allow_once_cancels_when_no_option_allows_one_call() {
        // Given
        let request = asking(vec![option("reject-once", "reject_once")]);

        // When
        let decision = AllowOnce.decide(request).await;

        // Then
        assert_eq!(decision, PermissionDecision::Cancelled);
    }
}
