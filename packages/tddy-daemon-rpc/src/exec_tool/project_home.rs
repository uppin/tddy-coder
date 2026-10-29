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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_agent_launch::conversation_spawn::session_tool_role;
use tddy_agent_launch::svc_signal_delete_session::signal_session_dir;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_host_service::HostServiceImpl;
use tddy_rpc::{Request, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{ListSessionsRequest, Signal};
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_session_lifecycle::connection_service::start_choices;
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_task::TextGrid;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_terminal_rpc::{ControlClaim, TerminalControl};
use tddy_tool_engine::catalog::ToolDef;
use tddy_tool_engine::project_home_git_tools::is_project_home_git_tool;
use tddy_tool_engine::project_home_terminal_tools::{
    execute_project_home_terminal_tool, is_project_home_terminal_tool, ProjectHomeTerminalPort,
};
use tddy_tool_engine::project_home_tools::{
    execute_project_home_tool, is_conversation_spawner_tool, is_project_home_tool,
    session_offered_catalog_for, ListProjectSessionsResult, ListedSession, ProjectHomePort,
    SessionToolRole,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{StartHandle, StartLedger, StartLedgerRecord};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::{StartContext, StartSessionArgs};
use tddy_worktree_service::service::project_main_repo_on_host;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};
use tddy_worktree_service::worktrees::{
    list_worktree_diff_rows, project_worktree_snapshot, worktree_path_is_listed,
    WorktreeSizeCalculator,
};

use super::home_terminal_leases::{HomeTerminalClaimKey, HomeTerminalLeases};
use super::project_home_remote::PeerSessionLists;
use super::project_home_session_follow;
use super::project_home_start_options::{describe_start_options, StartOptionsSources};
use super::ExecToolRpcHandler;
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
        let home = project_home_caller(sessions_base, &req.session_id, &req.tool_name)?;
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

/// Refuse `tool_name` unless `session_id` is a project-home session — or a session that spawns
/// conversations calling one of the four start tools — and name that session's project. The role is
/// the session's own record, not anything the request claims.
fn project_home_caller(
    sessions_base: &Path,
    session_id: &str,
    tool_name: &str,
) -> Result<HomeCaller, Status> {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
        Status::permission_denied(format!(
            "tool {tool_name} is refused: cannot read session {session_id}'s role ({e})"
        ))
    })?;
    match session_tool_role(&metadata) {
        SessionToolRole::ProjectHome => {}
        // A session that spawns conversations is served the start tools, for its own project.
        SessionToolRole::ConversationSpawner if is_conversation_spawner_tool(tool_name) => {}
        _ => {
            return Err(Status::permission_denied(format!(
                "tool {tool_name} is served only to a project-home session"
            )));
        }
    }
    Ok(HomeCaller {
        session_id: session_id.to_string(),
        project_id: metadata.project_id,
    })
}

/// Append the call to the session's tool-call log, as the local runner does for every other tool.
/// A failed write is logged and does not fail the call.
fn record_call(sessions_base: &Path, req: &ExecuteToolRequest, response: &ExecuteToolResponse) {
    let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
    let record = tddy_tool_engine::tool_call_log::ToolCallRecord {
        task_id: response.job_id.clone(),
        tool_name: req.tool_name.clone(),
        args_json: req.args_json.clone(),
        result_json: response.result_json.clone(),
        is_error: response.is_error,
        error_message: response.error_message.clone(),
        job_running: response.job_running,
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };
    if let Err(e) = tddy_tool_engine::tool_call_log::append_tool_call(&session_dir, &record) {
        log::warn!(
            "tool_call_log: failed to persist project-home tool call for session {}: {}",
            req.session_id,
            e
        );
    }
}

/// [`ProjectHomePort`] over this host, for one caller: the host's session list and start run as the
/// caller's token, and the session store is read for the worktrees the resolver needs.
pub(super) struct DaemonProjectHome {
    launch: LaunchSessions,
    session_token: String,
    os_user: String,
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
}

/// One session as the target resolver reads it: its `.session.yaml`, its changeset's branch, and the
/// ref its worktree is diffed against.
struct SessionRow {
    entry: SessionEntry,
    branch: String,
    base_ref: String,
}

