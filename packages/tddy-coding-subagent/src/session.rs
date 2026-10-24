//! `CodingAgentSession` — a `SubagentSession` whose turns a coding agent takes, over ACP.

use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use tddy_agent_backend::{
    AcpAgentCommand, AcpDriver, AcpStopReason, AcpToolStatus, AcpUpdate, NoHooks, OpenSession,
};
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    ContentBlock, MessageDescriptor, MessageId, MessageRole, PromptOutcome, StopReason,
    SubagentError, SubagentSession, ToolCallDescriptor, TurnRequest, MESSAGE_PREVIEW_CHARS,
};

use crate::permissions::DenyAndRecord;

/// One conversation with a coding agent: one agent process, one ACP session.
///
/// A turn is one `session/prompt`; its outcome carries the agent's text, `end_turn` /
/// `cancelled` / …, and one message descriptor per tool call the turn made. Options that act on an
/// OpenAI-loop transcript (`fromMessageId`, `correction`, `replacement`, `yieldConditions`,
/// `maxTurns`) are refused with [`SubagentError::unsupported_by_coding_agent`].
///
/// Dropping the session drops the driver, which terminates the agent process.
pub struct CodingAgentSession {
    driver: Arc<AcpDriver>,
    acp_session_id: String,
    permissions: Arc<DenyAndRecord>,
    model: String,
    /// The ordinal the next message id is minted from.
    next_ordinal: u64,
    /// Each turn's question and answer, oldest first, for [`SubagentSession::tail`].
    history: Vec<String>,
}

impl CodingAgentSession {
    /// Spawn the agent from `command`, open (or resume) its ACP session, and hold both.
    pub async fn start(
        command: AcpAgentCommand,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let permissions = Arc::new(DenyAndRecord::default());
        let driver = AcpDriver::spawn(command, permissions.clone(), Arc::new(NoHooks))
            .map_err(|e| SubagentError::from(format!("could not start the coding agent: {e}")))?;
        Self::opened(driver, permissions, open, model).await
    }

    /// [`Self::start`] for an agent already running behind `stream` (started by the supervisor as
    /// the session's OS user): no process of this session's own; closing the stream ends the agent.
    pub async fn start_over(
        stream: std::os::unix::net::UnixStream,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let permissions = Arc::new(DenyAndRecord::default());
        let driver = AcpDriver::spawn_over(stream, permissions.clone(), Arc::new(NoHooks))
            .map_err(|e| SubagentError::from(format!("could not reach the coding agent: {e}")))?;
        Self::opened(driver, permissions, open, model).await
    }

    async fn opened(
        driver: AcpDriver,
        permissions: Arc<DenyAndRecord>,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let opened = driver
            .open_session(open)
            .await
            .map_err(|e| SubagentError::from(format!("could not open the coding agent: {e}")))?;
        Ok(Self {
            driver: Arc::new(driver),
            acp_session_id: opened.session_id,
            permissions,
            model,
            next_ordinal: 1,
            history: Vec::new(),
        })
    }

    /// A handle that cancels this session's running turn without holding the session — what the
    /// conversation table uses when `subagent_cancel` arrives while a prompt holds the session.
    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            driver: Arc::downgrade(&self.driver),
            session_id: self.acp_session_id.clone(),
        }
    }

    /// The agent process's id, while it runs.
    pub fn agent_pid(&self) -> Option<u32> {
        self.driver.agent_pid()
    }

    fn mint_id(&mut self) -> MessageId {
        let id = MessageId::from(format!("m{}", self.next_ordinal));
        self.next_ordinal += 1;
        id
    }

    fn describe_turn(&mut self, updates: Vec<AcpUpdate>, text: &str) -> Vec<MessageDescriptor> {
        let mut calls: Vec<ToolCallRecord> = Vec::new();
        for update in updates {
            match update {
                AcpUpdate::ToolCall {
                    id,
                    title,
                    raw_input,
                    ..
                } => calls.push(ToolCallRecord {
                    id,
                    name: title,
                    arguments: raw_input.map(|input| input.to_string()).unwrap_or_default(),
                    failed: false,
                }),
                AcpUpdate::ToolCallUpdate { id, status, .. } => {
                    if let Some(call) = calls.iter_mut().find(|call| call.id == id) {
                        call.failed |= status == Some(AcpToolStatus::Failed);
                    }
                }
                AcpUpdate::Text(_) | AcpUpdate::Thought(_) | AcpUpdate::Plan(_) => {}
            }
        }
        let mut messages = Vec::new();
        for call in calls {
            messages.push(MessageDescriptor {
                id: self.mint_id(),
                role: MessageRole::Tool,
                tool: Some(call.name.clone()),
                tool_calls: vec![ToolCallDescriptor {
                    name: call.name,
                    arguments: cut(&call.arguments),
                }],
                is_error: call.failed,
                preview: cut(&call.arguments),
                result_summary: None,
                worktree_change: None,
            });
        }
        for denied in self.permissions.take_denied() {
            let title = denied.title.unwrap_or_else(|| denied.tool_call_id.clone());
            messages.push(MessageDescriptor {
                id: self.mint_id(),
                role: MessageRole::Tool,
                tool: Some(title.clone()),
                tool_calls: Vec::new(),
                is_error: true,
                preview: cut(&format!("permission denied: {title}")),
                result_summary: None,
                worktree_change: None,
            });
        }
        if !text.is_empty() {
            messages.push(MessageDescriptor {
                id: self.mint_id(),
                role: MessageRole::Assistant,
                tool: None,
                tool_calls: Vec::new(),
                is_error: false,
                preview: cut(text),
                result_summary: None,
                worktree_change: None,
            });
        }
        messages
    }
}

