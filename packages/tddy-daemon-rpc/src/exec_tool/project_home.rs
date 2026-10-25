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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_host_service::HostServiceImpl;
use tddy_rpc::{Request, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{ListSessionsRequest, ListSessionsResponse};
use tddy_service::proto::tasks::CancelTaskRequest;
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_session_lifecycle::connection_service::start_choices;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_tool_engine::catalog::ToolDef;
use tddy_tool_engine::project_home_git_tools::is_project_home_git_tool;
use tddy_tool_engine::project_home_tools::{
    execute_project_home_tool, is_project_home_tool, session_offered_catalog,
    ListProjectSessionsResult, ListedSession, ProjectHomePort,
};
use tddy_tool_engine::project_home_worktree_tools::WorktreeEntry;
use tddy_tool_engine::session_follow::{SessionActivity, ToolCallBodies, TranscriptItem};
use tddy_tool_engine::start_ledger::{
    begin_recorded_start_with, StartHandle, StartLedger, StartLedgerRecord, StartState,
    StartedSessionDetails,
};
use tddy_tool_engine::start_options::{StartOptions, StartOptionsQuery};
use tddy_tool_engine::start_session_args::{StartContext, StartSessionArgs};
use tddy_worktree_service::service::project_main_repo_on_host;
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};
use tddy_worktree_service::worktrees::{project_worktree_snapshot, WorktreeSizeCalculator};

use super::project_home_remote::PeerSessionLists;
use super::project_home_session_follow;
use super::project_home_start_options::{describe_start_options, StartOptionsSources};
use super::ExecToolRpcHandler;
use crate::families::ProviderAccountsPort;
use crate::{CatalogRpcHandler, ProjectRpcHandler};

const SESSION_SERVICE: &str = "session.SessionService";

/// The `CancelTask` RPC a start's cancel is forwarded as, to the host that runs it.
const CANCEL_TASK_RPC: &str = "CancelTask";

