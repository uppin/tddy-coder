//! The project-home session's exec tools, answered on this daemon (`#project-home` 5/7).
//!
//! The four tools in [`tddy_tool_engine::project_home_tools`] are served here rather than by the
//! local tool runner: this is where the caller's role is read, where the daemon's session store and
//! start path are reached through [`ProjectHomePort`], and where the call is recorded. Every other
//! exec tool still goes to [`LocalExecTools::run_exec_tool_locally`](tddy_session_lifecycle::connection_service::LocalExecTools::run_exec_tool_locally)
//! unchanged.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session-tools.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};
use tddy_session_split::workspace_session::{start_workspace_session, WorkspaceBranchIntent};
use tddy_tool_engine::project_home_tools::{
    execute_project_home_tool, is_project_home_tool, ListedSession, ProjectHomePort,
    StartProjectSessionArgs,
};
use tddy_worktree_service::session_target::{HomeCaller, TargetRecord};

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
        if !is_project_home_tool(&req.tool_name) {
            return Ok(self
                .local_exec_tools
                .run_exec_tool_locally(req, sessions_base, worktree_root)
                .await);
        }
        let home = project_home_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let port = DaemonProjectHome {
            os_user: self.os_user_of(&req.session_token)?,
            sessions_base: sessions_base.to_path_buf(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            request_timeout: self.config.spawn_worker_request_timeout(),
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

/// [`ProjectHomePort`] over this daemon's session store and its workspace start path, for one
/// caller: the OS user and sessions tree the caller's request resolved.
struct DaemonProjectHome {
    os_user: String,
    sessions_base: PathBuf,
    tddy_data_dir: PathBuf,
    request_timeout: Duration,
}

/// One session as the project-home tools read it: its `.session.yaml`, its changeset's branch, and
/// the ref its worktree is diffed against.
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
        Ok(self
            .rows()
            .await?
            .into_iter()
            .filter(|row| {
                row.entry.project_id == project_id && row.entry.session_id != home_session_id
            })
            .map(|row| ListedSession {
                session_id: row.entry.session_id,
                branch: row.branch,
                worktree: row.entry.repo_path,
                is_active: row.entry.is_active,
                agent_status: row.entry.status,
                // The listing does not carry a session's pending elicitation; reporting `false` here
                // is a known gap, not a measurement.
                pending_elicitation: false,
            })
            .collect())
    }

    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartProjectSessionArgs,
    ) -> anyhow::Result<String> {
        // FIXME(home-session-tools): only the workspace session type is started from here. Every
        // other type needs the daemon's full start path, which lives on the host and is not reachable
        // from this handler without a cycle through the host's installed RPC families.
        if args.session_type.trim() != "workspace" {
            anyhow::bail!(
                "StartProjectSession starts workspace sessions only; session type {:?} is not served from a project-home session yet",
                args.session_type
            );
        }
        if !args.prompt.trim().is_empty() || !args.agent.trim().is_empty() {
            anyhow::bail!("a workspace session takes no prompt or agent");
        }
        let new_branch_name = args.new_branch_name.trim();
        let intent = WorkspaceBranchIntent {
            branch_worktree_intent: if new_branch_name.is_empty() {
                ""
            } else {
                "new_branch_from_base"
            },
            new_branch_name,
            selected_integration_base_ref: "",
            selected_branch_to_work_on: "",
        };
        let session_id = uuid::Uuid::new_v4().to_string();
        start_workspace_session(
            &self.os_user,
            &session_id,
            self.sessions_base.clone(),
            project_id,
            &intent,
            None,
            false,
            &self.tddy_data_dir,
            self.request_timeout,
        )
        .await
        .map_err(|status| anyhow::anyhow!(status.message().to_string()))?;
        tag_orchestrator(&self.sessions_base, &session_id, home_session_id)?;
        Ok(session_id)
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
