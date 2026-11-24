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

use tddy_daemon_extensions::{ReachableSessions, RoomAdmission, SessionReach, SessionRooms};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::relay_idle::RpcActivity;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::Status;
use tddy_service::proto::session::SessionAttachment;
use tddy_session_agents::session_agent_clone::HostedAgentClones;
use tddy_session_agents::session_agent_inference::SessionAgentInferenceStore;
use tddy_spawn::spawn_worker::SpawnClient;
use tddy_spawn::supervisor_client::SpawnBackendChoice;
use tddy_task::TaskRegistry;

use crate::host_session_socket::HostSessionSockets;
use crate::session_worktree_observer::SessionWorktreeObserver;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_session_activity::presenter_observer_spawn::PresenterObserverDeps;
use tddy_session_agents::agent_host_callbacks::AgentRoster;
use tddy_session_files::attachment_progress::AttachmentMaterialization;
use tddy_session_files::svc_materialize_staged_attachment::AttachmentState;
use tddy_session_split::split_ports::SplitSessions;
use tddy_work_queue::rpc::WorkQueueHandler;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_acting_identity::SessionIdentity;

/// The capabilities of the session host the launch topic calls and does not own.
///
/// Implemented once, on the host, in wiring (`svc_agent_host_ports`).
pub trait LaunchHost: Send + Sync {
    /// The host-side dispatch a jail built for `session_id` relays family B to, bound to that
    /// session (the host's `sandbox_rpc_handler`).
    fn sandbox_rpc_handler(
        &self,
        session_id: &str,
        session_dir: &Path,
    ) -> Arc<dyn tddy_sandbox_runner::HostRpcHandler>;

    /// This daemon's PR-stack handler, which a stack base or a stack link owned by a peer is asked
    /// of, or `FAILED_PRECONDITION` when the host was never given the RPC families.
    fn work_queue(&self) -> Result<Arc<dyn WorkQueueHandler>, Status>;

    /// What a session's vault reads go through: this daemon's vaults, how a session token names
    /// its owner, and the token the session was started with.
    fn session_account_access(&self, session_token: &str) -> SessionAccountAccess;

    /// What reads the vault of `subject` — the GitHub user a session token resolved to — for a
    /// start the daemon makes on that person's behalf with no token in hand (a jailed shell tab's
    /// provider route).
    fn subject_account_access(&self, subject: &str) -> SessionAccountAccess;

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

    /// Where a session's PR is read: the host's GitHub inbox. A listing peeks its cache through
    /// this and never waits on GitHub (`#workqueue` 4/14).
    fn pull_requests(
        &self,
    ) -> Arc<dyn tddy_worktree_service::pull_request_view::SessionPullRequests>;

    /// An orchestrator-role session of `project_id` is running — just started, or just resumed
    /// — so the host watches the project's remote trunk for it until it stops (`#orchestra` 5/9).
    /// `session_token` is the caller's, from which the project's GitHub account is read.
    fn orchestrator_running(
        &self,
        os_user: &str,
        project_id: &str,
        session_id: &str,
        session_token: &str,
    );
}

