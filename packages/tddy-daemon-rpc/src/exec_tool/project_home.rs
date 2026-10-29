//! The project-home session's exec tools, answered on this daemon (`#project-home` 5/7).
//!
//! The four tools in [`tddy_tool_engine::project_home_tools`] are served here rather than by the
//! local tool runner: this is where the caller's role is read, where the host's own session list and
//! session start are reached through [`ProjectHomePort`], and where the call is recorded. Every other
//! exec tool still goes to [`LocalExecTools::run_exec_tool_locally`](tddy_session_lifecycle::connection_service::LocalExecTools::run_exec_tool_locally)
//! unchanged.
//!
//! The host's list and start are late-bound: the launch snapshot they run on is attached when the
//! RPC families are installed on the host (see `DaemonRpcFamilies::attach_session_start`), so a
//! project-home start is the same start any other caller gets, for every session type.
//!
//! A call naming a target on another host, a listing of every host, and a caller held on another
//! host are `project_home_remote`'s (`#start-parity` 1/9).
//!
//! A start is begun as a `StreamStartSession` and answered with its handle at once; its record is
//! the start ledger in the orchestrator's own session directory, read by `GetSessionStart` and
//! stopped by `CancelSessionStart` (`#start-parity` 4/9). The ledger is recovered the first time this
//! daemon process loads a home session's ledger, before any start is begun in it:
//! [`DaemonProjectHome::start_ledger_of`].
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

mod answer;
mod caller;
mod listing;
mod notify;
mod port;
mod terminal;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_host_service::HostServiceImpl;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::pending_answers::PendingAnswers;
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_session_lifecycle::connection_service::start_choices;
use tddy_session_lifecycle::connection_service::PeerRoutedActivity;
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_terminal_rpc::TerminalControl;
use tddy_tool_engine::catalog::ToolDef;
use tddy_tool_engine::project_home_answer_tools::{
    execute_project_home_answer_tool, is_project_home_answer_tool,
};
use tddy_tool_engine::project_home_git_tools::is_project_home_git_tool;
use tddy_tool_engine::project_home_inbox_tools::{
    execute_project_home_inbox_tool, is_project_home_inbox_tool,
};
use tddy_tool_engine::project_home_notify_tools::{
    execute_project_home_notify_tool, is_project_home_notify_tool,
};
use tddy_tool_engine::project_home_terminal_tools::{
    execute_project_home_terminal_tool, is_project_home_terminal_tool,
};
use tddy_tool_engine::project_home_tools::{
    execute_project_home_tool, is_project_home_tool, session_offered_catalog_for,
};
use tddy_tool_engine::start_ledger::StartLedger;
use tddy_worktree_service::session_target::HomeCaller;
use tddy_worktree_service::worktrees::WorktreeSizeCalculator;

use super::home_terminal_leases::{HomeTerminalClaimKey, HomeTerminalLeases};
use super::project_home_remote::PeerSessionLists;
use super::ExecToolRpcHandler;
use crate::exec_tool::project_home::caller::project_home_caller;
use crate::exec_tool::project_home::caller::record_call;
use crate::exec_tool::project_home::caller::refuse_a_spawner_outside_its_own_sessions;
use crate::exec_tool::project_home::listing::session_rows;
use crate::exec_tool::project_home::listing::SessionRow;
use crate::families::ProviderAccountsPort;
use crate::{CatalogRpcHandler, ProjectRpcHandler};

