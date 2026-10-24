//! `CodingAgentSession` — a `SubagentSession` whose turns a coding agent takes, over ACP.

use async_trait::async_trait;
use tddy_agent_backend::{AcpAgentCommand, OpenSession};
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{PromptOutcome, SubagentError, SubagentSession, TurnRequest};

/// One conversation with a coding agent: one agent process, one ACP session.
///
/// A turn is one `session/prompt`; its outcome carries the agent's text, `end_turn` /
/// `cancelled` / …, and one message descriptor per tool call the turn made. Options that act on an
/// OpenAI-loop transcript (`fromMessageId`, `correction`, `replacement`, `yieldConditions`,
/// `maxTurns`) are refused with [`SubagentError::unsupported_by_coding_agent`].
pub struct CodingAgentSession {
    // TODO(acp-subagent): implement — the driver, the ACP session id, the permission recorder
    model: String,
}

impl CodingAgentSession {
    /// Spawn the agent from `command`, open (or resume) its ACP session, and hold both.
    pub async fn start(
        _command: AcpAgentCommand,
        _open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        // TODO(acp-subagent): implement
        let _ = model;
        Err(SubagentError::from(
            "TODO(acp-subagent): CodingAgentSession::start is not implemented",
        ))
    }

    /// Send every ACP update of every later turn to `sink`, as it streams — how a daemon-run
    /// conversation's transcript records the agent's tool calls while the turn is still running.
    pub fn record_updates_with(&mut self, _sink: tddy_agent_backend::AcpUpdateSink) {
        // TODO(conversation-bubble): implement
    }

    /// A handle that cancels this session's running turn without holding the session — what the
    /// conversation table uses when `subagent_cancel` arrives while a prompt holds the session.
    pub fn cancel_handle(&self) -> CancelHandle {
        // TODO(acp-subagent): implement
        CancelHandle { _private: () }
    }

    /// The agent process's id, while it runs.
    pub fn agent_pid(&self) -> Option<u32> {
        // TODO(acp-subagent): implement
        None
    }
}

/// Cancels a [`CodingAgentSession`]'s running turn (`session/cancel`); the turn then ends as
/// cancelled. Cloneable and `Send`, so it can be held apart from the session.
#[derive(Clone)]
pub struct CancelHandle {
    // TODO(acp-subagent): implement — the driver and the ACP session id
    _private: (),
}

impl CancelHandle {
    pub async fn cancel(&self) -> Result<(), SubagentError> {
        // TODO(acp-subagent): implement
        Err(SubagentError::from(
            "TODO(acp-subagent): CancelHandle::cancel is not implemented",
        ))
    }
}

#[async_trait]
impl SubagentSession for CodingAgentSession {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        // TODO(acp-subagent): implement
        Err(SubagentError::from(
            "TODO(acp-subagent): CodingAgentSession::take_turn is not implemented",
        ))
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn cumulative_usage(&self) -> TokenUsage {
        // TODO(acp-subagent): implement
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        // TODO(acp-subagent): implement
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        // TODO(acp-subagent): implement
        Vec::new()
    }
}
