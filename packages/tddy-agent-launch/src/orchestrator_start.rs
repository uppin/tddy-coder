//! Orchestrator sessions on the start path: who may ask for the role, and where one is placed.
//!
//! An orchestrator is placed exactly as a project's main is — on the project's home worktree,
//! fast-forwarded to the main branch ref, managed — by the same placement and the same refusals
//! (`project_home_start_spec`); it differs from main in not being recorded as the project's main
//! and in not being auto-stopped.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::ORCHESTRATOR_ROLE;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

use crate::harness_profile_request::{admit_a_requested_role, RoleRequester};
use crate::launch_ports::LaunchSessions;
use crate::project_home_start_spec::{
    on_home_worktree, refuse_a_type_main_cannot_run, refuse_what_an_orchestrator_cannot_honour,
};
use crate::svc_ensure_project_home::{fast_forward_home, home_worktree_for};

impl LaunchSessions {
    /// Admit the standing role `req` asks for, by who asks: the token's principal says whether it
    /// is a person or a session's agent, and an agent's own role is read from the session it
    /// names as its orchestrator (the calling session).
    pub(crate) fn admit_the_requested_role(&self, req: &StartSessionRequest) -> Result<(), Status> {
        if req.session_role.trim().is_empty() {
            return Ok(());
        }
        let own_role = if self.split_sessions.token_is_an_agents(&req.session_token) {
            Some(self.own_role_of_the_calling_session(req))
        } else {
            None
        };
        let requester = match &own_role {
            None => RoleRequester::Person,
            Some(role) => RoleRequester::Agent {
                own_role: role.as_deref(),
            },
        };
        admit_a_requested_role(requester, req)
    }

    /// The role of the session `req` names as its orchestrator, on this daemon; `None` for a
    /// session that is not here, is unreadable, or holds none.
    fn own_role_of_the_calling_session(&self, req: &StartSessionRequest) -> Option<String> {
        let os_user = self.os_user_of(&req.session_token).ok()?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )?;
        let calling_session = req.orchestrator_session_id.trim();
        tddy_core::session_lifecycle::validate_session_id_segment(calling_session).ok()?;
        tddy_core::read_session_metadata(&unified_session_dir_path(&sessions_base, calling_session))
            .ok()?
            .session_role
    }

    /// `req` placed as an orchestrator of its project when it asks for the role, otherwise `req`
    /// unchanged. The spec is refused where an orchestrator could not honour it before anything is
    /// created; the project's home worktree is cut if absent and fast-forwarded to its main branch
    /// ref. A start naming another daemon is returned unplaced. `req` is the start after its harness profile was applied, so a profile cannot swap
    /// the session type in after the type check.
    pub(crate) fn place_an_orchestrator(
        &self,
        req: StartSessionRequest,
    ) -> Result<StartSessionRequest, Status> {
        if req.session_role.trim() != ORCHESTRATOR_ROLE {
            return Ok(req);
        }
        // A start naming another daemon is that daemon's to place — it holds the project's home
        // worktree it runs on — and is forwarded as it is.
        let target = req.daemon_instance_id.trim();
        if !target.is_empty() && target != local_instance_id_for_config(&self.config) {
            return Ok(req);
        }
        refuse_what_an_orchestrator_cannot_honour(&req)?;
        refuse_a_type_main_cannot_run(Some(&*self.coding_agents), &req)?;
        let os_user = self.os_user_of(&req.session_token)?;
        let (_, project) = tddy_session_split::service_util::find_registered_project(
            &self.tddy_data_dir,
            &os_user,
            req.project_id.trim(),
        )?;
        let home = home_worktree_for(&project)?;
        let start = on_home_worktree(req, ORCHESTRATOR_ROLE, &project.project_id, &home);
        self.refuse_a_claude_cli_start_up_front(&start)?;
        fast_forward_home(&home, &project)?;
        Ok(start)
    }

    /// Fast-forward the home worktree of a stopped orchestrator about to resume, so it resumes
    /// on the project's main branch ref as it starts on it. A session that is no orchestrator is
    /// left as it is.
    pub(crate) fn fast_forward_a_resumed_orchestrators_home(
        &self,
        os_user: &str,
        metadata: &tddy_core::SessionMetadata,
    ) -> Result<(), Status> {
        if metadata.session_role.as_deref() != Some(ORCHESTRATOR_ROLE) {
            return Ok(());
        }
        let (_, project) = tddy_session_split::service_util::find_registered_project(
            &self.tddy_data_dir,
            os_user,
            &metadata.project_id,
        )?;
        let home = metadata
            .repo_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                Status::failed_precondition("the orchestrator has no worktree to resume in")
            })?;
        fast_forward_home(&home, &project)
    }

    /// The OS user the token's login maps to.
    fn os_user_of(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }
}
