//! Running a roster agent this process holds no def for.
//!
//! A roster entry carries no endpoint, credential or turn budget — deliberately, so editing a def
//! cannot change what a running session may call. This process can therefore only run the loop of
//! an agent it was *seeded* with; every other one — an agent owned by another daemon, and a local
//! one attached after spawn — is run by asking the **facilitating** daemon to run it, over the same
//! transport the roster stream and the exec tools already use
//! (docs/ft/daemon/session-agent-roster.md § Invoking an agent).
//!
//! The four RPCs mirror the MCP surface one for one, so nothing here decides policy: which agents
//! are addressable is the roster's, and resolving the entry to an owning daemon is the daemon's.
//! What is here is the wire: one request per call, and what its frames mean.
//!
//! Three of those meanings are load-bearing, and the first two are about a partial answer:
//!
//! - a turn ends with exactly one frame marked `last`, so a stream that ends without one was
//!   **truncated** and is refused rather than handed back as a complete answer;
//! - a stop reason this build cannot spell is refused naming the spelling, because reading an
//!   unknown one as `EndTurn` would report a turn that was cut short as one that finished;
//! - a message **role** this build cannot spell is refused the same way. A descriptor is what a
//!   caller picks a rewind point out of, and one attributed to the wrong speaker would have it
//!   send the conversation back to a message it never meant.

use std::sync::Arc;

use crate::openai::TokenUsage;
use crate::subagent::{
    ContentBlock, MessageDescriptor, MessageId, MessageRole, PromptOutcome, StopReason,
    SubagentError, SubagentSession, TurnRequest,
};
use prost::Message;
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AgentMessageDescriptor, CancelAgentConversationRequest,
    OpenAgentConversationRequest, OpenAgentConversationResponse, PromptAgentConversationRequest,
    ReportAgentConversationStateRequest, ResumeAgentConversationRequest,
};
use tddy_service::proto::types::SessionAgentStatus;

use tddy_session_tool_client::SessionToolEnvelope;

/// The coordinate the four conversation RPCs are served at, read from `tddy-service` so this
/// client and the daemon serving it cannot disagree about the name.
const SESSION_AGENT_SERVICE: &str = tddy_service::session_agents::SESSION_AGENT_SERVICE;

/// A connection to the session's facilitating daemon, plus the identity it authenticates.
///
/// Held rather than reconnected per call: a conversation outlives the call that opened it, and over
/// the sandbox socket a fresh connection per prompt would leave the daemon's half addressed by a
/// channel nobody is reading.
pub struct AgentConversationLink {
    client: Arc<dyn tddy_rpc::RpcClientTransport>,
    envelope: SessionToolEnvelope,
}

impl AgentConversationLink {
    pub fn new(
        client: Arc<dyn tddy_rpc::RpcClientTransport>,
        envelope: SessionToolEnvelope,
    ) -> Self {
        Self { client, envelope }
    }

    /// Connect to the facilitating daemon over whichever transport this session was spawned with.
    pub async fn connect() -> Result<Self, String> {
        let transport = tddy_session_tool_client::detect_session_tool_transport()
            .ok_or_else(|| NO_TRANSPORT.to_string())?;
        let (client, envelope) = super::link::connect_facilitating_daemon(&transport).await?;
        Ok(Self::new(client, envelope))
    }

    /// Open a conversation with `agent_id`, returning the id it is addressed by.
    ///
    /// `conversation_id` is the caller's choice where it has one: an open that times out still
    /// leaves it able to name — and therefore cancel — whatever the daemon built. Empty means the
    /// daemon mints one.
    pub async fn open(&self, agent_id: &str, conversation_id: &str) -> Result<String, String> {
        let request = OpenAgentConversationRequest {
            session_token: self.envelope.session_token.clone(),
            session_id: self.envelope.session_id.clone(),
            daemon_instance_id: self.envelope.daemon_instance_id.clone(),
            agent_id: agent_id.to_string(),
            conversation_id: conversation_id.to_string(),
        };
        let bytes = self
            .client
            .call_unary(
                SESSION_AGENT_SERVICE,
                "OpenAgentConversation",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("OpenAgentConversation: {e}"))?;
        let response = OpenAgentConversationResponse::decode(bytes.as_slice())
            .map_err(|e| format!("OpenAgentConversation decode response: {e}"))?;
        Ok(response.conversation_id)
    }