/// The session host's launch fields, owned, plus its callbacks.
///
/// Built per call by the host (`DaemonSessionHost::launch_sessions`). Every shared field is the
/// `Arc` the host holds, so a clone of this talks to the managers, registries and jails the host
/// does.
#[derive(Clone)]
pub struct LaunchSessions {
    pub config: DaemonConfig,
    pub tddy_data_dir: PathBuf,
    pub staging_base_dir: PathBuf,
    pub peer_routing: PeerRouting,
    pub cli_manager: Arc<CliSessionManager>,
    /// Sandboxed claude-cli sessions (darwin Seatbelt).
    pub sandbox_manager: Arc<tddy_daemon_sandbox::sandbox_session::SandboxSessionManager>,
    pub task_registry: TaskRegistry,
    /// The per-OS-user host-session sockets and the tool sessions they answer.
    pub host_session_sockets: Arc<HostSessionSockets>,
    /// The agent-tool socket of each session's OS user (`#agent-tools` 26/28): what a session's
    /// hooks, `tddy-tools notify` and tool calls are pointed at.
    pub agent_tool_sockets: Arc<tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets>,
    pub agent_activity_hub: Arc<tddy_daemon_kernel::AgentActivityHub>,
    /// The agent topic's handle: a jail start claims its seeded clones and resolves its agent defs
    /// through it.
    pub agent_roster: AgentRoster,
    /// The split topic's handle: a start or resume of a split or jailed-codebase session, and a
    /// delete of its paired checkout, go through it.
    pub split_sessions: SplitSessions,
    /// What the presenter observer of a freshly spawned workflow session reads.
    pub presenter_observer_deps: PresenterObserverDeps,
    pub user_resolver: SessionUserResolver,
    pub spawn_client: Option<Arc<SpawnClient>>,
    /// Which privileged backend spawns and clones on this host, chosen once at startup. Read beside
    /// `spawn_client`, never re-derived from config.
    pub spawn_backend: SpawnBackendChoice,
    /// Where a clone's `SSH_ASKPASS` helper asks for a key passphrase. `None` on a host without a
    /// daemon runtime, whose SSH clones are then given no helper.
    pub askpass: Option<Arc<tddy_host_service::askpass::AskpassEndpoint>>,
    pub workspace_sandboxes:
        Arc<tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceSandboxRegistry>,
    /// The relay's idle tracker, bumped on every RPC this topic serves.
    pub rpc_activity: RpcActivity,
    /// What each agent session's own conversation says its agent is doing, which `ListSessions`
    /// reports.
    pub session_agent_inference: Arc<SessionAgentInferenceStore>,
    pub session_rooms: Arc<dyn SessionRooms>,
    /// The checkouts this daemon holds on other daemons' behalf; a delete forgets the one it holds
    /// for the session.
    pub hosted_agent_clones: Arc<HostedAgentClones>,
    pub session_admissions: Arc<dyn RoomAdmission>,
    /// Told of each started session's worktree. `None` on a host whose embedder acts on nothing of
    /// the kind.
    pub worktree_observer: Option<Arc<dyn SessionWorktreeObserver>>,
    /// The coding agents declared as tools (`#agent-tools 2/12`). Every launch has them
    /// (`#agent-tools 19/19`): a host that installed none is given an empty registry, whose every
    /// agent session is refused for naming no spec — nothing is launched by hand.
    pub coding_agents: Arc<crate::spec_launch::HostCodingAgents>,
    /// The orchestrators' session watches and the log they read, which the idle reaper asks
    /// whether a project-home session holds a live watch (`#activity-wake` 3/3). `None` on a host
    /// with none: nothing is held up.
    pub session_watches: Option<(
        Arc<tddy_session_activity::session_watches::SessionWatchStore>,
        Arc<tddy_session_activity::project_notification_log::ProjectNotificationLog>,
    )>,
    /// The host's capabilities that are not fields.
    pub host: Arc<dyn LaunchHost>,
    /// What extends a session's reach beyond this host when it is connected to.
    /// [`NoSessionReach`](tddy_daemon_extensions::NoSessionReach) when nothing is wired.
    pub session_reach: Arc<dyn SessionReach>,
    /// The host as an extension reaches back into it when it is told of a reach.
    pub reachable_sessions: Arc<dyn ReachableSessions>,
    /// The hook-log followers of this daemon's target sessions, by session id.
    pub remote_hook_followers: crate::remote_hook_follower::RemoteHookFollowers,
    /// Where a target session's followed hooks are delivered: this daemon's activity handling.
    /// `None` on a host that serves none, whose target sessions then start no follower.
    pub hook_report_delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
    /// Reads a Coder account's credentials from its owner's vault, for a session whose target
    /// is a Coder workspace (`coder_target::shell_for_coder_target`).
    pub coder_credentials: Arc<dyn tddy_host_service::coder_workspaces::CoderCredentialSource>,
    /// The jails of the host's jailed shell tabs (`#session-liveness` 5/8): a restored
    /// `sandbox` → `shell` tab is registered here, and a delete stops its session's.
    pub tab_jails: Arc<tddy_daemon_sandbox::tab_jails::TabJails>,
    /// What starts a tab's jail in place of this topic's own jail launch
    /// ([`crate::tab_jail_launch`]); `None` on every host but one a test gave a double.
    pub tab_jail_spawn: Option<Arc<dyn tddy_daemon_sandbox::tab_jails::TabJailSpawn>>,
    /// What resolves a target session's target for its tabs in place of this topic's own
    /// ([`crate::tab_target_shell`], `#session-liveness` 6/8); `None` on every host but one a test
    /// gave a double.
    pub tab_target_shell: Option<Arc<dyn crate::tab_target_shell::TabTargetShell>>,
    /// What ends a session's tabs beside its main, and the conversations open on it, when the
    /// session is deleted (docs/ft/daemon/session-liveness.md § Closing a session).
    pub session_tabs: Arc<dyn SessionTabEnding>,
}

/// Ends every tab of a session other than its main — each tab of its roster and each live
/// terminal, the tmux sessions their chains record here or on the session's target, jailed tabs —
/// and every subagent conversation open on it, here or on the daemon that runs it.
///
/// The terminal roster and the conversation table live in wiring above this topic, so a delete
/// reaches them through this port (implemented once, on the host). A close ends the same tabs
/// through the same code, one step each.
#[async_trait::async_trait]
pub trait SessionTabEnding: Send + Sync {
    /// End them for session `session_id`, a target reached — and a conversation on another daemon
    /// cancelled — with `session_token`'s account. Every tab and conversation is attempted; the
    /// error names each one that could not be ended. A session with no directory on this daemon
    /// has nothing to end.
    async fn end_tabs_and_conversations(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Result<(), Status>;
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
