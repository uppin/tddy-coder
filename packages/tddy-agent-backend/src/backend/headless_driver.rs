//! A coding agent driven headless — `claude -p`, `cursor agent -p`, `codex exec --json` — behind the
//! same conversation shape the ACP driver gives.
//!
//! One turn is one CLI invocation resuming the conversation's own CLI session (`--resume <id>`,
//! `exec resume <thread>`); its streamed events become the same [`AcpUpdate`]s an ACP turn
//! produces, so a caller cannot tell which transport took the turn. ACP is preferred wherever a
//! harness offers both.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::BackendError;

use super::acp_driver::{AcpDriver, AcpTurnEnd, AcpUpdateSink};

/// The CLIs this driver runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessHarness {
    /// `claude -p --output-format stream-json --verbose`
    Claude,
    /// `agent -p --output-format stream-json --stream-partial-output`
    Cursor,
    /// `codex exec --json`
    Codex,
}

impl HeadlessHarness {
    /// The harness id a profile or `allowed_agents` entry names it by.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Codex => "codex",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
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
}

/// One headless conversation: its CLI, and the CLI's own session id once the first turn named it.
#[derive(Debug)]
pub struct HeadlessDriver {
    harness: HeadlessHarness,
    command: HeadlessCommand,
    // TODO(headless-driver): implement — the running child, for cancel
    session_id: Mutex<Option<String>>,
}

impl HeadlessDriver {
    pub fn new(harness: HeadlessHarness, command: HeadlessCommand) -> Self {
        Self {
            harness,
            command,
            session_id: Mutex::new(None),
        }
    }

    pub fn harness(&self) -> HeadlessHarness {
        self.harness
    }

    /// The CLI's session (claude, cursor) or thread (codex) id, once a turn has named it.
    pub fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }
}

fn not_implemented(what: &str) -> BackendError {
    BackendError::InvocationFailed(format!("TODO(headless-driver): {what} is not implemented"))
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
        _prompt: String,
        _updates: Option<AcpUpdateSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        // TODO(headless-driver): implement
        let _ = &self.command;
        Err(not_implemented("HeadlessDriver::prompt_turn"))
    }

    async fn cancel_turn(&self) -> Result<(), BackendError> {
        // TODO(headless-driver): implement
        Err(not_implemented("HeadlessDriver::cancel_turn"))
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
        _prompt: String,
        _updates: Option<AcpUpdateSink>,
    ) -> Result<AcpTurnEnd, BackendError> {
        // TODO(headless-driver): implement
        Err(not_implemented("AcpConversation::prompt_turn"))
    }

    async fn cancel_turn(&self) -> Result<(), BackendError> {
        // TODO(headless-driver): implement
        Err(not_implemented("AcpConversation::cancel_turn"))
    }
}
