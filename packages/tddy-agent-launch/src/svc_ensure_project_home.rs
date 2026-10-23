//! `EnsureProjectHomeSession`: start, resume, or return a project's project-home session
//! (`#project-home` 4/7).
//!
//! The decision, by what exists for the project on this host:
//! - no home agent set, or one whose harness cannot enforce managed-codebase mode → refused;
//! - no project-home session → create the home worktree and start a managed session on it with the
//!   home agent and the `project-home` role ([`EnsureOutcome::Started`]);
//! - a stopped one → fast-forward the home worktree, then resume it, same conversation
//!   ([`EnsureOutcome::Resumed`]);
//! - a running one → return it unchanged ([`EnsureOutcome::Running`]).
//!
//! It builds a `StartSessionRequest` and calls the existing start and resume paths; it adds no
//! branch to `start_session_core` (`docs/code-issues/complexity-svc-start-session-core-*`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use std::path::{Path, PathBuf};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{write_session_metadata, PROJECT_HOME_ROLE};
use tddy_core::{read_session_metadata, SessionMetadata};
use tddy_discovery::harness_profile::{load_harness_profiles, HarnessProfile};
use tddy_projects::project_storage::ProjectData;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, ListSessionsRequest,
    ResumeSessionRequest, SessionEntry, StartSessionRequest,
};
use tddy_session_activity::session_reader;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::home_worktree::{ensure_home_worktree, fast_forward_home_worktree};

use crate::harness_profile_request::request_on_profile;
use crate::launch_ports::LaunchSessions;
use crate::session_coordinate_handlers::SESSION_SERVICE;

/// The one harness that can enforce managed-codebase mode, which is what makes the home session's
/// read-only guarantee real (`docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`).
const ENFORCING_HARNESS: &str = "claude-cli";

/// Serializes ensures, so two clicks cannot both find no home session and start two.
static ENSURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whose sessions an ensure looks through, and for which project.
struct HomeScope {
    os_user: String,
    sessions_base: PathBuf,
    project: ProjectData,
}

/// What `EnsureProjectHomeSession` did, reported on the wire as `outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureOutcome {
    Started,
    Resumed,
    Running,
}

impl EnsureOutcome {
    /// The wire spelling of this outcome.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Resumed => "resumed",
            Self::Running => "running",
        }
    }
}

