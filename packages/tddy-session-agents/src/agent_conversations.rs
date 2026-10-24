//! The conversations open with a session's roster agents, and where each one's turn loop runs.
//!
//! Moved out of `tddy-daemon`'s `connection_service` by `#unbundle` node 7 with the four handlers
//! that drive it. It holds node 5's [`SubagentSession`] rather than re-implementing one: what this
//! module owns is *which* loop a prompt reaches and when a cancel interrupts it, not how a turn is
//! taken.

use std::collections::HashMap;
use std::sync::Arc;

use crate::conversation_recorder::ConversationRecorder;
use tddy_discovery::subagent::{PermissionAnswering, SubagentSession};
use tokio::sync::{Mutex, Notify};

/// One open conversation with a roster agent.
///
/// The two variants are what the main agent must not be able to tell apart: both answer
/// `{stopReason, content}`, and only the daemon deciding where the turn loop runs sees the
/// difference.
pub enum AgentConversation {
    /// The turn loop runs here, in this process.
    Local {
        session_id: String,
        agent_id: String,
        /// Shared rather than owned by the map, so a turn can be awaited on this lock alone with
        /// the map's lock released — a turn that pinned the map would block every cancel for its
        /// whole duration, including the cancel meant to interrupt it.
        session: Arc<Mutex<Box<dyn SubagentSession>>>,
        /// Signalled when the conversation is closed. `notify_one` rather than `notify_waiters`, so
        /// a cancel that lands between the turn being spawned and its first await is still seen.
        closed: Arc<Notify>,
        /// Records the conversation's turns as they run: the transcript a replay reads, and the frames a
        /// replay tails. Opened with the session, before its first turn.
        recorder: Arc<ConversationRecorder>,
        /// The handle its parked permission requests are answered through, captured when the
        /// conversation opens. Held beside the session rather than read through it, because a turn
        /// that is waiting on a request holds the session's lock; `None` when the session parks none.
        answering: Option<Arc<dyn PermissionAnswering>>,
    },
    /// The turn loop runs on `daemon_instance_id`; this daemon forwards to it.
    Remote {
        session_id: String,
        agent_id: String,
        daemon_instance_id: String,
    },
}

impl AgentConversation {
    /// The roster agent this conversation is with, whichever daemon runs its loop.
    #[must_use]
    pub fn agent_id(&self) -> &str {
        match self {
            AgentConversation::Local { agent_id, .. }
            | AgentConversation::Remote { agent_id, .. } => agent_id,
        }
    }

    /// The session this conversation is open on, whichever daemon runs its loop.
    #[must_use]
    pub fn session_id(&self) -> &str {
        match self {
            AgentConversation::Local { session_id, .. }
            | AgentConversation::Remote { session_id, .. } => session_id,
        }
    }

    /// Whether this conversation is with `agent_id` on `session_id`, whichever daemon runs its
    /// loop.
    #[must_use]
    pub fn is_with(&self, session_id: &str, agent_id: &str) -> bool {
        let (open_session, open_agent) = match self {
            AgentConversation::Local {
                session_id,
                agent_id,
                ..
            }
            | AgentConversation::Remote {
                session_id,
                agent_id,
                ..
            } => (session_id, agent_id),
        };
        open_session == session_id && open_agent == agent_id
    }
}

/// What one open conversation hands a prompt, taken out of the map so the map's lock can be
/// released before the turn is awaited.
pub enum PromptRouting {
    Local {
        session: Arc<Mutex<Box<dyn SubagentSession>>>,
        closed: Arc<Notify>,
        recorder: Arc<ConversationRecorder>,
        /// The handle the conversation's parked permission requests are answered through, `None` when
        /// the session parks none. Carried here so a permission answer can reach a request while a turn
        /// holds the session, without taking the map's lock again.
        answering: Option<Arc<dyn PermissionAnswering>>,
    },
    Remote(String),
}

/// What closing a roster agent's conversations left for its caller to finish.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ClosedConversations {
    /// The conversations whose turn loops ran here, now closed.
    pub local: Vec<String>,
    /// The `(owning daemon, conversation id)` pairs whose loops run on another daemon, to cancel there.
    pub remote: Vec<(String, String)>,
}

