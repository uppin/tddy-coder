//! Family L — `exec_tools.ExecToolService`.

mod path_guard;
mod ports;
mod project_home;
mod project_home_remote;
mod project_home_session_follow;
mod project_home_start_options;
mod result_frames;
mod role_guard;

pub use result_frames::EXEC_TOOL_FRAME_BYTES;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tddy_coding_subagent::ApprovalRoutes;
use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::{GithubCli, HostServiceImpl};
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::{DaemonSessionHost, LocalExecTools};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::relay_idle::RpcActivity;
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;
use tddy_worktree_service::worktrees::WorktreeSizeCalculator;

use crate::families::ProviderAccountsPort;
use crate::{CatalogRpcHandler, ProjectRpcHandler};

/// The exec-tool RPCs: the caller's identity, the peer a split session's tools are forwarded
/// to, and the three places a tool actually runs — this host's task registry, a workspace sandbox,
/// or a hosted agent clone.
pub struct ExecToolRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    peer_routing: PeerRouting,
    rpc_activity: RpcActivity,
    local_exec_tools: LocalExecTools,
    /// The host's full session start and list paths, late-bound when the RPC families are attached
    /// to the host (`#project-home` 5/7). Unset on a handler built from a host that was never given
    /// its families, and a project-home tool that needs it then refuses.
    session_launch: Arc<OnceLock<LaunchSessions>>,
    /// The routes a headless conversation's permission approval is answered through. `None` on a
    /// host with none wired, which refuses every approval.
    approval_routes: Option<Arc<ApprovalRoutes>>,
    /// What the project-home tools read from the host beyond the session store.
    project_home_host: ProjectHomeHost,
}

/// The host resources only the project-home tools use, held together so the handler stays within
/// its field budget.
pub(crate) struct ProjectHomeHost {
    /// The vaults a project-home git tool reads a target's GitHub identity from (`#project-home` 7/7).
    pub(crate) credential_vaults: Option<Arc<SessionVaults>>,
    pub(crate) github_cli: GithubCli,
    /// The size calculator the Worktrees screen streams from, shared with the host, so a project-home
    /// `ListWorktrees` call reads the same calculation state (`#start-parity` 3/9).
    pub(crate) worktree_sizes: Arc<WorktreeSizeCalculator>,
    /// The catalogue, project and host RPCs `DescribeSessionStartOptions` answers its lists from,
    /// called in-process on this host (`#start-parity` 5/9).
    pub(crate) catalog: Arc<CatalogRpcHandler>,
    pub(crate) projects: Arc<ProjectRpcHandler>,
    pub(crate) host_service: Arc<HostServiceImpl>,
    /// The provider accounts this host's vaults hold, late-bound by
    /// [`RpcHandlers::attach_provider_accounts`](crate::RpcHandlers::attach_provider_accounts).
    pub(crate) provider_accounts: Arc<OnceLock<Arc<dyn ProviderAccountsPort>>>,
    /// The terminals `ReadSessionTerminal` reads a session's main terminal through: the store the
    /// terminal RPCs resolve through, sandbox first (`#home-inbox` 2/8).
    pub(crate) terminals: DaemonTerminalSessionStore,
}

impl ExecToolRpcHandler {
    /// A handler sharing `host`'s state — the same task registry, sandboxes and hosted clones, so
    /// a tool it runs is one the rest of the daemon can see.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            peer_routing: host.peer_routing(),
            rpc_activity: host.rpc_activity(),
            local_exec_tools: host.local_exec_tools(),
            session_launch: Arc::new(OnceLock::new()),
            approval_routes: host.approval_routes(),
            project_home_host: ProjectHomeHost {
                credential_vaults: host.credential_vaults(),
                github_cli: host.github_cli(),
                worktree_sizes: host.worktree_size_calculator(),
                catalog: Arc::new(CatalogRpcHandler::from_host(host)),
                projects: Arc::new(ProjectRpcHandler::from_host(host)),
                // Only its `ListSshConfigHosts` is answered from this instance, which reads the host's
                // SSH config and no peer's, so the registry and prompt state it is built without are
                // never reached from here.
                host_service: Arc::new(HostServiceImpl::new(
                    host.config().clone(),
                    host.tddy_data_dir(),
                    host.user_resolver(),
                )),
                provider_accounts: Arc::new(OnceLock::new()),
                terminals: host.terminal_sessions(),
            },
        }
    }

    /// Answer approvals through `routes` (builder). Without it every approval is refused.
    #[must_use]
    pub fn with_approval_routes(mut self, routes: Arc<ApprovalRoutes>) -> Self {
        self.approval_routes = Some(routes);
        self
    }

    /// The late-bound launch cell, shared with the families that attach it.
    pub(crate) fn session_launch_cell(&self) -> Arc<OnceLock<LaunchSessions>> {
        Arc::clone(&self.session_launch)
    }

    /// The late-bound provider-account cell, shared with the families that attach it.
    pub(crate) fn provider_accounts_cell(&self) -> Arc<OnceLock<Arc<dyn ProviderAccountsPort>>> {
        Arc::clone(&self.project_home_host.provider_accounts)
    }
}
