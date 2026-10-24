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
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_rpc::{Request, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{ListSessionsRequest, StartSessionRequest};
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_tool_engine::catalog::ToolDef;
use tddy_tool_engine::project_home_git_tools::is_project_home_git_tool;
use tddy_tool_engine::project_home_tools::{
    execute_project_home_tool, is_project_home_tool, session_offered_catalog, ListedSession,
    ProjectHomePort, StartProjectSessionArgs,
};
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget, TargetRecord};

use super::ExecToolRpcHandler;

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
        let port = DaemonProjectHome {
            launch: self.attached_launch(&req.tool_name)?,
            session_token: req.session_token.clone(),
            os_user: self.os_user_of(&req.session_token)?,
            sessions_base: sessions_base.to_path_buf(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            identity: SessionAccountAccess::new(
                self.credential_vaults.clone(),
                self.user_resolver.clone(),
                &req.session_token,
            )
            .with_github_cli(self.github_cli.clone()),
        };
        let outcome = execute_project_home_tool(&port, &home, &req.tool_name, &req.args_json).await;
        let response = ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        };
        record_call(sessions_base, req, &response);
        Ok(response)
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
    /// a project-home session is offered the project-home catalog, and any other session the daemon-wide
    /// catalog without the project-home tools.
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
        ))
    }

    /// The OS user `session_token` maps to, as the start path resolves it.
    fn os_user_of(&self, session_token: &str) -> Result<String, Status> {
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
struct DaemonProjectHome {
    launch: LaunchSessions,
    session_token: String,
    os_user: String,
    sessions_base: PathBuf,
    tddy_data_dir: PathBuf,
    /// Reads the signed-in person's vault for a target's GitHub identity, as a session start does.
    identity: SessionAccountAccess,
}

/// One session as the target resolver reads it: its `.session.yaml`, its changeset's branch, and the
/// ref its worktree is diffed against.
struct SessionRow {
    entry: SessionEntry,
    branch: String,
    base_ref: String,
}

impl DaemonProjectHome {
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
            })
            .collect())
    }

    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartProjectSessionArgs,
    ) -> anyhow::Result<String> {
        let started = self
            .launch
            .start_session_at_session_coordinate(Request::direct(StartSessionRequest {
                session_token: self.session_token.clone(),
                project_id: project_id.to_string(),
                session_type: args.session_type,
                agent: args.agent,
                initial_prompt: args.prompt,
                branch_worktree_intent: if args.new_branch_name.trim().is_empty() {
                    String::new()
                } else {
                    "new_branch_from_base".to_string()
                },
                new_branch_name: args.new_branch_name,
                model: args.model,
                ..Default::default()
            }))
            .await
            .map_err(|status| anyhow::anyhow!(status.message().to_string()))?
            .into_inner();
        if started.branch_conflict.is_some() {
            anyhow::bail!("the session was not started: its branch is owned by another session");
        }
        tag_orchestrator(&self.sessions_base, &started.session_id, home_session_id)?;
        Ok(started.session_id)
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

/// Record on the new session's changeset that `home_session_id` started it.
fn tag_orchestrator(
    sessions_base: &Path,
    session_id: &str,
    home_session_id: &str,
) -> anyhow::Result<()> {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let mut changeset = tddy_core::read_changeset(&session_dir)?;
    changeset.orchestrator_session_id = Some(home_session_id.to_string());
    tddy_core::write_changeset(&session_dir, &changeset)?;
    Ok(())
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
