//! `CheckBaseFreshness` and `RefreshHome` answered on this host (`#orchestra` 4/9).
//!
//! Routed from `run_locally` ahead of the local runner, as the session message tools are. Served to
//! an orchestrator role alone, on **the caller's own home worktree** — the checkout its session
//! record names — against its project's trunk ref
//! ([`tddy_projects::project_storage::effective_integration_base_ref_for_project`]). A worker
//! session is refused.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_tool_engine::base_freshness_tools::{execute_base_freshness_tool, BaseFreshnessPort};
use tddy_worktree_service::base_freshness::{base_freshness, refresh_home, BaseFreshness};
use tddy_worktree_service::session_target::HomeCaller;

use super::project_home::caller::{orchestrator_caller, record_call};
use super::ExecToolRpcHandler;

/// [`BaseFreshnessPort`] over this host: the caller's home worktree and its project's trunk.
struct DaemonBaseFreshness {
    sessions_base: PathBuf,
    projects_dir: PathBuf,
}

impl DaemonBaseFreshness {
    /// The caller's home worktree and the trunk ref of its project.
    fn home_and_trunk(&self, caller: &HomeCaller) -> Result<(PathBuf, String), String> {
        let dir = unified_session_dir_path(&self.sessions_base, &caller.session_id);
        let home = tddy_core::read_session_metadata(&dir)
            .map_err(|e| format!("session {}'s record cannot be read: {e}", caller.session_id))?
            .repo_path
            .ok_or_else(|| format!("session {} has no home worktree", caller.session_id))?;
        let trunk = tddy_projects::project_storage::effective_integration_base_ref_for_project(
            &self.projects_dir,
            &caller.project_id,
        )
        .map_err(|e| format!("the project's trunk could not be resolved: {e}"))?;
        Ok((PathBuf::from(home), trunk))
    }
}

#[async_trait]
impl BaseFreshnessPort for DaemonBaseFreshness {
    async fn check_base_freshness(&self, caller: &HomeCaller) -> Result<BaseFreshness, String> {
        let (home, trunk) = self.home_and_trunk(caller)?;
        tokio::task::spawn_blocking(move || base_freshness(&home, &trunk))
            .await
            .map_err(|e| e.to_string())?
    }

    async fn refresh_home(&self, caller: &HomeCaller) -> Result<BaseFreshness, String> {
        let (home, trunk) = self.home_and_trunk(caller)?;
        tokio::task::spawn_blocking(move || refresh_home(&home, &trunk))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|refusal| refusal.to_string())
    }
}

impl ExecToolRpcHandler {
    /// Serve `req`'s base-freshness tool for an orchestrator caller and record the call.
    pub(super) async fn run_base_freshness_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        let caller = orchestrator_caller(sessions_base, &req.session_id, &req.tool_name)?;
        let os_user = self.os_user_of(&req.session_token)?;
        let projects_dir = projects_path_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        let port = DaemonBaseFreshness {
            sessions_base: sessions_base.to_path_buf(),
            projects_dir,
        };
        let outcome = execute_base_freshness_tool(&port, &caller, &req.tool_name).await;
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
}
