//! `EnsureProjectHomeSession`: set up, replace, resume, or return a project's main — its
//! project-home session (`#project-home` 4/7).
//!
//! The project records its main as `home_session_id`. Ensures are serialized, and each reads the
//! project's record only once it holds its turn, so two set-ups cannot both find no main. With a
//! `start` spec:
//! - the spec is checked first ([`crate::project_home_start_spec`]): a type that cannot be held
//!   read-only, a placement other than the home worktree on this host, or a requested role is
//!   refused, and nothing starts or stops;
//! - the start itself is then checked the way the start path checks it (model, recipe, provider
//!   route — [`LaunchSessions::refuse_a_claude_cli_start_up_front`]), so a refused replace leaves
//!   the running main running and recorded;
//! - a recorded main this daemon still serves is stopped (its record and role stay, so it stays
//!   read-only), and the home worktree is fast-forwarded;
//! - main starts from the spec on the home worktree with the `project-home` role, and its id is
//!   recorded ([`EnsureOutcome::Replaced`] when a main existed, else [`EnsureOutcome::Started`]).
//!   A start that fails past those checks (the agent fails to spawn) leaves the old main stopped
//!   but still recorded, so an ensure without a spec resumes it. A started main whose id cannot be
//!   recorded is stopped again and the error returned, so no unrecorded main is left running.
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

/// The RPC this module answers: its peer-routing method name and its log prefix.
const ENSURE_RPC: &str = "EnsureProjectHomeSession";

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
            .rpc_served_by_peer(SESSION_SERVICE, ENSURE_RPC, &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        // Held before the project is read: what an ensure decides from `home_session_id` must not
        // go stale while a concurrent ensure starts or replaces main.
        let _one_ensure_at_a_time = ENSURE_LOCK.lock().await;
        let scope = self.caller_home_scope(&req)?;
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
        self.resume_stopped_home(session_token, scope, &main)
            .await?;
        Ok((main.session_id, EnsureOutcome::Resumed))
    }

    /// Start main from `spec` on the home worktree and record it, stopping a recorded main this
    /// daemon serves first. The spec and the start it makes are checked before anything is stopped
    /// or started.
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
        let home = home_worktree_for(project)?;
        let start = on_home_worktree(spec, &project.project_id, &home);
        self.refuse_a_claude_cli_start_up_front(&start)?;

        let previous = self.recorded_main(scope)?;
        if let Some(previous) = &previous {
            self.stop_previous_main(project, &home, previous).await?;
        }
        let started = self.start_and_record_main(scope, start).await?;
        let outcome = match previous {
            Some(_) => EnsureOutcome::Replaced,
            None => EnsureOutcome::Started,
        };
        Ok((started, outcome))
    }

    /// Make way for a new main: fast-forward the home worktree and stop `previous` if this daemon
    /// serves it. Its record and role stay, so it stays read-only.
    async fn stop_previous_main(
        &self,
        project: &ProjectData,
        home: &Path,
        previous: &session_reader::SessionEntry,
    ) -> Result<(), Status> {
        // The worktree outlives the main it was made for; the new one starts at today's ref.
        fast_forward_home(home, project)?;
        // TODO: a previous main whose pid is alive but whose terminal this daemon no longer
        // serves (a daemon restart) is left running
        // (docs/dev/todo/2026-10-23-project-home-running-heuristic.md).
        if self.is_served(&previous.session_id).await {
            self.claude_cli_manager
                .stop_session(&previous.session_id)
                .await;
            log::info!(
                "{ENSURE_RPC}: stopped main {} of project {} to replace it",
                previous.session_id,
                project.project_id
            );
        }
        Ok(())
    }

    /// Start `start` and record it as the project's main. A main whose id cannot be recorded is
    /// stopped again, so no project-home session runs that the project does not name.
    async fn start_and_record_main(
        &self,
        scope: &HomeScope,
        start: StartSessionRequest,
    ) -> Result<String, Status> {
        let project_id = &scope.project.project_id;
        let started = self
            .start_session_with_role(start, &AttachmentProgressSink::discarding())
            .await?
            .into_inner()
            .session_id;
        if let Err(status) = record_main(&scope.projects_dir, project_id, Some(&started)) {
            log::error!(
                "{ENSURE_RPC}: stopping main {started} of project {project_id}: {}",
                status.message
            );
            self.claude_cli_manager.stop_session(&started).await;
            return Err(status);
        }
        log::info!(
            "{ENSURE_RPC}: started main {started} for project {project_id} as {}",
            scope.os_user
        );
        Ok(started)
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
                "{ENSURE_RPC}: main {session_id} of project {} no longer exists; clearing it",
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

    /// Fast-forward a stopped main's home worktree to the project's main branch ref, then resume
    /// it in the same conversation.
    async fn resume_stopped_home(
        &self,
        session_token: &str,
        scope: &HomeScope,
        stopped: &session_reader::SessionEntry,
    ) -> Result<(), Status> {
        let session_dir = unified_session_dir_path(&scope.sessions_base, &stopped.session_id);
        let metadata = read_session_metadata(&session_dir)
            .map_err(|e| Status::internal(format!("home session metadata: {e}")))?;
        let home = metadata.repo_path.map(PathBuf::from).ok_or_else(|| {
            Status::failed_precondition("the home session has no worktree to resume in")
        })?;
        fast_forward_home(&home, &scope.project)?;
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

/// Fast-forward `home` to `project`'s main branch ref.
fn fast_forward_home(home: &Path, project: &ProjectData) -> Result<(), Status> {
    fast_forward_home_worktree(home, &main_branch_ref_of(project)?).map_err(home_worktree_failure)
}

/// A home worktree that could not be made or moved: the project is not in a state main can run in.
fn home_worktree_failure(error: impl std::fmt::Display) -> Status {
    Status::failed_precondition(format!("home worktree: {error}"))
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
    .map_err(home_worktree_failure)
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
