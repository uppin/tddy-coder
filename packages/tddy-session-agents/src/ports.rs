//! What this crate needs from the host it runs on, and nothing it can decide for itself.
//!
//! Every port below answers a question only a *daemon* can answer: which directory a session token
//! may reach, which def an agent id resolves to on this host, whether a checkout on a peer could be
//! claimed, and how a roster change reaches a room. The nine handlers own the ordering, the
//! refusals and the roster itself — see [`crate::service`] — and none of them reaches for a peer, a
//! room or a config, because none of them can from here.
//!
//! The shape follows `tddy_session_files::SessionFilesPorts`, which node 6 established for the same
//! reason: a struct rather than a dozen positional parameters, each field carrying why the daemon
//! supplies it rather than this subsystem choosing it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::SessionAgentRecord;
use tddy_discovery::subagent::SubagentSession;
use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, OpenAgentConversationRequest, PromptAgentConversationRequest,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::agent_conversations::OpenAgentConversations;
use crate::session_agent_clone::SessionAgentCloneStore;
use crate::session_agent_roster::SessionAgentRosterStore;

/// Resolves one request's `(session_token, session_id)` to the session directory it may read.
///
/// A port rather than a path this crate derives, and a closure rather than a base directory,
/// because both halves of the answer are the daemon's: the token maps to an OS user through the
/// `users[]` table in its config, and the session id has to be validated as one path segment before
/// it becomes one. A caller holding a valid token for one of its own sessions must not be able to
/// name a root it does not own, and only the side holding the mapping can refuse that.
pub type SessionDirResolver = Arc<dyn Fn(&str, &str) -> Result<PathBuf, Status> + Send + Sync>;

/// The roster entry a qualified `agent_id` attaches as.
///
/// A port because resolving an id reads this host's `<tddyhome>/agents` defs *and* its model
/// registry's assistants, and — for an id naming a peer — that peer's own `ListSubagents` over the
/// common room. There is deliberately no "assume the local daemon" reading, which is the reading
/// that silently picks the wrong host the moment two daemons offer a def of the same name; the
/// refusal that says so is the daemon's to raise, because only it knows what it can resolve.
#[async_trait]
pub trait AgentCatalog: Send + Sync {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status>;
}

/// One agent admitted to a session, and the checkout that admission claimed for it.
///
/// [`Self::commissioned`] is what makes a failed attach unwindable without taking a checkout away
/// from an agent that is still using it: two agents on one host share one clone, and only the
/// attach that minted it may delete it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedAgent {
    /// The owning daemon this admission was settled against.
    pub daemon_instance_id: String,
    /// The `workspace` session on that daemon holding its checkout. `None` when the owner is this
    /// host — a local agent works the real worktree and there is nothing to claim.
    pub codebase_session_id: Option<String>,
    /// Whether this admission commissioned that checkout, and so may take it away again.
    pub commissioned: bool,
}

/// What an attach has to settle on this host before a roster entry may be written, and how that is
/// taken back.
///
/// A port rather than three calls this crate makes, because every one of them is the daemon's:
/// [`Self::admit`] reads the session's `.session.yaml` to decide whether a withdrawal is actually
/// *enforced* against its main agent — a session that runs no jail holds native file tools that
/// never reach `tddy-tools`, so accepting the attach would advertise an enforcement that does not
/// exist — and then opens the session's LiveKit room and commissions a checkout on the owning peer.
/// [`Self::withdraw`] and [`Self::tear_down`] undo the two halves of that over the common room.
///
/// The crate owns the *order*: admit before the roster is written, withdraw when the write failed,
/// tear down only after the entry is gone and persisted. That order is the contract — "no roster
/// entry, no half-built clone, no room membership" — and it is why these are three methods rather
/// than one.
#[async_trait]
pub trait AgentAdmission: Send + Sync {
    /// Settle whether `record` may attach to `session_id`, claiming whatever checkout its owner
    /// needs first.
    async fn admit(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
    ) -> Result<AdmittedAgent, Status>;