impl ExecToolRpcHandler {
    /// Run `req`'s tool on this host. The one place the exec-tool handlers reach the local runner,
    /// so the project-home tools are answered here, ahead of it, and every other tool is not.
    pub(super) async fn run_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        if !is_project_home_tool(&req.tool_name) && !is_project_home_git_tool(&req.tool_name) {
            return Ok(self
                .local_exec_tools
                .run_exec_tool_locally(req, sessions_base, worktree_root)
                .await);
        }
        let (home, role) = project_home_caller(sessions_base, &req.session_id, &req.tool_name)?;
        refuse_a_spawner_outside_its_own_sessions(sessions_base, &home, role, req)?;
        let response = match self.served_on_target_host(req).await? {
            Some(answered) => answered,
            None => {
                let peers = PeerSessionLists {
                    routing: self.peer_routing.clone(),
                    local_id: local_instance_id_for_config(&self.config),
                    request: req.clone(),
                };
                let os_user = self.os_user_of(&req.session_token)?;
                let local_id = local_instance_id_for_config(&self.config);
                self.project_home_port(req, os_user, sessions_base, Some(peers), local_id)?
                    .execute(&home, req)
                    .await
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// [`ProjectHomePort`] over this host for `req`'s caller, whose sessions are `os_user`'s in
    /// `sessions_base`. `peers` lists the other hosts' sessions; `None` lists this host's alone.
    /// `caller_daemon_instance_id` is the daemon that owns the calling session: this host for a
    /// caller held here, the peer that confirmed it otherwise.
    pub(super) fn project_home_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
        peers: Option<PeerSessionLists>,
        caller_daemon_instance_id: String,
    ) -> Result<DaemonProjectHome, Status> {
        Ok(DaemonProjectHome {
            launch: self.attached_launch(&req.tool_name)?,
            session_token: req.session_token.clone(),
            os_user,
            sessions_base: sessions_base.to_path_buf(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            identity: SessionAccountAccess::new(
                self.project_home_host.credential_vaults.clone(),
                self.user_resolver.clone(),
                &req.session_token,
            )
            .with_github_cli(self.project_home_host.github_cli.clone()),
            daemon_instance_id: local_instance_id_for_config(&self.config),
            peers,
            worktree_sizes: Arc::clone(&self.project_home_host.worktree_sizes),
            catalog: Arc::clone(&self.project_home_host.catalog),
            projects: Arc::clone(&self.project_home_host.projects),
            host_service: Arc::clone(&self.project_home_host.host_service),
            provider_accounts: self
                .project_home_host
                .provider_accounts
                .get()
                .map(Arc::clone),
            terminals: self.project_home_host.terminals.clone(),
            home_session_id: req.session_id.clone(),
            caller_daemon_instance_id,
            terminal_control: Arc::clone(&self.project_home_host.terminal_control),
            leases: Arc::clone(&self.project_home_host.leases),
            pending_answers: Arc::clone(&self.project_home_host.pending_answers),
            notification_log: self.project_home_host.notification_log.clone(),
            activity: Arc::clone(&self.project_home_host.activity),
        })
    }

    /// The host's launch snapshot, or a refusal naming the missing wiring when the families were
    /// never attached to a host.
    fn attached_launch(&self, tool_name: &str) -> Result<LaunchSessions, Status> {
        self.session_launch.get().cloned().ok_or_else(|| {
            Status::failed_precondition(format!(
                "tool {tool_name} is refused: this daemon's RPC families were never attached to its \
                 session host, so the host's session list and start are unreachable"
            ))
        })
    }

    /// The exec tools a `ListExecTools` caller is answered, once `session_token` is verified to map
    /// to an OS user: the set `session_id`'s agent is offered, or the daemon-wide catalog when the
    /// caller names no session.
    pub(super) fn offered_catalog(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<Vec<ToolDef>, Status> {
        let os_user = self.os_user_of(session_token)?;
        if session_id.trim().is_empty() {
            return Ok(tddy_tool_engine::tool_catalog());
        }
        self.session_tool_catalog(&os_user, session_id)
    }

    /// The exec tools `session_id`'s agent is offered. The role is read from the session's own record:
    /// a project-home session is offered the project-home catalog, its `StartSession` offering this
    /// daemon's eligible hosts and recipes, and any other session the daemon-wide catalog without the
    /// project-home tools.
    fn session_tool_catalog(
        &self,
        os_user: &str,
        session_id: &str,
    ) -> Result<Vec<ToolDef>, Status> {
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
            Status::not_found(format!(
                "session {session_id}: cannot read its record to tell which tools it is offered ({e})"
            ))
        })?;
        Ok(session_offered_catalog_for(
            session_tool_role(&metadata),
            &start_choices(&self.peer_routing),
        ))
    }

    /// The OS user `session_token` maps to, as the start path resolves it.
    pub(super) fn os_user_of(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }
}

/// [`ProjectHomePort`] over this host, for one caller: the host's session list and start run as the
/// caller's token, and the session store is read for the worktrees the resolver needs.
pub(super) struct DaemonProjectHome {
    launch: LaunchSessions,
    session_token: String,
    /// The OS user the caller's sessions belong to; the notification log is read for it
    /// (`#home-inbox` 7/8).
    pub(super) os_user: String,
    sessions_base: PathBuf,
    tddy_data_dir: PathBuf,
    /// Reads the signed-in person's vault for a target's GitHub identity, as a session start does.
    identity: SessionAccountAccess,
    /// This host, as the eligible-daemon roster names it; every session it lists carries it.
    daemon_instance_id: String,
    /// The other hosts' listings, for a caller held here; `None` for one held elsewhere, whose own
    /// host fans out.
    peers: Option<PeerSessionLists>,
    /// The daemon's worktree size calculator, the one the Worktrees screen reads.
    worktree_sizes: Arc<WorktreeSizeCalculator>,
    /// The host's catalogue, project and host RPCs, which `DescribeSessionStartOptions` answers its
    /// lists from in-process (`#start-parity` 5/9).
    catalog: Arc<CatalogRpcHandler>,
    projects: Arc<ProjectRpcHandler>,
    host_service: Arc<HostServiceImpl>,
    /// The provider accounts this host's vaults hold; `None` on a host with no credential vaults.
    provider_accounts: Option<Arc<dyn ProviderAccountsPort>>,
    /// The terminals `ReadSessionTerminal` reads a session's main terminal through, and
    /// `SendSessionKeys` writes to.
    terminals: DaemonTerminalSessionStore,
    /// The calling home session's id, as the caller's own session record names it. Half of the key
    /// its terminal claims are remembered under (`#home-inbox` 5/8).
    home_session_id: String,
    /// The daemon instance that owns the calling home session: this host for a caller held here, the
    /// peer that confirmed it otherwise. The other half of that key.
    caller_daemon_instance_id: String,
    /// The control lease of every terminal this host serves, the one the browsers claim and steal.
    terminal_control: Arc<dyn TerminalControl>,
    /// The control tokens this host's project-home sessions hold on its terminals.
    leases: Arc<HomeTerminalLeases>,
    /// The permission prompts this host's hooks hold open, which `AnswerSessionPermission` answers.
    pending_answers: Arc<PendingAnswers>,
    /// This host's project notification log, which the two inbox tools read (`#home-inbox` 7/8);
    /// `None` on a host with none installed.
    pub(super) notification_log: Option<Arc<ProjectNotificationLog>>,
    /// This host's activity surface, which `NotifyOperator` reports through (`#home-inbox` 11/11).
    activity: Arc<PeerRoutedActivity>,
}

impl DaemonProjectHome {
    /// Run `req`'s project-home tool for `home`.
    pub(super) async fn execute(
        &self,
        home: &HomeCaller,
        req: &ExecuteToolRequest,
    ) -> ExecuteToolResponse {
        let outcome = if is_project_home_notify_tool(&req.tool_name) {
            execute_project_home_notify_tool(self, home, &req.tool_name, &req.args_json).await
        } else if is_project_home_answer_tool(&req.tool_name) {
            execute_project_home_answer_tool(self, home, &req.tool_name, &req.args_json).await
        } else if is_project_home_inbox_tool(&req.tool_name) {
            execute_project_home_inbox_tool(self, home, &req.tool_name, &req.args_json).await
        } else if is_project_home_terminal_tool(&req.tool_name) {
            execute_project_home_terminal_tool(self, home, &req.tool_name, &req.args_json).await
        } else {
            execute_project_home_tool(self, home, &req.tool_name, &req.args_json).await
        };
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }

