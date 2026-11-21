//! Family L — `exec_tools.ExecToolService`.

mod base_freshness;
pub mod home_terminal_leases;
mod lsp_roots;
mod orchestrator_reach;
mod path_access;
mod path_guard;
mod ports;
mod project_home;
mod project_home_inbox;
mod project_home_remote;
mod project_home_session_follow;
mod project_home_start_options;
mod project_home_watch_stream;
mod project_home_watches;
mod result_frames;
mod role_guard;
mod session_files;
mod session_messages;
mod stack_git_tools;
pub mod stack_tools;
mod watch_events;
pub mod watch_wake;

pub use result_frames::EXEC_TOOL_FRAME_BYTES;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tddy_coding_subagent::ApprovalRoutes;
use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::{GithubCli, HostServiceImpl};
use tddy_rpc::Status;
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::{DaemonSessionHost, LocalExecTools};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::relay_idle::RpcActivity;
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;
use tddy_terminal_rpc::TerminalControl;
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
    /// The host service the project-home tools answer host questions from, late-bound by
    /// [`ExecToolRpcHandler::attach_host_service`]; unset until attached, and a project-home
    /// tool that needs it then refuses (see [`ExecToolRpcHandler::host_service`]).
    pub(crate) host_service: Arc<OnceLock<Arc<HostServiceImpl>>>,
    /// The provider accounts this host's vaults hold, late-bound by
    /// [`RpcHandlers::attach_provider_accounts`](crate::RpcHandlers::attach_provider_accounts).
    pub(crate) provider_accounts: Arc<OnceLock<Arc<dyn ProviderAccountsPort>>>,
    /// The terminals `ReadSessionTerminal` reads a session's main terminal through: the store the
    /// terminal RPCs resolve through, sandbox first (`#home-inbox` 2/8).
    pub(crate) terminals: DaemonTerminalSessionStore,
    /// The control lease of every terminal this host serves, the one `ClaimSessionTerminal` takes and
    /// `SendSessionKeys` verifies (`#home-inbox` 5/8).
    pub(crate) terminal_control: Arc<dyn TerminalControl>,
    /// The control tokens this host's project-home sessions hold on its terminals, kept here so the
    /// agent never sees one (`#home-inbox` 5/8).
    pub(crate) leases: Arc<home_terminal_leases::HomeTerminalLeases>,
    /// The permission prompts this host's claude-cli hooks hold open, the registry
    /// `AnswerSessionPermission` delivers into (`#home-inbox` 6/8).
    pub(crate) pending_answers: Arc<tddy_session_activity::pending_answers::PendingAnswers>,
    /// The access requests `RequestPathAccess` records and the activity service lists and answers
    /// (`#orchestra` 6/9).
    pub(crate) path_access: tddy_session_activity::path_access::PathAccessRequests,
    /// The project notification log the daemon registered on its bus, which
    /// `WaitProjectNotifications` and `ReadProjectNotifications` read (`#home-inbox` 7/8). `None`
    /// on a host with none installed.
    pub(crate) notification_log: Option<Arc<ProjectNotificationLog>>,
    /// The bus a session's `PostSessionMessage` is published on (`#activity-wake` 5/5). `None` on a
    /// host with none.
    pub(crate) notification_bus:
        Option<Arc<tddy_session_activity::session_notifications::SessionNotificationBus>>,
    /// The session watches the watch tools keep and every orchestrator's answer carries the events
    /// of (`#activity-wake` 2/3). `None` on a host with none.
    pub(crate) session_watches:
        Option<Arc<tddy_session_activity::session_watches::SessionWatchStore>>,
    /// This host's activity surface, which `NotifyOperator` publishes through in process
    /// (`#home-inbox` 11/11).
    pub(crate) activity: Arc<tddy_session_lifecycle::connection_service::PeerRoutedActivity>,
    /// How the stack tools reach GitHub (`#orchestrators` 7/11), late-bound by
    /// [`RpcHandlers::attach_stack_github`](crate::RpcHandlers::attach_stack_github); the GitHub REST
    /// API itself when nothing is attached.
    pub(crate) stack_github: Arc<OnceLock<Arc<dyn stack_tools::StackGithubClients>>>,
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
                host_service: Arc::new(OnceLock::new()),
                provider_accounts: Arc::new(OnceLock::new()),
                terminals: host.terminal_sessions(),
                terminal_control: host.terminal_control(),
                leases: Arc::new(home_terminal_leases::HomeTerminalLeases::following(
                    host.agent_ends(),
                )),
                pending_answers: host.pending_answers(),
                path_access: host.path_access_requests(),
                notification_log: host.project_notification_log(),
                notification_bus: host.session_notification_bus(),
                session_watches: host.session_watches(),
                activity: Arc::new(host.activity_service()),
                stack_github: Arc::new(OnceLock::new()),
            },
        }
    }

    /// Answer approvals through `routes` (builder). Without it every approval is refused.
    #[must_use]
    pub fn with_approval_routes(mut self, routes: Arc<ApprovalRoutes>) -> Self {
        self.approval_routes = Some(routes);
        self
    }

    /// Attach the host service the project-home tools answer host questions from — the
    /// composition root's wired service, with its operator files and Coder vaults, so a start
    /// names and resolves the derived hosts the daemon itself lists for the caller. Called
    /// once, before the handler answers; a second attach keeps the first.
    pub fn attach_host_service(&self, host_service: Arc<HostServiceImpl>) {
        if self
            .project_home_host
            .host_service
            .set(host_service)
            .is_err()
        {
            log::error!("the host service was attached twice; keeping the first attachment");
        }
    }

    /// The host service the project-home tools answer host questions from: the one the
    /// composition root attached. When none was attached the tool is refused, naming the
    /// missing wiring — there is no bare service to fall back to, because one built here
    /// would read this host's SSH config with no Coder accounts and answer a different
    /// listing than the daemon itself serves.
    pub(crate) fn host_service(&self) -> Result<Arc<HostServiceImpl>, Status> {
        self.project_home_host
            .host_service
            .get()
            .cloned()
            .ok_or_else(|| {
                Status::failed_precondition(
                    "this daemon's host service was never attached to its exec-tool handler, \
                 so the host's derived hosts cannot be listed or resolved",
                )
            })
    }

    /// The late-bound launch cell, shared with the families that attach it.
    pub(crate) fn session_launch_cell(&self) -> Arc<OnceLock<LaunchSessions>> {
        Arc::clone(&self.session_launch)
    }

    /// The late-bound stack-tools GitHub cell, shared with the families that attach it.
    pub(crate) fn stack_github_cell(
        &self,
    ) -> Arc<OnceLock<Arc<dyn stack_tools::StackGithubClients>>> {
        Arc::clone(&self.project_home_host.stack_github)
    }

    /// The late-bound provider-account cell, shared with the families that attach it.
    pub(crate) fn provider_accounts_cell(&self) -> Arc<OnceLock<Arc<dyn ProviderAccountsPort>>> {
        Arc::clone(&self.project_home_host.provider_accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unattached_handler_refuses_rather_than_answering_from_a_bare_host_service() {
        // Given a handler no composition root attached a host service to
        let sessions = tempfile::tempdir().expect("sessions storage");
        let host = tddy_session_lifecycle::test_util::test_host(sessions.path().to_path_buf());
        let handler = ExecToolRpcHandler::from_host(&host);

        // When a project-home tool asks it for the host service
        let refused = handler.host_service();

        // Then it refuses, naming the missing wiring, instead of building a bare service
        // whose listing (SSH config only, no Coder accounts) would silently differ
        match refused {
            Ok(_) => panic!("an unattached handler must refuse, not fall back"),
            Err(status) => assert!(
                status.message().contains("never attached"),
                "the refusal names the missing wiring: {}",
                status.message()
            ),
        }

        // And once the wired service is attached, that service is the one answered from
        handler.attach_host_service(Arc::new(HostServiceImpl::new(
            host.config().clone(),
            host.tddy_data_dir(),
            host.user_resolver(),
        )));
        assert!(handler.host_service().is_ok());
    }
}