    /// Ask `conversation_id` something new and return the agent's whole answer.
    pub async fn prompt(
        &self,
        conversation_id: &str,
        prompt: &str,
    ) -> Result<PromptOutcome, String> {
        self.take_turn(conversation_id, &TurnRequest::prompting(prompt))
            .await
    }

    /// Run one turn of `conversation_id` and return the agent's whole answer.
    ///
    /// Which RPC carries the turn is decided by the request, not by the caller: a turn that asks
    /// something new is a `PromptAgentConversation`, and one that continues what the conversation
    /// already holds is a `ResumeAgentConversation`. Both carry the caller's turn budget, and the
    /// resume carries the rewind point and the correction as well.
    pub async fn take_turn(
        &self,
        conversation_id: &str,
        request: &TurnRequest,
    ) -> Result<PromptOutcome, String> {
        let call = self.turn_call(conversation_id, request)?;
        let mut frames = self
            .client
            .call_server_stream(SESSION_AGENT_SERVICE, call.method(), call.encode())
            .await
            .map_err(|e| format!("{}: {e}", call.method()))?;

        let method = call.method();
        let mut answer = String::new();
        // TODO(livekit-rpc-deadline): this await is unbounded, as every other client-side stream in
        // this crate is — a daemon that dies mid-turn hangs the turn rather than failing it. The
        // fix belongs on `tddy_rpc`'s client engine, not in a timeout wrapped around this loop;
        // recorded in docs/dev/TODO.md, "No LiveKit RPC call has a client-side deadline".
        while let Some(frame) = frames.recv().await {
            let frame = frame.map_err(|e| format!("{method} stream: {e}"))?;
            let frame = AgentConversationChunk::decode(frame.as_slice())
                .map_err(|e| format!("{method} decode frame ({} bytes): {e}", frame.len()))?;
            answer.push_str(&frame.content_chunk);
            if !frame.last {
                continue;
            }
            // The turn ran on the owning daemon, which accounts for what it spent; nothing was
            // spent here. Reporting the daemon's total as this process's would double-count it in
            // the session's own accounting file.
            let mut outcome = PromptOutcome::new(
                parse_stop_reason(&frame.stop_reason)?,
                vec![ContentBlock::text(answer)],
                TokenUsage::default(),
            );
            // The ids are the *owning* daemon's, minted against the history it holds, and they are
            // passed through untouched: a caller reads one to choose a rewind point, and an id
            // this host renumbered would address a different message than the one it described.
            outcome.messages = frame
                .messages
                .iter()
                .map(parse_message_descriptor)
                .collect::<Result<Vec<_>, String>>()?;
            outcome.clamped_max_turns = frame.clamped_max_turns;
            return Ok(outcome);
        }
        Err(format!(
            "{method} for conversation '{conversation_id}' was truncated: the stream ended after \
             {} bytes with no final frame, so the answer is partial",
            answer.len()
        ))
    }

    /// The call `request` travels as, or the reason no RPC on this coordinate can carry it.
    ///
    /// Refused by name rather than degraded. `PromptAgentConversation` has no rewind point and no
    /// correction, so a prompt that also rewinds would run as an ordinary prompt: the caller asked
    /// to go back and the agent would carry on instead, which is the class of silent substitution
    /// this whole surface exists to remove. Nothing can build that combination through the MCP
    /// tools — `subagent_prompt` takes no `fromMessageId` and `subagent_resume` sends no prompt —
    /// so it is refused here rather than given a wire shape with no caller.
    fn turn_call(&self, conversation_id: &str, request: &TurnRequest) -> Result<TurnCall, String> {
        let Some(prompt) = request.prompt_text() else {
            return Ok(TurnCall::Resume(ResumeAgentConversationRequest {
                session_token: self.envelope.session_token.clone(),
                session_id: self.envelope.session_id.clone(),
                daemon_instance_id: self.envelope.daemon_instance_id.clone(),
                conversation_id: conversation_id.to_string(),
                from_message_id: request.rewind_point().map(MessageId::to_string),
                correction: request.correction().map(str::to_string),
                max_turns: request.requested_max_turns(),
            }));
        };
        if let Some(rewind_point) = request.rewind_point() {
            return Err(format!(
                "cannot rewind to message '{rewind_point}' and send a new prompt in one turn: \
                 PromptAgentConversation carries no rewind point, and ResumeAgentConversation \
                 sends no prompt — resume with a correction instead"
            ));
        }
        if let Some(correction) = request.correction() {
            return Err(format!(
                "cannot send a new prompt and a correction in one turn: \
                 PromptAgentConversation carries no correction ('{correction}') — send the \
                 correction as the prompt, or resume with it"
            ));
        }
        Ok(TurnCall::Prompt(PromptAgentConversationRequest {
            session_token: self.envelope.session_token.clone(),
            session_id: self.envelope.session_id.clone(),
            daemon_instance_id: self.envelope.daemon_instance_id.clone(),
            conversation_id: conversation_id.to_string(),
            prompt: prompt.to_string(),
            max_turns: request.requested_max_turns(),
        }))
    }

