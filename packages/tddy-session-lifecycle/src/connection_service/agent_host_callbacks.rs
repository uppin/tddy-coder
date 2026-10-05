//! What the agent topic needs from the session host that is not one of its fields, and the owned
//! handle that carries both.
//!
//! The agent roster, its clones and agent-def resolution read a fixed set of host fields
//! ([`tddy_session_agents::AgentRosterState`] lends them for one call). Five capabilities are not
//! fields, and all of them live in wiring above this topic: [`AgentHostCallbacks`] is how the
//! topic reaches them without naming the host. [`AgentRoster`] is the same fields **owned**, plus
//! those callbacks, for the places a borrow cannot go: a `tokio::spawn`, a `'static` closure, a
//! guard that outlives the call.
//!
//! Its fields carry the host's names, so a method that moves from `impl DaemonSessionHost` to
//! `impl AgentRoster` changes its `impl` header and nothing in the body, and the `self.clone()` it
//! hands to a task is textually the same.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_livekit::livekit_rooms_stream::RoomRoster;
use tddy_daemon_livekit::peer_routing::PeerRouting;
use tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry;
use tddy_daemon_livekit::session_room::{OpenedSessionRoom, SessionRoomRegistry, WorktreeSnapshot};
use tddy_model_registry::ModelRegistryStore;
use tddy_rpc::Status;
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_session_agents::session_agent_clone::{
    HostedAgentClones, HostedClone, SessionAgentCloneStore,
};
use tddy_session_agents::session_agent_roster::SessionAgentRosterStore;
use tddy_session_agents::AgentRosterState;

/// The capabilities of the session host the agent topic calls and does not own.
///
/// Implemented once, on the host, in wiring (`svc_agent_host_ports`).
#[async_trait::async_trait]
pub(crate) trait AgentHostCallbacks: Send + Sync {
    /// Measure a checkout that lives on a peer: the same answer a caller's own
    /// `GetWorktreeSnapshot` gets, peer routing and blocking-pool budget included.
    // TODO(#carve 18/21): drop this allowance once `join_split_livekit_room` (T4, the split
    // topic) calls it over the handle.
    #[allow(dead_code)]
    async fn worktree_snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status>;

    /// Run one exec tool on this daemon, through its task registry and jails (see
    /// `LocalExecTools::run_exec_tool_locally`).
    async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse;

    /// The clone this daemon hosts for `session_id`, when it holds one (see
    /// `LocalExecTools::hosted_clone_for`).
    fn hosted_clone_for(&self, session_id: &str) -> Option<Arc<HostedClone>>;

    /// Serve one exec tool for a session whose checkout this daemon holds as an agent clone: the
    /// read/write split (see `LocalExecTools::run_hosted_clone_tool`).
    async fn run_hosted_clone_tool(
        &self,
        req: &ExecuteToolRequest,
        clone: &HostedClone,
    ) -> ExecuteToolResponse;

    /// Open the session's room over a checkout this daemon holds, unless it is open already (the
    /// host's `ensure_session_room`: the room's roster and its terminal bridge are the host's to
    /// supply). `Ok(None)` means this daemon has no LiveKit credentials and hosts no rooms.
    async fn ensure_session_room(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_root: &Path,
    ) -> Result<Option<OpenedSessionRoom>, Status>;
}

/// The session host's roster fields, owned, plus its callbacks.
///
/// Built per call by the host (`DaemonSessionHost::agent_roster`). Every shared field is the `Arc`
/// the host holds, so a clone of this talks to the stores, rooms and peers the host does.
#[derive(Clone)]
pub(crate) struct AgentRoster {
    pub(crate) config: DaemonConfig,
    pub(crate) tddy_data_dir: std::path::PathBuf,
    pub(crate) user_resolver: SessionUserResolver,
    pub(crate) peer_routing: PeerRouting,
    pub(crate) room_roster: Arc<dyn RoomRoster>,
    pub(crate) session_rooms: Arc<SessionRoomRegistry>,
    pub(crate) session_agent_rosters: Arc<SessionAgentRosterStore>,
    pub(crate) session_agent_clones: Arc<SessionAgentCloneStore>,
    pub(crate) hosted_agent_clones: Arc<HostedAgentClones>,
    pub(crate) roster_keepalive_interval: Duration,
    pub(crate) session_admissions: Arc<SessionAdmissionRegistry>,
    pub(crate) model_registry: Option<Arc<ModelRegistryStore>>,
    /// The host's capabilities that are not fields.
    pub(crate) host: Arc<dyn AgentHostCallbacks>,
}

impl AgentRoster {
    /// The fields as the borrowed view the functions in `tddy-session-agents` take.
    pub(crate) fn state(&self) -> AgentRosterState<'_> {
        AgentRosterState {
            config: &self.config,
            tddy_data_dir: &self.tddy_data_dir,
            user_resolver: &self.user_resolver,
            peer_routing: &self.peer_routing,
            room_roster: &self.room_roster,
            session_rooms: &self.session_rooms,
            session_agent_rosters: &self.session_agent_rosters,
            session_agent_clones: &self.session_agent_clones,
            hosted_agent_clones: &self.hosted_agent_clones,
            roster_keepalive_interval: self.roster_keepalive_interval,
            session_admissions: &self.session_admissions,
            model_registry: &self.model_registry,
        }
    }
}