    /// Hand back an admission whose roster entry could not be written.
    ///
    /// Infallible by signature: the roster write has already failed and its error is what the
    /// caller is about to return, so a second failure here has nowhere to go but the log.
    async fn withdraw(&self, session_id: &str, admitted: &AdmittedAgent, session_token: &str);

    /// Delete the checkout `codebase_session_id` names on `daemon_instance_id`.
    ///
    /// Fallible, unlike [`Self::withdraw`]: this runs on the detach path, where the roster entry is
    /// already gone, so a checkout left behind is something an operator has to be told about by
    /// name.
    async fn tear_down(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status>;
}

/// Where a roster change is announced beyond this crate's own subscribers.
///
/// `StreamSessionAgents` is fed by [`SessionAgentRosterStore`] itself, which is in this crate. What
/// is not is the session **room**: a whole snapshot is published on its `session.agents` topic for
/// participants rebuilding a registry — a browser tab, a newly admitted owning daemon — and the
/// room, its publisher and its LiveKit connection are the daemon's transport.
#[async_trait]
pub trait RosterBroadcast: Send + Sync {
    /// Broadcast one snapshot on the session room's `session.agents` topic. Best-effort: a room
    /// that is not open is the ordinary state of a session on a daemon with no LiveKit config.
    ///
    /// The payload is the `connection` form of the message, not this service's, because that is the
    /// **broadcast** schema (`tddy_service::session_agents`): the in-jail `tddy-tools` and every
    /// browser tab already subscribed to the topic decode it, and a publisher that switched schema
    /// ahead of them would deliver frames none of them could read — which fails as *silence*, since
    /// a receiver that cannot decode a frame drops it.
    async fn broadcast(
        &self,
        session_id: &str,
        roster: &tddy_service::proto::session_agents_svc::SessionAgentRoster,
    );
}

/// The agent turn loops this host can open, and the two refusals that decide whether it should.
///
/// A port because opening one resolves a def — through the *spawn* resolver, so a registry
/// assistant comes back carrying its provider's credential — and hands it the session's exec-tool
/// transport. Both are the daemon's: the credential store is its config, and the transport is the
/// jail or the proxy it built for that session.
#[async_trait]
pub trait AgentSessions: Send + Sync {
    /// Whether this host holds a clone for `session_id` — i.e. the session belongs to a *peer* and
    /// this host is the one keeping its checkout.
    ///
    /// What it decides: a status recorded against such a session is one nothing will ever read,
    /// because the roster naming the agent is on the daemon facilitating it.
    fn hosts_a_clone_for(&self, session_id: &str) -> bool;

    /// A turn loop for an agent this host **owns**, reading the clone it holds for a peer's
    /// session. `None` when it holds no such clone, which is when the roster decides instead.
    async fn open_owned(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status>;

    /// A turn loop for an agent resolved on this host, reading the session's own worktree.
    async fn open_local(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
    ) -> Result<Box<dyn SubagentSession>, Status>;

    /// Refuse a prompt to an agent whose checkout is not ready to serve reads, naming the state.
    ///
    /// The daemon's because the clone states are what *it* recorded from the owning peer's reports.
    fn refuse_unready_clone(
        &self,
        session_id: &str,
        record: &SessionAgentRecord,
    ) -> Result<(), Status>;

    /// Refuse to address an owning daemon that is no longer in the common room.
    ///
    /// Named rather than left to a forward deadline: an agent on a departed daemon must fail *its
    /// own* prompts with an error naming that daemon, while the rest of the roster keeps working.
    async fn refuse_departed_owner(&self, daemon_instance_id: &str) -> Result<(), Status>;
}

/// Forwarding a conversation to the daemon that owns the agent.
///
/// Distinct from the daemon's *routing* preamble, which stays in the daemon and follows the
/// `daemon_instance_id` the request named. This one follows the **agent's** owner, read out of the
/// roster entry after the roster holding it has been read — so it is a decision this crate makes
/// and a delivery only the daemon can perform.
#[async_trait]
pub trait AgentConversationPeers: Send + Sync {
    /// Ask the owning daemon to open the conversation on its side, under the id this host minted.
    ///
    /// The id travels rather than being minted there, so a forward that times out still leaves this
    /// host able to name — and therefore cancel — whatever the peer opened.
    async fn open(
        &self,
        request: &OpenAgentConversationRequest,
        owner: &str,
        conversation_id: &str,
    ) -> Result<(), Status>;