/// Every conversation open on this host, keyed by conversation id.
///
/// One map for the whole host rather than one per session: a conversation id is what a caller names
/// in a prompt and a cancel, and the request carries the session it belongs to only so the status
/// can be recorded against the right roster.
#[derive(Default)]
pub struct OpenAgentConversations {
    open: Mutex<HashMap<String, AgentConversation>>,
}

impl OpenAgentConversations {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember one conversation under `conversation_id`.
    pub async fn insert(&self, conversation_id: String, conversation: AgentConversation) {
        self.open.lock().await.insert(conversation_id, conversation);
    }

    /// What a prompt to `conversation_id` reaches, with the agent it is with.
    ///
    /// The routing and the agent id come out together, under one lock, and the guard is dropped
    /// before anything is awaited on them: the request names a conversation, not an agent, and the
    /// status is recorded per agent.
    pub async fn routing_for(&self, conversation_id: &str) -> Option<(PromptRouting, String)> {
        let open = self.open.lock().await;
        match open.get(conversation_id)? {
            AgentConversation::Local {
                session,
                closed,
                recorder,
                answering,
                agent_id,
                ..
            } => Some((
                PromptRouting::Local {
                    session: Arc::clone(session),
                    closed: Arc::clone(closed),
                    recorder: Arc::clone(recorder),
                    answering: answering.clone(),
                },
                agent_id.clone(),
            )),
            AgentConversation::Remote {
                daemon_instance_id,
                agent_id,
                ..
            } => Some((
                PromptRouting::Remote(daemon_instance_id.clone()),
                agent_id.clone(),
            )),
        }
    }

    /// Whether `conversation_id` is open on `session_id`. The map is host-wide, so the session a
    /// conversation was opened on is checked here too: a conversation another session opened is not
    /// one this session may act on. Takes only the map's lock, never a session's.
    pub async fn is_open_on(&self, session_id: &str, conversation_id: &str) -> bool {
        self.open
            .lock()
            .await
            .get(conversation_id)
            .is_some_and(|conversation| conversation.session_id() == session_id)
    }

    /// The handle `conversation_id`'s parked permission requests are answered through, when it is a
    /// conversation open on `session_id` whose turns run here and whose session parks requests.
    /// `None` for a conversation another daemon runs the loop of. Takes only the map's lock, never a
    /// session's, so an answer reaches a turn that is waiting on it.
    pub async fn answering_for(
        &self,
        session_id: &str,
        conversation_id: &str,
    ) -> Option<Arc<dyn PermissionAnswering>> {
        match self.open.lock().await.get(conversation_id)? {
            AgentConversation::Local {
                session_id: open_session,
                answering,
                ..
            } if open_session == session_id => answering.clone(),
            AgentConversation::Local { .. } | AgentConversation::Remote { .. } => None,
        }
    }

    /// Forget `conversation_id`, handing back what was open under it.
    pub async fn remove(&self, conversation_id: &str) -> Option<AgentConversation> {
        self.open.lock().await.remove(conversation_id)
    }

    /// Forget every conversation with `agent_id` on `session_id`, interrupting the local ones and
    /// reporting the remote ones so their owning daemon can be told too.
    ///
    /// An in-flight local turn is interrupted rather than left to finish: the caller is the main
    /// agent, and a truncated answer accepted as whole is the failure that reaches the operator as
    /// a wrong review. A conversation whose loop runs on another daemon has to be cancelled
    /// *there* as well — dropping the routing record alone would leave that daemon's turn loop
    /// running against a clone the detach is about to delete, with nothing left on this side able
    /// to name it.
    ///
    /// Returns the conversations that were closed here, and the `(owning daemon, conversation id)`
    /// pairs the caller still has to cancel remotely, because delivering them needs the common room
    /// and this map has no transport.
    pub async fn close_every_conversation_with(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> ClosedConversations {
        let mut closed = ClosedConversations::default();
        let mut open = self.open.lock().await;
        open.retain(|conversation_id, conversation| {
            if !conversation.is_with(session_id, agent_id) {
                return true;
            }
            match conversation {
                AgentConversation::Local { closed: stop, .. } => {
                    stop.notify_one();
                    closed.local.push(conversation_id.clone());
                }
                AgentConversation::Remote {
                    daemon_instance_id, ..
                } => closed
                    .remote
                    .push((daemon_instance_id.clone(), conversation_id.clone())),
            }
            false
        });
        closed
    }
}
