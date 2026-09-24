use super::DaemonSessionHost;

use crate::{
    connection_service::seed_codebase, session_deletion,
    user_sessions_path::projects_path_for_user, workspace_session,
};

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::path::Path;

use tddy_service::proto::session::StartSessionRequest;

impl DaemonSessionHost {
    pub(super) async fn seed_and_start_workspace_session(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        paired_agent: Option<workspace_session::PairedAgentSession>,
        sessions_base: &Path,
        session_id: &str,
        timeout: std::time::Duration,
    ) -> Result<
        (
            Response<StartSessionResponse>,
            super::super::SeedCodebase,
            Vec<super::super::SeededAgent>,
        ),
        Status,
    > {
        let seed = self.seeded_roster_records(&req.specialized_agents).await?;
        let started = workspace_session::start_workspace_session(
            os_user,
            session_id,
            sessions_base.to_path_buf(),
            req.project_id.trim(),
            &workspace_session::WorkspaceBranchIntent {
                branch_worktree_intent: req.branch_worktree_intent.trim(),
                new_branch_name: req.new_branch_name.trim(),
                selected_integration_base_ref: req.selected_integration_base_ref.trim(),
                selected_branch_to_work_on: req.selected_branch_to_work_on.trim(),
            },
            paired_agent.as_ref(),
            req.sandbox,
            &self.tddy_data_dir,
            timeout,
        )
        .await?;
        // Written before this call answers, because the answer is what releases the agent
        // host to spawn its agent — and that spawn fixes the tool allowlist at launch. A
        // roster written afterwards would leave a seeded agent's `replaces` unenforced until
        // the first resume. The seed takes its own artifacts back out on failure; the
        // session it was recorded on is the caller's to reclaim, which is what the split
        // start's teardown does with the id it minted.
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        let codebase = seed_codebase::SeedCodebase::read(session_id, &session_dir)?;
        let seeded = self
            .seed_session_agent_roster(session_id, &codebase, &req.session_token, seed)
            .await?;
        Ok((started, codebase, seeded))
    }

    pub(super) async fn remove_unconfined_workspace_session(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        sessions_base: &Path,
        session_id: &str,
        codebase: super::super::SeedCodebase,
        seeded: Vec<super::super::SeededAgent>,
    ) {
        self.unwind_seeded_roster(session_id, &codebase, &req.session_token, seeded)
            .await;
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir));
        if let Err(e) = session_deletion::delete_session_directory(
            sessions_base,
            session_id,
            projects_dir.as_deref(),
        ) {
            log::warn!(
                "StartSession: could not remove session {session_id} after its \
                                 jail could not be provisioned: {}",
                e.message()
            );
        }
    }

    pub(super) async fn start_agent_clone_workspace_session(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        sessions_base: std::path::PathBuf,
        session_id: String,
        timeout: std::time::Duration,
        placement: tddy_service::proto::session::AgentClonePlacement,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let started = workspace_session::start_agent_clone_session(
            os_user,
            &session_id,
            sessions_base.clone(),
            req.project_id.trim(),
            &self.tddy_data_dir,
            timeout,
        )
        .await?;
        self.start_hosted_agent_clone(
            &placement,
            &sessions_base,
            &session_id,
            req.project_id.trim(),
            &req.session_token,
        )
        .await?;
        Ok(started)
    }
}
