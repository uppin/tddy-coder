//! `EnsureProjectHomeSession`: set up, replace, resume, or return a project's main — its
//! project-home session (`#project-home` 4/7).
//!
//! The project records its main as `home_session_id`. With a `start` spec:
//! - the spec is checked first ([`crate::project_home_start_spec`]): a type that cannot be held
//!   read-only, a sandbox placement or a requested role is refused, and nothing starts or stops;
//! - a recorded main this daemon still serves is stopped (its record and role stay, so it stays
//!   read-only), and the home worktree is fast-forwarded;
//! - main starts from the spec on the home worktree with the `project-home` role, and its id is
//!   recorded ([`EnsureOutcome::Replaced`] when a main existed, else [`EnsureOutcome::Started`]).
//!
//! Without one, for the recorded main:
//! - none recorded, or one whose session no longer exists (the record is then cleared) → not found;
//! - a stopped one → fast-forward the home worktree, then resume it, same conversation
//!   ([`EnsureOutcome::Resumed`]);
//! - a running one → return it unchanged ([`EnsureOutcome::Running`]).
//!
//! It builds a `StartSessionRequest` and calls the existing start and resume paths; it adds no
//! branch to `start_session_core` (`docs/code-issues/complexity-svc-start-session-core-*`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use std::path::{Path, PathBuf};

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_projects::project_storage::{self, ProjectData};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session::{
    EnsureProjectHomeSessionRequest, EnsureProjectHomeSessionResponse, ListSessionsRequest,
    ResumeSessionRequest, SessionEntry, StartSessionRequest,
};
use tddy_session_activity::session_reader;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::home_worktree::{ensure_home_worktree, fast_forward_home_worktree};

use crate::launch_ports::LaunchSessions;
use crate::project_home_start_spec::{
    on_home_worktree, refuse_a_type_main_cannot_run, refuse_what_main_cannot_honour,
};
use crate::session_coordinate_handlers::SESSION_SERVICE;

/// Serializes ensures, so two clicks cannot both find no main and start two.
static ENSURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whose sessions an ensure looks through, and for which project.
struct HomeScope {
    os_user: String,
    sessions_base: PathBuf,
    projects_dir: PathBuf,
    project: ProjectData,
}

/// What `EnsureProjectHomeSession` did, reported on the wire as `outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureOutcome {
    Started,
    Replaced,
    Resumed,
    Running,
}

impl EnsureOutcome {
    /// The wire spelling of this outcome.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Replaced => "replaced",
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

