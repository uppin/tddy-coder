use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_session_activity::session_deletion;
use tddy_session_agents::seed_codebase;
use tddy_session_agents::workspace_roster::workspace_roster_admits;
use tddy_session_split::workspace_session;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use std::path::Path;

use crate::launch_ports::LaunchSessions;
use tddy_service::proto::session::StartSessionRequest;

impl LaunchSessions {
    /// The records a workspace start seeds its roster with. A workspace session's roster holds the
    /// coding agents of the daemon serving it and no others, so each record is admitted to that
    /// roster here: a model agent, or an agent another daemon owns, is refused naming it
    /// (docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host).
    ///
    /// Admission applies only when `split_agent` is None. A split start is exempt, and so is the
    /// jailed-codebase start, which carries the same placement: the PRD keeps split sessions as
    /// they are ("Split sessions (remote-managed worktree)" under What's Staying the Same), and
    /// their roster already lives on the agent daemon. The seed is still resolved for them, because
    /// the codebase half holds the roster the session was started with.
    ///
    /// Called before the caller writes anything for the session, so a refused entry leaves no
    /// session behind. Resolving the records writes no roster and clones nothing; the only call it
    /// makes is a peer's listing of its agents.
    pub(super) async fn admitted_workspace_seed(
        &self,
        req: &StartSessionRequest,
    ) -> Result<Vec<tddy_core::SessionAgentRecord>, Status> {
        let seed = self
            .agent_roster
            .seeded_roster_records_placed(&req.specialized_agents, &req.clone_placed_agents)
            .await?;
        if req.split_agent.is_none() {
            let local_instance_id = local_instance_id_for_config(&self.config);
            for record in &seed {
                workspace_roster_admits(record, &local_instance_id)?;
            }
        }
        Ok(seed)
    }

    pub(super) async fn seed_and_start_workspace_session(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        paired_agent: Option<workspace_session::PairedAgentSession>,
        sessions_base: &Path,
        session_id: &str,
        seed: Vec<tddy_core::SessionAgentRecord>,
    ) -> Result<
        (
            Response<StartSessionResponse>,
            tddy_session_agents::seed_codebase::SeedCodebase,
            Vec<tddy_session_agents::seeded_clone_guard::SeededAgent>,
        ),
        Status,
    > {
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
            .agent_roster
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
        codebase: tddy_session_agents::seed_codebase::SeedCodebase,
        seeded: Vec<tddy_session_agents::seeded_clone_guard::SeededAgent>,
    ) {
        self.agent_roster
            .unwind_seeded_roster(session_id, &codebase, &req.session_token, seeded)
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
        placement: tddy_service::proto::session::AgentClonePlacement,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let started = workspace_session::start_agent_clone_session(
            os_user,
            &session_id,
            sessions_base.clone(),
            req.project_id.trim(),
            &self.tddy_data_dir,
        )
        .await?;
        self.agent_roster
            .start_hosted_agent_clone(
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
