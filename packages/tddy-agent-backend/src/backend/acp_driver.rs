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

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::BackendError;

use super::{BackendModels, ProgressSink};

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

#[async_trait::async_trait]
impl PermissionHandler for AllowOnce {
    async fn decide(&self, _request: PermissionRequest) -> PermissionDecision {
        // TODO(acp-driver): implement
        PermissionDecision::Cancelled
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

/// One ACP agent child and its connection. Dropping it terminates the child.
pub struct AcpDriver {
    // TODO(acp-driver): implement — worker channel, child pid
    _command: AcpAgentCommand,
}

impl std::fmt::Debug for AcpDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpDriver")
            .field("program", &self._command.program)
            .finish()
    }
}

fn not_implemented(what: &str) -> BackendError {
    BackendError::InvocationFailed(format!("TODO(acp-driver): {what} is not implemented"))
}

impl AcpDriver {
    /// Spawn the agent and initialize the connection.
    pub fn spawn(
        command: AcpAgentCommand,
        _permissions: Arc<dyn PermissionHandler>,
        _hooks: Arc<dyn AcpAgentHooks>,
    ) -> Result<Self, BackendError> {
        // TODO(acp-driver): implement
        Ok(Self { _command: command })
    }

    /// `session/new` (or `session/load` for `open.resume`), then `session/set_model` if asked.
    pub async fn open_session(&self, _open: OpenSession) -> Result<AcpSession, BackendError> {
        // TODO(acp-driver): implement
        Err(not_implemented("open_session"))
    }

    /// One `session/prompt` with `prompt` as its text. Every update goes to `updates` and, mapped
    /// to [`crate::ProgressEvent`]s, to `progress`.
    pub async fn prompt(
        &self,
        _session_id: &str,
        _prompt: String,
        _updates: Option<AcpUpdateSink>,
        _progress: Option<ProgressSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        // TODO(acp-driver): implement
        Err(not_implemented("prompt"))
    }

    /// `session/cancel` for a running turn; that turn then ends as [`AcpStopReason::Cancelled`].
    pub async fn cancel(&self, _session_id: &str) -> Result<(), BackendError> {
        // TODO(acp-driver): implement
        Err(not_implemented("cancel"))
    }

    /// The models a fresh session would offer.
    pub async fn list_models(&self) -> Result<BackendModels, BackendError> {
        // TODO(acp-driver): implement
        Err(not_implemented("list_models"))
    }

    /// The agent child's process id, while it runs.
    pub fn agent_pid(&self) -> Option<u32> {
        // TODO(acp-driver): implement
        None
    }
}