        let _one_ensure_at_a_time = ENSURE_LOCK.lock().await;
        let (session_id, outcome) = match req.start {
            Some(spec) => self.set_up_main(&req.session_token, &scope, spec).await?,
            None => self.reach_recorded_main(&req.session_token, &scope).await?,
        };
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
        let (projects_dir, project) = tddy_session_split::service_util::find_registered_project(
            &self.tddy_data_dir,
            &os_user,
            req.project_id.trim(),
        )?;
        Ok(HomeScope {
            os_user,
            sessions_base,
            projects_dir,
            project,
        })
    }

    /// The running / resumed decision for the main the project records; not found when it records
    /// none.
    async fn reach_recorded_main(
        &self,
        session_token: &str,
        scope: &HomeScope,
    ) -> Result<(String, EnsureOutcome), Status> {
        let Some(main) = self.recorded_main(scope)? else {
            return Err(Status::not_found(format!(
                "project {} has no main session: set one up",
                scope.project.project_id
            )));
        };
        if self.is_running(&main).await {
            return Ok((main.session_id, EnsureOutcome::Running));
        }
        self.resume_stopped_home(session_token, &scope.project, &scope.sessions_base, &main)
            .await?;
        Ok((main.session_id, EnsureOutcome::Resumed))
    }

    /// Start main from `spec` on the home worktree and record it, stopping a recorded main this
    /// daemon serves first. The spec is checked before anything is stopped or started.
    async fn set_up_main(
        &self,
        session_token: &str,
        scope: &HomeScope,
        spec: StartSessionRequest,
    ) -> Result<(String, EnsureOutcome), Status> {
        let project = &scope.project;
        refuse_what_main_cannot_honour(&spec)?;
        let spec = self.apply_harness_profile(StartSessionRequest {
            session_token: session_token.to_string(),
            ..spec
        })?;
        refuse_a_type_main_cannot_run(&spec)?;

        let previous = self.recorded_main(scope)?;
        let home = home_worktree_for(project)?;
        if let Some(previous) = &previous {
            // The worktree outlives the main it was made for; the new one starts at today's ref.
            fast_forward_home_worktree(&home, &main_branch_ref_of(project)?)
                .map_err(|e| Status::failed_precondition(format!("home worktree: {e}")))?;
            // TODO: a previous main whose pid is alive but whose terminal this daemon no longer
            // serves (a daemon restart) is left running
            // (docs/dev/todo/2026-10-23-project-home-running-heuristic.md).
            if self.is_served(&previous.session_id).await {
                self.claude_cli_manager
                    .stop_session(&previous.session_id)
                    .await;
                log::info!(
                    "EnsureProjectHomeSession: stopped main {} of project {} to replace it",
                    previous.session_id,
                    project.project_id
                );
            }
        }
        let start = on_home_worktree(spec, &project.project_id, &home);
        let started = self
            .start_session_with_role(start, &AttachmentProgressSink::discarding())
            .await?
            .into_inner()
            .session_id;
        record_main(&scope.projects_dir, &project.project_id, Some(&started))?;
        log::info!(
            "EnsureProjectHomeSession: started main {started} for project {} as {}",
            project.project_id,
            scope.os_user
        );
        let outcome = match previous {
            Some(_) => EnsureOutcome::Replaced,
            None => EnsureOutcome::Started,
        };
        Ok((started, outcome))
    }

    /// The session the project records as its main, if it still exists. A recorded session whose
    /// directory is gone (deleted) is cleared from the project, so the next ensure asks for setup.
    fn recorded_main(
        &self,
        scope: &HomeScope,
    ) -> Result<Option<session_reader::SessionEntry>, Status> {
        let project = &scope.project;
        let Some(session_id) = project
            .home_session_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            return Ok(None);
        };
        if !unified_session_dir_path(&scope.sessions_base, session_id).is_dir() {
            log::info!(
                "EnsureProjectHomeSession: main {session_id} of project {} no longer exists; \
                 clearing it",
                project.project_id
            );
            record_main(&scope.projects_dir, &project.project_id, None)?;
            return Ok(None);
        }
        let sessions = session_reader::list_sessions_in_dir(&scope.sessions_base)
            .map_err(|e| Status::internal(format!("listing sessions: {e}")))?;
        sessions
            .into_iter()
            .find(|s| s.session_id == session_id)
            .map(Some)
            .ok_or_else(|| {
                Status::internal(format!(
                    "main {session_id} of project {} has no readable session record",
                    project.project_id
                ))
            })
    }

    /// Running: its recorded pid is alive, or this daemon still serves its terminal. A live pid
    /// with no served terminal also reads as running
    /// (docs/dev/todo/2026-10-23-project-home-running-heuristic.md).
    async fn is_running(&self, main: &session_reader::SessionEntry) -> bool {
        main.is_active || self.is_served(&main.session_id).await
    }

    /// Whether this daemon still serves `session_id`'s terminal; the handle is dropped when the
    /// agent's process exits.
    async fn is_served(&self, session_id: &str) -> bool {
        self.claude_cli_manager.get(session_id).await.is_some()
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

/// Record (`Some`) or clear (`None`) `project_id`'s main in the projects store.
fn record_main(
    projects_dir: &Path,
    project_id: &str,
    session_id: Option<&str>,
) -> Result<(), Status> {
    project_storage::set_project_home_session(projects_dir, project_id, session_id)
        .map_err(|e| Status::internal(format!("recording project {project_id}'s main: {e}")))
}