    /// Close `conversation_id` on the daemon holding it.
    pub async fn cancel(&self, conversation_id: &str) -> Result<(), String> {
        let request = CancelAgentConversationRequest {
            session_token: self.envelope.session_token.clone(),
            session_id: self.envelope.session_id.clone(),
            daemon_instance_id: self.envelope.daemon_instance_id.clone(),
            conversation_id: conversation_id.to_string(),
        };
        self.client
            .call_unary(
                SESSION_AGENT_SERVICE,
                "CancelAgentConversation",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("CancelAgentConversation: {e}"))?;
        Ok(())
    }

    /// Tell the facilitating daemon what a turn loop running **here** is doing.
    ///
    /// The daemon infers a status for every agent whose loop *it* runs, from the three RPCs above.
    /// An agent this process was seeded with runs its loop here, so the daemon is never asked to
    /// open anything and the roster row would sit at UNSPECIFIED for an agent that is demonstrably
    /// working. This is the only source for that row.
    ///
    /// Best-effort by design, and the caller is expected to ignore the error: a status is a display
    /// signal, and failing a turn because a badge could not be updated would trade a stale badge for
    /// a broken conversation. The error is returned rather than swallowed here only so the caller
    /// can log it.
    pub async fn report_state(
        &self,
        agent_id: &str,
        status: SessionAgentStatus,
        summary: &str,
    ) -> Result<(), String> {
        let request = ReportAgentConversationStateRequest {
            session_token: self.envelope.session_token.clone(),
            session_id: self.envelope.session_id.clone(),
            daemon_instance_id: self.envelope.daemon_instance_id.clone(),
            agent_id: agent_id.to_string(),
            status: status as i32,
            summary: summary.to_string(),
        };
        self.client
            .call_unary(
                SESSION_AGENT_SERVICE,
                "ReportAgentConversationState",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("ReportAgentConversationState: {e}"))?;
        Ok(())
    }

    /// An already-opened conversation as the [`SubagentSession`] the rest of the MCP surface runs.
    ///
    /// `model` comes from the roster entry rather than from the daemon: the conversation RPCs carry
    /// no model, and a remote conversation recorded as modelless would be indistinguishable in the
    /// accounting file from one whose def named none.
    pub fn session(self: &Arc<Self>, conversation_id: String, model: &str) -> RemoteAgentSession {
        RemoteAgentSession {
            link: Arc::clone(self),
            conversation_id,
            model: model.to_string(),
        }
    }

    /// A handle to an already-opened conversation, for closing it later.
    pub fn handle(self: &Arc<Self>, conversation_id: String) -> RemoteConversationHandle {
        RemoteConversationHandle {
            link: Arc::clone(self),
            conversation_id,
        }
    }
}

/// An open conversation on the facilitating daemon, as the side that opened it must close it.
///
/// Held apart from [`RemoteAgentSession`] because the two are needed at different times: the
/// session is boxed behind [`SubagentSession`] and prompted for the life of the conversation, while
/// closing it is a call on the link that no trait method carries. Without this the daemon keeps the
/// turn loop — and, for a remote agent, the owning daemon's session behind it — for the life of the
/// process, however the caller ended the conversation here.
#[derive(Clone)]
pub struct RemoteConversationHandle {
    link: Arc<AgentConversationLink>,
    conversation_id: String,
}

impl RemoteConversationHandle {
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    /// Close the conversation on the daemon holding it.
    pub async fn cancel(&self) -> Result<(), String> {
        self.link.cancel(&self.conversation_id).await
    }
}

/// What a session with no daemon in the loop is told when it asks for an agent it cannot run.
pub const NO_TRANSPORT: &str =
    "no session-tool transport is configured, so this session has no daemon to run the agent";

/// One open conversation with an agent another daemon runs, addressed exactly as a local one is.
pub struct RemoteAgentSession {
    link: Arc<AgentConversationLink>,
    conversation_id: String,
    model: String,
}