    /// The key this caller's claim on `target_session_id`'s terminal is remembered under.
    fn claim_key(&self, target_session_id: &str) -> HomeTerminalClaimKey {
        HomeTerminalClaimKey {
            caller_daemon_instance_id: self.caller_daemon_instance_id.clone(),
            caller_session_id: self.home_session_id.clone(),
            target_session_id: target_session_id.to_string(),
        }
    }

    /// The start ledger of `home_session_id`. The first time this daemon process loads it, the
    /// starts it still records as running are recovered: a start is only ever begun in a ledger after
    /// that, so a record still running is one an earlier daemon was draining when it stopped
    /// (`#start-parity` 4/9).
    fn start_ledger_of(&self, home_session_id: &str) -> anyhow::Result<StartLedger> {
        validate_session_id_segment(home_session_id)
            .map_err(|e| anyhow::anyhow!("{}", e.message()))?;
        StartLedger::recovered_in_session_dir(&unified_session_dir_path(
            &self.sessions_base,
            home_session_id,
        ))
    }

    async fn rows(&self) -> anyhow::Result<Vec<SessionRow>> {
        let sessions_base = self.sessions_base.clone();
        let tddy_data_dir = self.tddy_data_dir.clone();
        let os_user = self.os_user.clone();
        tokio::task::spawn_blocking(move || session_rows(&sessions_base, &tddy_data_dir, &os_user))
            .await
            .map_err(|e| anyhow::anyhow!("reading the sessions failed: {e}"))?
    }
}

pub(super) fn status_error(status: Status) -> anyhow::Error {
    anyhow::anyhow!(status.message().to_string())
}

/// The project `project_id` registered for `os_user`, or why it could not be read.
fn registered_project(
    tddy_data_dir: &Path,
    os_user: &str,
    project_id: &str,
) -> Result<tddy_projects::project_storage::ProjectData, String> {
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir))
        .ok_or_else(|| "could not resolve projects path".to_string())?;
    tddy_projects::project_storage::find_project(&projects_dir, project_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "project not found".to_string())
}