impl DaemonProjectHome {
    /// Run `req`'s project-home tool for `home`.
    pub(super) async fn execute(
        &self,
        home: &HomeCaller,
        req: &ExecuteToolRequest,
    ) -> ExecuteToolResponse {
        let outcome = if is_project_home_terminal_tool(&req.tool_name) {
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

fn session_rows(
    sessions_base: &Path,
    tddy_data_dir: &Path,
    os_user: &str,
) -> anyhow::Result<Vec<SessionRow>> {
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir));
    // A project's main branch is the diff base of every session in it, so each is read once.
    let mut main_branch_refs: HashMap<String, Option<String>> = HashMap::new();
    let mut rows = Vec::new();
    for entry in list_sessions_in_dir(sessions_base)? {
        let session_dir = unified_session_dir_path(sessions_base, &entry.session_id);
        let changeset = tddy_core::read_changeset(&session_dir).ok();
        let branch = changeset
            .as_ref()
            .and_then(|c| c.branch.clone())
            .unwrap_or_default();
        let recorded_base = changeset
            .and_then(|c| c.effective_worktree_integration_base_ref)
            .filter(|r| !r.trim().is_empty());
        let base_ref = match recorded_base {
            Some(base) => base,
            None => main_branch_refs
                .entry(entry.project_id.clone())
                .or_insert_with(|| {
                    let dir = projects_dir.as_ref()?;
                    tddy_projects::project_storage::find_project(dir, &entry.project_id)
                        .ok()
                        .flatten()
                        .and_then(|p| p.main_branch_ref)
                })
                .clone()
                .unwrap_or_default(),
        };
        rows.push(SessionRow {
            entry,
            branch,
            base_ref,
        });
    }
    Ok(rows)
}

#[async_trait]
impl ProjectHomePort for DaemonProjectHome {
    /// The host's choices for a start, answered from this host's own RPCs and sessions. A remote
    /// host's answer is the one it gives for itself: the tool forwards a call that names it.
    async fn start_options(
        &self,
        project_id: &str,
        query: StartOptionsQuery,
    ) -> anyhow::Result<StartOptions> {
        describe_start_options(
            StartOptionsSources {
                session_token: &self.session_token,
                daemon_instance_id: &self.daemon_instance_id,
                launch: &self.launch,
                catalog: &self.catalog,
                projects: &self.projects,
                host_service: &self.host_service,
                provider_accounts: self.provider_accounts.as_deref(),
            },
            project_id,
            query,
        )
        .await
    }

    async fn list_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> anyhow::Result<Vec<ListedSession>> {
        let listed = self
            .launch
            .list_sessions_at_session_coordinate(Request::direct(ListSessionsRequest {
                session_token: self.session_token.clone(),
            }))
            .await
            .map_err(|status| anyhow::anyhow!(status.message().to_string()))?
            .into_inner();
        Ok(listed
            .sessions
            .into_iter()
            .filter(|s| s.project_id == project_id && s.session_id != home_session_id)
            .map(|s| ListedSession {
                agent_status: if s.activity_status.is_empty() {
                    s.status
                } else {
                    s.activity_status
                },
                session_id: s.session_id,
                branch: s.branch,
                worktree: s.repo_path,
                is_active: s.is_active,
                pending_elicitation: s.pending_elicitation,
                daemon_instance_id: self.daemon_instance_id.clone(),
            })
            .collect())
    }

    async fn list_peer_sessions(
        &self,
        _project_id: &str,
        _home_session_id: &str,
    ) -> ListProjectSessionsResult {
        match &self.peers {
            Some(peers) => peers.list().await,
            None => ListProjectSessionsResult {
                sessions: Vec::new(),
                unreachable_hosts: Vec::new(),
            },
        }
    }

    /// The start is begun on this daemon's streamed start path, which forwards it to the host the
    /// arguments name, and recorded in the start ledger of `home_session_id`, the orchestrator the
    /// session is linked to. Answers with the handle once the start has named its task.
    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartSessionArgs,
    ) -> anyhow::Result<StartHandle> {
        let request = args.into_request(StartContext {
            session_token: self.session_token.clone(),
            project_id: project_id.to_string(),
            orchestrator_session_id: home_session_id.to_string(),
            orchestrator_daemon_instance_id: self.daemon_instance_id.clone(),
        });
        let ledger = self.start_ledger_of(home_session_id)?;
        self.launch
            .begin_recorded_start(request, &self.os_user, ledger)
            .await
    }

    async fn get_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<StartLedgerRecord> {
        self.start_ledger_of(home_session_id)?.read(start_id)
    }

