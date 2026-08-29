//! Running a roster agent this process holds no def for.
//!
//! A roster entry carries no endpoint, credential or turn budget — deliberately, so editing a def
//! cannot change what a running session may call. This process can therefore only run the loop of
//! an agent it was *seeded* with; every other one — an agent owned by another daemon, and a local
//! one attached after spawn — is run by asking the **facilitating** daemon to run it, over the same
//! transport the roster stream and the exec tools already use
//! (docs/ft/daemon/session-agent-roster.md § Invoking an agent).
//!
//! The three RPCs mirror the MCP surface one for one, so nothing here decides policy: which agents
//! are addressable is the roster's, and resolving the entry to an owning daemon is the daemon's.
//! What is here is the wire: one request per call, and what its frames mean.
//!
//! Two of those meanings are load-bearing, and both are about a partial answer:
//!
//! - a turn ends with exactly one frame marked `last`, so a stream that ends without one was
//!   **truncated** and is refused rather than handed back as a complete answer;
//! - a stop reason this build cannot spell is refused naming the spelling, because reading an
//!   unknown one as `EndTurn` would report a turn that was cut short as one that finished.

use std::sync::Arc;

use prost::Message;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    ContentBlock, PromptOutcome, StopReason, SubagentError, SubagentSession,
};
use tddy_service::proto::connection::{
    AgentConversationChunk, CancelAgentConversationRequest, OpenAgentConversationRequest,
    OpenAgentConversationResponse, PromptAgentConversationRequest,
};

use crate::session_tool_client::SessionToolEnvelope;

const CONNECTION_SERVICE: &str = "connection.ConnectionService";

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
        let transport = crate::session_tool_client::detect_session_tool_transport()
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
                CONNECTION_SERVICE,
                "OpenAgentConversation",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("OpenAgentConversation: {e}"))?;
        let response = OpenAgentConversationResponse::decode(bytes.as_slice())
            .map_err(|e| format!("OpenAgentConversation decode response: {e}"))?;
        Ok(response.conversation_id)
    }

    /// Run one turn of `conversation_id` and return the agent's whole answer.
    pub async fn prompt(
        &self,
        conversation_id: &str,
        prompt: &str,
    ) -> Result<PromptOutcome, String> {
        let request = PromptAgentConversationRequest {
            session_token: self.envelope.session_token.clone(),
            session_id: self.envelope.session_id.clone(),
            daemon_instance_id: self.envelope.daemon_instance_id.clone(),
            conversation_id: conversation_id.to_string(),
            prompt: prompt.to_string(),
        };
        let mut frames = self
            .client
            .call_server_stream(
                CONNECTION_SERVICE,
                "PromptAgentConversation",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("PromptAgentConversation: {e}"))?;

        let mut answer = String::new();
        // TODO(livekit-rpc-deadline): this await is unbounded, as every other client-side stream in
        // this crate is — a daemon that dies mid-turn hangs the prompt rather than failing it. The
        // fix belongs on `tddy_rpc`'s client engine, not in a timeout wrapped around this loop;
        // recorded in docs/dev/TODO.md, "No LiveKit RPC call has a client-side deadline".
        while let Some(frame) = frames.recv().await {
            let frame = frame.map_err(|e| format!("PromptAgentConversation stream: {e}"))?;
            let frame = AgentConversationChunk::decode(frame.as_slice()).map_err(|e| {
                format!(
                    "PromptAgentConversation decode frame ({} bytes): {e}",
                    frame.len()
                )
            })?;
            answer.push_str(&frame.content_chunk);
            if !frame.last {
                continue;
            }
            return Ok(PromptOutcome {
                stop_reason: parse_stop_reason(&frame.stop_reason)?,
                content: vec![ContentBlock::text(answer)],
                // The turn ran on the owning daemon, which accounts for what it spent; nothing was
                // spent here. Reporting the daemon's total as this process's would double-count it
                // in the session's own accounting file.
                usage: TokenUsage::default(),
            });
        }
        Err(format!(
            "PromptAgentConversation for conversation '{conversation_id}' was truncated: the \
             stream ended after {} bytes with no final frame, so the answer is partial",
            answer.len()
        ))
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
                CONNECTION_SERVICE,
                "CancelAgentConversation",
                request.encode_to_vec(),
            )
            .await
            .map_err(|e| format!("CancelAgentConversation: {e}"))?;
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
    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.link
            .prompt(&self.conversation_id, text)
            .await
            .map_err(SubagentError::from)
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// Always zero: the turn ran on the owning daemon, and its tokens are accounted for there. See
    /// [`AgentConversationLink::prompt`].
    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
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
        other => Err(format!(
            "PromptAgentConversation ended with stop reason '{other}', which this build does not \
             recognise — the two hosts disagree about how a turn ends"
        )),
    }
}