struct ToolCallRecord {
    id: String,
    name: String,
    arguments: String,
    failed: bool,
}

/// `text` cut to [`MESSAGE_PREVIEW_CHARS`], marking that it was cut (counted in chars).
fn cut(text: &str) -> String {
    if text.chars().count() <= MESSAGE_PREVIEW_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MESSAGE_PREVIEW_CHARS - 1).collect();
    format!("{kept}…")
}

/// Cancels a [`CodingAgentSession`]'s running turn (`session/cancel`); the turn then ends as
/// cancelled. Cloneable and `Send`, so it can be held apart from the session. It does not keep the
/// agent process alive: once the session is dropped there is nothing left to cancel.
#[derive(Clone)]
pub struct CancelHandle {
    driver: Weak<AcpDriver>,
    session_id: String,
}

impl CancelHandle {
    pub async fn cancel(&self) -> Result<(), SubagentError> {
        let Some(driver) = self.driver.upgrade() else {
            return Ok(());
        };
        driver
            .cancel(&self.session_id)
            .await
            .map_err(|e| SubagentError::from(format!("could not cancel the coding agent: {e}")))
    }
}

/// Refuse the request options that act on an OpenAI-loop transcript, naming the first one set.
fn refuse_loop_only(request: &TurnRequest) -> Result<(), SubagentError> {
    let refused = [
        (request.requested_max_turns().is_some(), "maxTurns"),
        (!request.yield_conditions().is_empty(), "yieldConditions"),
        (request.rewind_point().is_some(), "fromMessageId"),
        (request.correction().is_some(), "correction"),
        (request.replacement().is_some(), "replacement"),
    ];
    match refused.iter().find(|(set, _)| *set) {
        Some((_, option)) => Err(SubagentError::unsupported_by_coding_agent(option)),
        None => Ok(()),
    }
}

#[async_trait]
impl SubagentSession for CodingAgentSession {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        refuse_loop_only(&request)?;
        let prompt = request.prompt_text().ok_or_else(|| {
            SubagentError::from(
                "a coding agent has nothing to resume: it takes a new prompt each turn",
            )
        })?;
        let updates = Arc::new(Mutex::new(Vec::new()));
        let sink_updates = Arc::clone(&updates);
        let sink: tddy_agent_backend::AcpUpdateSink = Arc::new(move |update| {
            sink_updates
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(update);
        });
        let end = self
            .driver
            .prompt(&self.acp_session_id, prompt.to_string(), Some(sink), None)
            .await
            .map_err(|e| SubagentError::from(format!("the coding agent's turn failed: {e}")))?;
        let stop_reason = match end.stop_reason {
            AcpStopReason::EndTurn => StopReason::EndTurn,
            AcpStopReason::MaxTokens => StopReason::MaxTokens,
            AcpStopReason::MaxTurnRequests => StopReason::MaxTurnRequests,
            AcpStopReason::Cancelled => StopReason::Cancelled,
            AcpStopReason::Refusal => {
                return Err(SubagentError::from("the coding agent refused the turn"))
            }
        };
        let updates = std::mem::take(&mut *updates.lock().unwrap_or_else(|e| e.into_inner()));
        let messages = self.describe_turn(updates, &end.text);
        self.history.push(format!("user: {}", cut(prompt)));
        self.history.push(format!("assistant: {}", cut(&end.text)));
        let mut outcome = PromptOutcome::new(
            stop_reason,
            vec![ContentBlock::text(end.text)],
            TokenUsage::default(),
        );
        outcome.messages = messages;
        Ok(outcome)
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// ACP reports no token usage, so there is nothing to total.
    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    /// ACP reports no context occupancy.
    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, max_messages: usize) -> Vec<String> {
        let skip = self.history.len().saturating_sub(max_messages);
        self.history[skip..].to_vec()
    }
}
