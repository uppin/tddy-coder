//! What the launch topic needs from the session host that is not one of its fields, and the owned
//! handle that carries both.
//!
//! The launch topic starts agent sessions: the stack-parent, stack-child and conversation spawns,
//! the sandboxed Claude and Cursor jails, their relaunch and resume, and the Claude CLI start. Those
//! read a fixed set of host fields (held by [`LaunchSessions`]) and reach two capabilities that are
//! not fields and live in wiring above this topic: [`LaunchHost`] is how the topic reaches them
//! without naming the host.
//!
//! [`LaunchSessions`] carries the fields the topic's bodies read, plus the roster handle, the split
//! handle, the presenter-observer deps and the two fields attachment materialization reads. Its
//! fields carry the host's names, so a method
//! that moves from `impl DaemonSessionHost` to `impl LaunchSessions` changes its `impl` header and
//! nothing in the body, and the `self.clone()` it hands to a task is textually the same. This
//! follows [`AgentRoster`](tddy_session_agents::agent_host_callbacks::AgentRoster) and
//! [`SplitSessions`](tddy_session_split::split_ports::SplitSessions) exactly.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::relay_idle::RpcActivity;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_daemon_livekit::peer_routing::PeerRouting;
use tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;
use tddy_rpc::Status;
use tddy_service::proto::session::SessionAttachment;
use tddy_session_agents::session_agent_clone::HostedAgentClones;
use tddy_session_agents::session_agent_inference::SessionAgentInferenceStore;
use tddy_spawn::spawn_worker::SpawnClient;
use tddy_task::TaskRegistry;

use tddy_session_agents::agent_host_callbacks::AgentRoster;
use super::host_session_socket::HostSessionSockets;
use super::session_worktree_observer::SessionWorktreeObserver;
use tddy_session_split::split_ports::SplitSessions;
use tddy_session_files::svc_materialize_staged_attachment::AttachmentState;
use tddy_session_files::attachment_progress::AttachmentMaterialization;
use crate::cli_session_manager::CliSessionManager;
use crate::presenter_observer_task::presenter_observer_spawn::PresenterObserverDeps;
use crate::PrStackHandler;

use super::session_acting_identity::SessionAccountAccess;
use super::session_acting_identity::SessionIdentity;

/// The capabilities of the session host the launch topic calls and does not own.
///
/// Implemented once, on the host, in wiring (`svc_agent_host_ports`).
pub(crate) trait LaunchHost: Send + Sync {
    /// The host-side dispatch a jail built for `session_id` relays family B to, bound to that
    /// session (the host's `sandbox_rpc_handler`).
    fn sandbox_rpc_handler(
        &self,
        session_id: &str,
        session_dir: &Path,
    ) -> Arc<dyn tddy_sandbox_runner::HostRpcHandler>;

    /// This daemon's PR-stack handler, which a stack base or a stack link owned by a peer is asked
    /// of, or `FAILED_PRECONDITION` when the host was never given the RPC families.
    fn pr_stack(&self) -> Result<Arc<dyn PrStackHandler>, Status>;

    /// What a session's vault reads go through: this daemon's vaults, how a session token names
    /// its owner, and the token the session was started with.
    fn session_account_access(&self, session_token: &str) -> SessionAccountAccess;

    /// The identity a session of `project_id` is launched with — its commit pairs and the handler
    /// that answers its tools' `github-token` — over the project's assignments as they stand now.
    /// Neither when the project cannot be read (logged by the host).
    fn session_identity(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> SessionIdentity;
}

/// The session host's launch fields, owned, plus its callbacks.
///
/// Built per call by the host (`DaemonSessionHost::launch_sessions`). Every shared field is the
/// `Arc` the host holds, so a clone of this talks to the managers, registries and jails the host
/// does.
#[derive(Clone)]
pub(crate) struct LaunchSessions {
    pub(crate) config: DaemonConfig,
    pub(crate) tddy_data_dir: PathBuf,
    pub(crate) staging_base_dir: PathBuf,
    pub(crate) peer_routing: PeerRouting,
    pub(crate) claude_cli_manager: Arc<CliSessionManager>,
    /// Sandboxed claude-cli sessions (darwin Seatbelt).
    pub(crate) sandbox_manager: Arc<tddy_daemon_sandbox::sandbox_session::SandboxSessionManager>,
    pub(crate) task_registry: TaskRegistry,
    /// The per-OS-user host-session sockets and the tool sessions they answer.
    pub(crate) host_session_sockets: Arc<HostSessionSockets>,
    pub(crate) agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    /// The agent topic's handle: a jail start claims its seeded clones and resolves its agent defs
    /// through it.
    pub(crate) agent_roster: AgentRoster,
    /// The split topic's handle: a start or resume of a split or jailed-codebase session, and a
    /// delete of its paired checkout, go through it.
    pub(crate) split_sessions: SplitSessions,
    /// What the presenter observer of a freshly spawned workflow session reads.
    pub(crate) presenter_observer_deps: PresenterObserverDeps,
    pub(crate) user_resolver: SessionUserResolver,
    pub(crate) spawn_client: Option<Arc<SpawnClient>>,
    pub(crate) workspace_sandboxes:
        Arc<tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry>,
    /// The relay's idle tracker, bumped on every RPC this topic serves.
    pub(crate) rpc_activity: RpcActivity,
    /// What each agent session's own conversation says its agent is doing, which `ListSessions`
    /// reports.
    pub(crate) session_agent_inference: Arc<SessionAgentInferenceStore>,
    pub(crate) session_rooms: Arc<SessionRoomRegistry>,
    /// The checkouts this daemon holds on other daemons' behalf; a delete forgets the one it holds
    /// for the session.
    pub(crate) hosted_agent_clones: Arc<HostedAgentClones>,
    pub(crate) session_admissions: Arc<SessionAdmissionRegistry>,
    /// Told of each started session's worktree. `None` on a host whose embedder acts on nothing of
    /// the kind.
    pub(crate) worktree_observer: Option<Arc<dyn SessionWorktreeObserver>>,
    /// The host's capabilities that are not fields.
    pub(crate) host: Arc<dyn LaunchHost>,
}

impl LaunchSessions {
    /// The fields attachment materialization reads, lent to it for the length of one call.
    pub(crate) fn attachment_state(&self) -> AttachmentState<'_> {
        AttachmentState {
            config: &self.config,
            tddy_data_dir: &self.tddy_data_dir,
            staging_base_dir: &self.staging_base_dir,
            peer_routing: &self.peer_routing,
        }
    }

    /// Start the presenter observer for a freshly spawned workflow session (see
    /// [`PresenterObserverDeps::maybe_spawn_presenter_observer`]), over this handle's sinks.
    pub(crate) fn maybe_spawn_presenter_observer(
        &self,
        os_user: &str,
        session_id: &str,
        grpc_port: u16,
    ) {
        self.presenter_observer_deps
            .maybe_spawn_presenter_observer(os_user, session_id, grpc_port);
    }

    /// Pre-creates `session_dir` when needed and materializes the request's attachments before
    /// spawn (see [`AttachmentState::prepare_session_attachments`]), over this handle's state.
    pub(crate) async fn prepare_session_attachments(
        &self,
        ctx: &AttachmentMaterialization<'_>,
    ) -> Result<Vec<SessionAttachment>, Status> {
        self.attachment_state()
            .prepare_session_attachments(ctx)
            .await
    }
}