#[async_trait::async_trait]
impl SubagentSession for RemoteAgentSession {
    /// The turn runs on the daemon holding the conversation, whichever shape it takes. What comes
    /// back is the same [`PromptOutcome`] a local session returns — message descriptors and the
    /// clamped budget included — so nothing above this layer can tell which host ran the loop.
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        self.link
            .take_turn(&self.conversation_id, &request)
            .await
            .map_err(SubagentError::from)
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// Always zero: the turn ran on the owning daemon, and its tokens are accounted for there. See
    /// [`AgentConversationLink::prompt`].
    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    /// Always zero: the history is the owning daemon's, and no conversation RPC reports what it
    /// costs to send. Zero here means "this side cannot see it", not "the window is empty" — the
    /// daemon running the loop is where that conversation's occupancy is visible.
    ///
    /// TODO: carry occupancy on `PromptAgentConversation`'s response so a remote conversation is
    /// as readable as a local one.
    fn context_tokens(&self) -> u64 {
        0
    }

    /// Always empty: the transcript lives on the daemon running the loop, and no conversation RPC
    /// returns it. Empty rather than a placeholder line — a caller reading a tail wants the
    /// exchanges as they happened, and inventing one would be worse than reporting none.
    ///
    /// TODO: add a conversation-tail RPC so a remote conversation can be read the same way.
    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }
}

/// One turn on its way to the daemon, as the RPC that carries it.
///
/// The two are not interchangeable and the difference is the point: a resume with no prompt sent
/// down `PromptAgentConversation` would ask the agent an empty question, and a peer too old to
/// serve `ResumeAgentConversation` answers `not_found` — closed and loud — rather than taking one
/// more turn forward while the caller believes it went back.
enum TurnCall {
    Prompt(PromptAgentConversationRequest),
    Resume(ResumeAgentConversationRequest),
}

impl TurnCall {
    fn method(&self) -> &'static str {
        match self {
            Self::Prompt(_) => "PromptAgentConversation",
            Self::Resume(_) => "ResumeAgentConversation",
        }
    }

    fn encode(&self) -> Vec<u8> {
        match self {
            Self::Prompt(request) => request.encode_to_vec(),
            Self::Resume(request) => request.encode_to_vec(),
        }
    }
}

/// One message descriptor as the daemon holding the history described it.
fn parse_message_descriptor(
    described: &AgentMessageDescriptor,
) -> Result<MessageDescriptor, String> {
    Ok(MessageDescriptor {
        id: MessageId::from(described.id.clone()),
        role: parse_message_role(&described.role)?,
        // Empty is the wire's spelling of "no tool", since proto3 has no absent string. Mapped
        // back to `None` here so a descriptor built from the wire is indistinguishable from one
        // built locally — a caller must not be able to tell which host ran the turn.
        tool: Some(described.tool.clone()).filter(|tool| !tool.is_empty()),
        tool_calls: described.tool_calls.clone(),
        is_error: described.is_error,
        preview: described.preview.clone(),
    })
}

/// The wire spelling of a message's role, as the daemon writes it (`agent_message_role`).
///
/// An unknown spelling is an error rather than a default, for the reason [`parse_stop_reason`]
/// gives one: the two builds disagree about the history, and a message attributed to the wrong
/// speaker is one a caller could rewind to believing it was something else.
fn parse_message_role(role: &str) -> Result<MessageRole, String> {
    match role {
        "system" => Ok(MessageRole::System),
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        other => Err(format!(
            "a turn described one of its messages as role '{other}', which this build does not \
             recognise — the two hosts disagree about what a conversation holds"
        )),
    }
}

/// The wire spelling of a stop reason, as the daemon writes it (`agent_stop_reason`).
///
/// An unknown spelling is an error rather than a default: the two builds disagree about what ended
/// the turn, and reading it as `EndTurn` would report a turn that was cut short as one that
/// finished.
fn parse_stop_reason(reason: &str) -> Result<StopReason, String> {
    match reason {
        "EndTurn" => Ok(StopReason::EndTurn),
        "MaxTurnRequests" => Ok(StopReason::MaxTurnRequests),
        "Cancelled" => Ok(StopReason::Cancelled),
        // The wire spelling `tddy-session-agents` writes for a turn the daemon ended because the
        // model's context window was full.
        "ContextExhausted" => Ok(StopReason::ContextExhausted),
        other => Err(format!(
            "PromptAgentConversation ended with stop reason '{other}', which this build does not \
             recognise — the two hosts disagree about how a turn ends"
        )),
    }
}