    /// Prompt a conversation whose turn loop runs on `owner`, handing back its frames verbatim.
    async fn prompt(
        &self,
        request: &PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status>;

    /// Cancel a conversation whose turn loop runs on `owner`.
    async fn cancel(
        &self,
        session_token: &str,
        session_id: &str,
        owner: &str,
        conversation_id: &str,
    ) -> Result<(), Status>;
}

/// Everything the nine handlers need from the host they run on.
pub struct SessionAgentPorts {
    /// `(session_token, session_id)` to the session directory this host will serve it out of. All
    /// nine handlers start here.
    ///
    /// Supplied by the daemon because the `users[]` mapping and the segment validation it draws its
    /// refusals from are the daemon's — see [`SessionDirResolver`].
    pub session_dirs: SessionDirResolver,
    /// This daemon's own instance id, as its config names it.
    ///
    /// Supplied rather than derived because it is what tells a *local* agent from an *owned* one:
    /// `AttachSessionAgent` claims a checkout only for an agent some other daemon owns, and
    /// `OpenAgentConversation` runs the turn loop here only for one this daemon does. A second
    /// derivation of the id would be a second answer to "is this agent mine".
    pub local_instance_id: String,
    /// The cadence at which a `StreamSessionAgents` subscription re-sends an unchanged roster.
    ///
    /// Supplied by the daemon because it is a tuning of the host doing the sending, and because the
    /// suites that pin the keepalive's behaviour need it shortened — a constant here would make
    /// every one of them wait out the production cadence.
    pub roster_keepalive: Duration,
    /// The authoritative roster of every session this host facilitates, and the activity store
    /// every snapshot reads. This crate's own — [`SessionAgentRosterStore`] — but shared, because
    /// `ListSessions` reports the same rows and a seeded start writes into the same map.
    pub rosters: Arc<SessionAgentRosterStore>,
    /// The checkouts this host asked peers to build for its sessions' remote agents. Shared for the
    /// same reason: a clone report and the roster snapshot that names its state must be one store.
    pub clones: Arc<SessionAgentCloneStore>,
    /// Every conversation open on this host, keyed by conversation id.
    ///
    /// Injected rather than created by the service because it is *state*, and the old coordinate
    /// still answers the same four methods: a prompt arriving on
    /// `connection.ConnectionService` has to find the conversation an open on
    /// `session_agents.SessionAgentService` created, and two maps would answer `NOT_FOUND` for a
    /// conversation that is open. The daemon holds the one `Arc` and hands it to both.
    pub conversations: Arc<OpenAgentConversations>,
    /// What an attach must settle before a roster entry may be written — see [`AgentAdmission`].
    pub admission: Arc<dyn AgentAdmission>,
    /// Which def an agent id resolves to on this host — see [`AgentCatalog`].
    pub catalog: Arc<dyn AgentCatalog>,
    /// Where a roster change is announced beyond this crate's subscribers — see
    /// [`RosterBroadcast`].
    pub broadcast: Arc<dyn RosterBroadcast>,
    /// The turn loops this host can open, and the two refusals — see [`AgentSessions`].
    pub sessions: Arc<dyn AgentSessions>,
    /// How a conversation reaches the daemon owning the agent — see [`AgentConversationPeers`].
    pub peers: Arc<dyn AgentConversationPeers>,
}