impl LaunchSessions {
    pub async fn ensure_project_home_session_at_session_coordinate(
        &self,
        request: Request<EnsureProjectHomeSessionRequest>,
    ) -> Result<Response<EnsureProjectHomeSessionResponse>, Status> {
        let req = request.into_inner();
        // The home session is one per project per host: a request naming another daemon is that
        // daemon's to answer, the way `GetWorktreeSnapshot` is routed.
        if let Some(answered) = self
            .peer_routing
            .rpc_served_by_peer(
                SESSION_SERVICE,
                "EnsureProjectHomeSession",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let scope = self.caller_home_scope(&req)?;
        let profile = self.home_agent_profile(&scope.project)?;

        let _one_ensure_at_a_time = ENSURE_LOCK.lock().await;
        let (session_id, outcome) = self
            .ensure_on_this_host(&req.session_token, &scope, &profile)
            .await?;
        let session = self.listed_session(&req.session_token, &session_id).await?;
        Ok(Response::new(EnsureProjectHomeSessionResponse {
            session: Some(session),
            outcome: outcome.as_wire().to_string(),
        }))
    }

    /// The caller's OS user and sessions base, and the registered project the request names.
    fn caller_home_scope(
        &self,
        req: &EnsureProjectHomeSessionRequest,
    ) -> Result<HomeScope, Status> {
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let (_, project) = tddy_session_split::service_util::find_registered_project(
            &self.tddy_data_dir,
            &os_user,
            req.project_id.trim(),
        )?;
        Ok(HomeScope {
            os_user,
            sessions_base,
            project,
        })
    }

    /// The started / resumed / running decision for the project's home session on this daemon.
    async fn ensure_on_this_host(
        &self,
        session_token: &str,
        scope: &HomeScope,
        profile: &HarnessProfile,
    ) -> Result<(String, EnsureOutcome), Status> {
        let HomeScope {
            os_user,
            sessions_base,
            project,
        } = scope;
        let existing = find_home_session(sessions_base, &project.project_id)?;
        // Running: its recorded pid is alive, or this daemon still serves its terminal. A live pid
        // with no served terminal also reads as running
        // (docs/dev/todo/2026-10-23-project-home-running-heuristic.md).
        let running = match &existing {
            Some(home) => home.is_active || self.is_served(&home.session_id).await,
            None => false,
        };
        match existing {
            Some(home) if running => Ok((home.session_id, EnsureOutcome::Running)),
            Some(stopped) => {
                self.resume_stopped_home(session_token, project, sessions_base, &stopped)
                    .await?;
                Ok((stopped.session_id, EnsureOutcome::Resumed))
            }
            None => {
                let started = self
                    .start_home(session_token, os_user, project, sessions_base, profile)
                    .await?;
                Ok((started, EnsureOutcome::Started))
            }
        }
    }

    /// The harness profile the project's home agent names, refused unless its harness can enforce
    /// managed-codebase mode.
    fn home_agent_profile(&self, project: &ProjectData) -> Result<HarnessProfile, Status> {
        let agent = project.home_agent.as_deref().map(str::trim).unwrap_or("");
        if agent.is_empty() {
            return Err(Status::failed_precondition(format!(
                "project {} has no home agent: set one on the Projects screen",
                project.project_id
            )));
        }
        let profile = load_harness_profiles(&self.tddy_data_dir.join("agents"))
            .into_iter()
            .find(|profile| profile.name == agent)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "home agent {agent:?} is not a harness profile, so it names no harness to run"
                ))
            })?;
        if profile.harness != ENFORCING_HARNESS {
            return Err(Status::failed_precondition(format!(
                "home agent {agent:?} runs {}, which cannot enforce managed-codebase mode, so the \
                 home session could not be kept read-only; only {ENFORCING_HARNESS} can",
                profile.harness
            )));
        }
        Ok(profile)
    }

    /// Whether this daemon still serves `session_id`'s terminal; the handle is dropped when the
    /// agent's process exits.
    async fn is_served(&self, session_id: &str) -> bool {
        self.claude_cli_manager.get(session_id).await.is_some()
    }

    async fn start_home(
        &self,
        session_token: &str,
        os_user: &str,
        project: &ProjectData,
        sessions_base: &Path,
        profile: &HarnessProfile,
    ) -> Result<String, Status> {
        let home = home_worktree_for(project)?;
        let start = StartSessionRequest {
            session_token: session_token.to_string(),
            project_id: project.project_id.clone(),
            session_type: ENFORCING_HARNESS.to_string(),
            model: profile.model.clone(),
            managed_codebase: true,
            repo_path: home.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let start = if profile.provider_account.trim().is_empty() {
            start
        } else {
            let access = self.host.session_account_access(session_token);
            request_on_profile(start, profile, &access)?
        };
        let started = self
            .start_session_core(start, &AttachmentProgressSink::discarding())
            .await?
            .into_inner();
        record_home_role(sessions_base, &started.session_id)?;
        log::info!(
            "EnsureProjectHomeSession: started session {} for project {} as {os_user}",
            started.session_id,
            project.project_id
        );
        Ok(started.session_id)
    }

    async fn resume_stopped_home(
        &self,
        session_token: &str,
        project: &ProjectData,
        sessions_base: &Path,
        stopped: &session_reader::SessionEntry,
    ) -> Result<(), Status> {
        let session_dir = unified_session_dir_path(sessions_base, &stopped.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("home session metadata: {e}")))?;
        let home = metadata.repo_path.map(PathBuf::from).ok_or_else(|| {
            Status::failed_precondition("the home session has no worktree to resume in")
        })?;
        let main_ref = main_branch_ref_of(project)?;
        fast_forward_home_worktree(&home, &main_ref)
            .map_err(|e| Status::failed_precondition(format!("home worktree: {e}")))?;
        self.resume_session_at_session_coordinate(Request::direct(ResumeSessionRequest {
            session_token: session_token.to_string(),
            session_id: stopped.session_id.clone(),
        }))
        .await
        .map(|_| ())
    }

    async fn listed_session(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<SessionEntry, Status> {
        self.list_sessions_at_session_coordinate(Request::direct(ListSessionsRequest {
            session_token: session_token.to_string(),
        }))
        .await?
        .into_inner()
        .sessions
        .into_iter()
        .find(|entry| entry.session_id == session_id)
        .ok_or_else(|| Status::internal("the home session is not listed after it was ensured"))
    }
}

/// The project's main branch ref, which the home worktree is detached at.
fn main_branch_ref_of(project: &ProjectData) -> Result<String, Status> {
    project
        .main_branch_ref
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            Status::failed_precondition(format!(
                "project {} has no main branch ref for its home worktree",
                project.project_id
            ))
        })
}

/// The project's home worktree, created at its main branch ref when absent.
fn home_worktree_for(project: &ProjectData) -> Result<PathBuf, Status> {
    let main_repo = tddy_session_split::service_util::project_repo_root(project)?;
    let worktrees_root = tddy_core::worktree_dir(&main_repo);
    ensure_home_worktree(
        &main_repo,
        &worktrees_root,
        &project.project_id,
        &main_branch_ref_of(project)?,
    )
    .map_err(|e| Status::failed_precondition(format!("home worktree: {e}")))
}

/// The project's project-home session on this host, if one was ever started.
fn find_home_session(
    sessions_base: &Path,
    project_id: &str,
) -> Result<Option<session_reader::SessionEntry>, Status> {
    let sessions = session_reader::list_sessions_in_dir(sessions_base)
        .map_err(|e| Status::internal(format!("listing sessions: {e}")))?;
    Ok(sessions
        .into_iter()
        .find(|s| s.project_id == project_id && s.session_role == PROJECT_HOME_ROLE))
}

/// Mark `session_id` as its project's project-home session.
fn record_home_role(sessions_base: &Path, session_id: &str) -> Result<(), Status> {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    let mut metadata: SessionMetadata = read_session_metadata(&session_dir)
        .map_err(|e| Status::internal(format!("home session metadata: {e}")))?;
    metadata.session_role = Some(PROJECT_HOME_ROLE.to_string());
    write_session_metadata(&session_dir, &metadata)
        .map_err(|e| Status::internal(format!("recording the home session's role: {e}")))
}