    /// Stop the start on the host its task runs on: this host's task registry, or the peer's
    /// `CancelTask`. The record reads cancelled once the start's stream has ended.
    async fn cancel_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<()> {
        let ledger = self.start_ledger_of(home_session_id)?;
        self.launch
            .cancel_recorded_start(&ledger, start_id, &self.session_token)
            .await
    }

    async fn push_identity(&self, target: &SessionTarget) -> anyhow::Result<PushIdentity> {
        let identity = self.identity.clone();
        let os_user = self.os_user.clone();
        let sessions_base = self.sessions_base.clone();
        let tddy_data_dir = self.tddy_data_dir.clone();
        let session_id = target.session_id.clone();
        let project_id = target.project_id.clone();
        let env = tokio::task::spawn_blocking(move || {
            let project = match registered_project(&tddy_data_dir, &os_user, &project_id) {
                Ok(project) => Some(project),
                Err(reason) => {
                    log::warn!(
                        "project-home git: target session {session_id} has no git identity: its project could not be read: {reason}"
                    );
                    None
                }
            };
            let session_dir = unified_session_dir_path(&sessions_base, &session_id);
            identity
                .session_identity_in(&session_id, &os_user, &session_dir, project.as_ref())
                .git_environment
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the target's git identity failed: {e}"))?;
        Ok(PushIdentity { env })
    }

    /// The worktrees of `project_id` on this host, from the snapshot the Worktrees screen's stream
    /// opens with, so the tool and the screen cannot disagree (`#start-parity` 3/9).
    async fn list_worktrees(&self, project_id: &str) -> anyhow::Result<Vec<WorktreeEntry>> {
        let main_repo = project_main_repo_on_host(
            &self.tddy_data_dir,
            &self.os_user,
            &self.daemon_instance_id,
            project_id,
        )
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
        let calculator = Arc::clone(&self.worktree_sizes);
        let project_id = project_id.to_string();
        let rows = tokio::task::spawn_blocking(move || {
            project_worktree_snapshot(&main_repo, &calculator, &project_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the worktrees failed: {e}"))?;
        Ok(rows.into_iter().map(WorktreeEntry::from).collect())
    }

    async fn session_activity(&self, target: &SessionTarget) -> anyhow::Result<SessionActivity> {
        project_home_session_follow::session_activity(&self.sessions_base, target).await
    }

    async fn session_transcript(
        &self,
        target: &SessionTarget,
    ) -> anyhow::Result<Vec<TranscriptItem>> {
        project_home_session_follow::session_transcript(&self.sessions_base, target).await
    }

    async fn session_tool_call(
        &self,
        target: &SessionTarget,
        tool_call_id: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>> {
        project_home_session_follow::session_tool_call(&self.sessions_base, target, tool_call_id)
            .await
    }

    /// SIGTERM to `target`'s process, delivered as `SignalSession` delivers it, on the sessions
    /// directory this port reads the target from (`#start-parity` 7/9). The record, worktree and
    /// branch are left as they are.
    async fn terminate_session(&self, target: &SessionTarget) -> anyhow::Result<()> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let session_id = target.session_id.clone();
        tokio::task::spawn_blocking(move || {
            signal_session_dir(&session_dir, &session_id, Signal::Sigterm as i32)
        })
        .await
        .map_err(|e| anyhow::anyhow!("terminating the session failed: {e}"))?
        .map_err(status_error)?;
        Ok(())
    }

    /// Enqueue the Worktrees screen's size walk: one worktree (membership-checked on git's own
    /// worktree list, as `CalculateWorktreeSize` checks it), or every worktree of the project
    /// (`#start-parity` 8/9). Answers once the walks are queued; the calculator reports the rest.
    async fn refresh_worktree_disk_usage(
        &self,
        project_id: &str,
        path: Option<&str>,
    ) -> anyhow::Result<tddy_tool_engine::project_home_disk_refresh::RefreshQueued> {
        let main_repo = project_main_repo_on_host(
            &self.tddy_data_dir,
            &self.os_user,
            &self.daemon_instance_id,
            project_id,
        )
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
        let wanted = path.map(PathBuf::from);
        let targets = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<PathBuf>> {
            match wanted {
                None => Ok(list_worktree_diff_rows(&main_repo)
                    .into_iter()
                    .map(|row| row.path)
                    .collect()),
                Some(path) if worktree_path_is_listed(&main_repo, &path) => Ok(vec![path]),
                Some(path) => anyhow::bail!(
                    "worktree path {} is not in git worktree list",
                    path.display()
                ),
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("reading the worktrees failed: {e}"))??;
        for target in &targets {
            self.worktree_sizes.enqueue(project_id, target).await;
        }
        Ok(tddy_tool_engine::project_home_disk_refresh::RefreshQueued {
            queued: targets.len(),
        })
    }

    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>> {
        Ok(self
            .rows()
            .await?
            .into_iter()
            .map(|row| TargetRecord {
                worktree: Some(row.entry.repo_path)
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
                session_id: row.entry.session_id,
                project_id: row.entry.project_id,
                branch: row.branch,
                base_ref: row.base_ref,
                is_active: row.entry.is_active,
            })
            .collect())
    }
}

#[async_trait]
impl ProjectHomeTerminalPort for DaemonProjectHome {
    /// The screen of `target`'s main terminal, read under its capture's lock: the active buffer's
    /// rows and the cursor at one moment. The store is the one the terminal RPCs resolve through,
    /// sandbox first. A session with no live terminal, or whose terminal has not reported its size,
    /// is refused with the reason (`#home-inbox` 2/8).
    async fn read_terminal(&self, target: &SessionTarget) -> anyhow::Result<TextGrid> {
        let session = self
            .terminals
            .get_terminal(&target.session_id, MAIN_TERMINAL_ID)
            .await
            .ok_or_else(|| anyhow::anyhow!("session {} has no live terminal", target.session_id))?;
        let capture = session.capture();
        let grid = capture
            .lock()
            .map_err(|_| {
                anyhow::anyhow!(
                    "session {}'s terminal capture is poisoned",
                    target.session_id
                )
            })?
            .text_grid();
        grid.ok_or_else(|| {
            anyhow::anyhow!(
                "session {}'s terminal size is not known yet",
                target.session_id
            )
        })
    }

    /// Take `target`'s control lease for `screen_id` the way a browser's claim does, stealing it, and
    /// remember the issued token under this caller and the target. The token is kept here and never
    /// returned (`#home-inbox` 5/8).
    async fn claim_terminal(&self, target: &SessionTarget, screen_id: &str) -> Result<(), String> {
        match self
            .terminal_control
            .claim(&target.session_id, screen_id, true)
            .await
        {
            ControlClaim::Granted { control_token } => {
                self.leases
                    .remember(self.claim_key(&target.session_id), control_token);
                Ok(())
            }
            ControlClaim::Denied { holder_screen_id } => Err(format!(
                "control of session {} is held by `{holder_screen_id}`, and was not taken",
                target.session_id
            )),
        }
    }

    /// Write `bytes` to `target`'s main terminal, provided this caller's remembered token is still the
    /// session's lease and `screen_id` holds it. The lease is verified here explicitly: a jailed
    /// terminal does not check it on input, so this is the only check it gets. Input is written with
    /// no input offset, which the terminal reads as unset and does not acknowledge (`#home-inbox` 5/8).
    async fn send_keys(
        &self,
        target: &SessionTarget,
        screen_id: &str,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let session_id = target.session_id.as_str();
        let holder = self.terminal_control.holder_screen_id(session_id).await;
        let token = match self.leases.token_of(&self.claim_key(session_id)) {
            Some(token) if holder.as_deref() == Some(screen_id) => token,
            _ => return Err(control_refusal(session_id, holder.as_deref())),
        };
        if !self.terminal_control.verify(session_id, &token).await {
            let holder = self.terminal_control.holder_screen_id(session_id).await;
            return Err(control_refusal(session_id, holder.as_deref()));
        }
        let terminal = self
            .terminals
            .get_terminal(session_id, MAIN_TERMINAL_ID)
            .await
            .ok_or_else(|| format!("session {session_id} has no live terminal"))?;
        terminal.send_input(bytes.into(), 0);
        Ok(())
    }
}

/// Why a send to `session_id` is refused for want of control: another screen holds it, naming that
/// screen, or nothing does. Either way the agent claims it again, which is the one way back.
fn control_refusal(session_id: &str, holder: Option<&str>) -> String {
    match holder {
        Some(holder) => format!(
            "control of session {session_id} is held by `{holder}`; claim it again with ClaimSessionTerminal, then send"
        ),
        None => format!(
            "control of session {session_id} is not held; claim it with ClaimSessionTerminal first"
        ),
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