/// The start ledgers this daemon process has already recovered, by their session directory.
static RECOVERED_START_LEDGERS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

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
                self.project_home_port(req, os_user, sessions_base, Some(peers))?
                    .execute(&home, req)
                    .await
            }
        };
        record_call(sessions_base, req, &response);
        Ok(response)
    }

    /// [`ProjectHomePort`] over this host for `req`'s caller, whose sessions are `os_user`'s in
    /// `sessions_base`. `peers` lists the other hosts' sessions; `None` lists this host's alone.
    pub(super) fn project_home_port(
        &self,
        req: &ExecuteToolRequest,
        os_user: String,
        sessions_base: &Path,
        peers: Option<PeerSessionLists>,
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
            peer_routing: self.peer_routing.clone(),
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
        Ok(session_offered_catalog(
            metadata.session_role.as_deref() == Some(PROJECT_HOME_ROLE),
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

/// Refuse `tool_name` unless `session_id` is a project-home session, and name that session's
/// project. The role is the session's own record, not anything the request claims.
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
    if metadata.session_role.as_deref() != Some(PROJECT_HOME_ROLE) {
        return Err(Status::permission_denied(format!(
            "tool {tool_name} is served only to a project-home session"
        )));
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
    /// Reaches the other hosts: a start's record and its cancel are forwarded to the host running it.
    peer_routing: PeerRouting,
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
        let outcome = execute_project_home_tool(self, home, &req.tool_name, &req.args_json).await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }

    /// The start ledger of `home_session_id`. The first time this daemon process loads it, the
    /// starts it still records as running are recovered: a start is only ever begun in a ledger after
    /// that, so a record still running is one an earlier daemon was draining when it stopped
    /// (`#start-parity` 4/9).
    fn start_ledger_of(&self, home_session_id: &str) -> anyhow::Result<StartLedger> {
        validate_session_id_segment(home_session_id)
            .map_err(|e| anyhow::anyhow!("{}", e.message()))?;
        let session_dir = unified_session_dir_path(&self.sessions_base, home_session_id);
        let ledger = StartLedger::in_session_dir(&session_dir);
        let recovered = RECOVERED_START_LEDGERS.get_or_init(Default::default);
        let mut recovered = recovered
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !recovered.contains(&session_dir) {
            let count = ledger.recover_interrupted()?;
            if count > 0 {
                log::info!(
                    "start ledger of session {home_session_id}: {count} start(s) left running by an earlier daemon were marked failed"
                );
            }
            recovered.insert(session_dir);
        }
        Ok(ledger)
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
        // Refused here, not by the start, so the caller gets the refusal in place of a handle.
        let conflict = self
            .launch
            .refuse_a_start_before_it_begins(&request, &self.os_user)
            .await
            .map_err(status_error)?;
        if let Some(conflict) = conflict {
            let owner = conflict
                .owner
                .map(|owner| format!(" by session {}", owner.session_id))
                .unwrap_or_default();
            anyhow::bail!(
                "the session was not started: branch {} is owned{owner}",
                conflict.branch
            );
        }
        let events = self
            .launch
            .stream_start_session_at_session_coordinate(Request::direct(request))
            .await
            .map_err(status_error)?
            .into_inner();
        let ledger = self.start_ledger_of(home_session_id)?;
        let details: Arc<dyn StartedSessionDetails> = Arc::new(StartedSessionListings {
            launch: self.launch.clone(),
            peer_routing: self.peer_routing.clone(),
            session_token: self.session_token.clone(),
        });
        begin_recorded_start_with(ledger, events, Some(details))
            .await
            .map_err(anyhow::Error::msg)
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
        let record = self.start_ledger_of(home_session_id)?.read(start_id)?;
        if record.state != StartState::Running {
            anyhow::bail!(
                "start {start_id} is not running: it has already {}",
                state_name(record.state)
            );
        }
        if record.daemon_instance_id == self.daemon_instance_id {
            let found = self.launch.cancel_local_task(&record.task_id).await;
            anyhow::ensure!(
                found,
                "start {start_id}'s task is no longer running on {}",
                record.daemon_instance_id
            );
            return Ok(());
        }
        let room = self
            .peer_routing
            .common_room_slot(CANCEL_TASK_RPC)
            .map_err(status_error)?;
        let answer = tddy_daemon_kernel::peer_calls::forward_cancel_task(
            room,
            &record.daemon_instance_id,
            &CancelTaskRequest {
                session_token: self.session_token.clone(),
                task_id: record.task_id.clone(),
                daemon_instance_id: record.daemon_instance_id.clone(),
            },
        )
        .await
        .map_err(status_error)?;
        anyhow::ensure!(
            answer.ok,
            "start {start_id} could not be cancelled on {}: {}",
            record.daemon_instance_id,
            answer.message
        );
        Ok(())
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

/// The branch and worktree of a started session, as the host running it lists it: this host's own
/// listing, or the peer's over the same forward a project-home listing uses (`#start-parity` 4/9).
struct StartedSessionListings {
    launch: LaunchSessions,
    peer_routing: PeerRouting,
    session_token: String,
}

#[async_trait]
impl StartedSessionDetails for StartedSessionListings {
    async fn branch_and_worktree(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<(String, String)> {
        let request = ListSessionsRequest {
            session_token: self.session_token.clone(),
        };
        let listed = match self
            .peer_routing
            .rpc_served_by_peer::<_, ListSessionsResponse>(
                SESSION_SERVICE,
                "ListSessions",
                daemon_instance_id,
                &request,
            )
            .await
        {
            Ok(Some(listed)) => listed,
            // Not a peer's: this host's own listing.
            Ok(None) => match self
                .launch
                .list_sessions_at_session_coordinate(Request::direct(request))
                .await
            {
                Ok(listed) => listed.into_inner(),
                Err(status) => {
                    log::warn!("start of session {session_id}: this host's sessions could not be listed: {status}");
                    return None;
                }
            },
            Err(status) => {
                log::warn!(
                    "start of session {session_id}: host {daemon_instance_id} could not list its sessions: {status}"
                );
                return None;
            }
        };
        listed
            .sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .map(|s| (s.branch, s.repo_path))
    }
}

pub(super) fn status_error(status: Status) -> anyhow::Error {
    anyhow::anyhow!(status.message().to_string())
}

fn state_name(state: StartState) -> &'static str {
    match state {
        StartState::Running => "running",
        StartState::Succeeded => "succeeded",
        StartState::Failed => "failed",
        StartState::Cancelled => "been cancelled",
    }
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
