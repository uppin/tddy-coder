use std::sync::Arc;

use crate::connection_service::seed_codebase;

use super::ConnectionServiceImpl;

/// One agent a start has already put on a session's roster, as its unwind needs to name it.
///
/// The clone is carried rather than looked up again: only the entry that *commissioned* a checkout
/// may delete it, and that fact lives nowhere but in the claim this seed made.
pub(crate) struct SeededAgent {
    pub(crate) agent_id: String,
    pub(crate) daemon_instance_id: String,
    pub(crate) clone: Option<seed_codebase::ClaimedAgentClone>,
}

/// Holds a co-located start's claimed clones until its roster is persisted, and releases them if it
/// never is.
///
/// A co-located start writes `.session.yaml` last — it cannot, until the agent it describes has a
/// pid — and may fail at any of the steps between the claim and that write. Those steps are spread
/// over several hundred lines of three launch paths, so the release is tied to the *scope* rather
/// than repeated at each `?`: a guard that is not [`Self::keep`]-ed on the way out takes every clone
/// it was given back off the peer that built it. A checkout on another host is the one artifact of
/// a failed start that this daemon cannot clean up later.
///
/// Spawned because `Drop` cannot await, and swallowed for the reason
/// [`ConnectionServiceImpl::unwind_seeded_roster`] swallows: whatever is unwinding this already has
/// the error worth reporting.
pub struct SeededCloneGuard {
    /// What to give back, and to whom. `None` once the start has kept the clones — and from the
    /// start for a roster that named no peer's agent, which claimed nothing to give back.
    pub(crate) release: Option<SeededCloneRelease>,
}

pub(crate) struct SeededCloneRelease {
    pub(crate) service: ConnectionServiceImpl,
    pub(crate) session_id: String,
    pub(crate) session_token: String,
    pub(crate) seeded: Vec<SeededAgent>,
}

impl SeededCloneGuard {
    /// A guard over nothing: this start claimed no clone on any peer.
    pub fn nothing_claimed() -> Self {
        Self { release: None }
    }

    /// A guard for a start that is about to claim, opened before the first claim so an early
    /// return releases whatever it got through.
    pub(crate) fn claiming(
        service: ConnectionServiceImpl,
        session_id: &str,
        session_token: &str,
    ) -> Self {
        Self {
            release: Some(SeededCloneRelease {
                service,
                session_id: session_id.to_string(),
                session_token: session_token.to_string(),
                seeded: Vec::new(),
            }),
        }
    }

    /// One more clone this start is answerable for until it keeps them.
    pub(crate) fn claimed(&mut self, agent: SeededAgent) {
        if let Some(release) = self.release.as_mut() {
            release.seeded.push(agent);
        }
    }

    /// The start reached the point where its roster is persisted; the clones are the session's now.
    pub fn keep(mut self) {
        self.release = None;
    }
}

impl Drop for SeededCloneGuard {
    fn drop(&mut self) {
        let Some(SeededCloneRelease {
            service,
            session_id,
            session_token,
            seeded,
        }) = self.release.take()
        else {
            return;
        };
        tokio::spawn(async move {
            for agent in seeded.into_iter().rev() {
                let Some(clone) = &agent.clone else {
                    continue;
                };
                log::warn!(
                    "StartSession: session {session_id} did not come up; releasing the clone \
                     daemon {} was building for agent '{}'",
                    agent.daemon_instance_id,
                    agent.agent_id
                );
                service
                    .unwind_agent_clone_claim(
                        &session_id,
                        &agent.daemon_instance_id,
                        clone,
                        &session_token,
                    )
                    .await;
            }
        });
    }
}

/// What one open conversation hands a prompt, taken out of the map so the map's lock can be
/// released before the turn is awaited.
pub(crate) enum PromptRouting {
    Local {
        session: Arc<tokio::sync::Mutex<Box<dyn tddy_discovery::subagent::SubagentSession>>>,
        closed: Arc<tokio::sync::Notify>,
    },
    Remote(String),
}

impl seed_codebase::AgentConversation {
    /// The roster agent this conversation is with, whichever daemon runs its loop.
    pub(crate) fn agent_id(&self) -> &str {
        match self {
            seed_codebase::AgentConversation::Local { agent_id, .. }
            | seed_codebase::AgentConversation::Remote { agent_id, .. } => agent_id,
        }
    }

    /// Whether this conversation is with `agent_id` on `session_id`, whichever daemon runs its loop.
    pub(crate) fn is_with(&self, session_id: &str, agent_id: &str) -> bool {
        let (open_session, open_agent) = match self {
            seed_codebase::AgentConversation::Local {
                session_id,
                agent_id,
                ..
            } => (session_id, agent_id),
            seed_codebase::AgentConversation::Remote {
                session_id,
                agent_id,
                ..
            } => (session_id, agent_id),
        };
        open_session == session_id && open_agent == agent_id
    }
}

/// A live reverse stdio endpoint to one spawned tddy-coder session. Holding it keeps the pipe's
/// read/dispatch loop running; dropping it (on session teardown) ends the loop.
pub(crate) struct SessionStdioEndpoint {
    #[allow(dead_code)]
    pub(crate) client: Arc<tddy_stdio::StdioRpcClient>,
    #[allow(dead_code)]
    pub(crate) task: tokio::task::JoinHandle<()>,
}

/// Where one exec tool call of a session this daemon holds is run.
pub(crate) enum ExecToolRoute {
    /// The session's checkout on this host, through the tool engine — every session that did not
    /// ask to be confined.
    HostWorktree,
    /// The session's own jail on this host: a sandboxed `workspace` session
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox).
    Jail(Arc<dyn tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandbox>),
    /// Neither, and the call is answered with this as its error. A session recorded as sandboxed
    /// whose jail this daemon does not hold is refused rather than served from the bare host: a
    /// tool that ran unconfined on a session that asked to be confined is the one failure nobody
    /// can see afterwards.
    Refused(String),
}
