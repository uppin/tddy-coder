use std::time::Duration;
use tddy_daemon_extensions::SessionRooms as WorktreeRoomCloser;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use super::project_main_repo_on_host;

use crate::{
    target_worktrees::TargetSession,
    worktrees::{self, WorktreeStatsCache, DAEMON_SIZE_WALK_PERMITS},
};

use tddy_daemon_kernel::user_paths::sessions_base_for_user;

use tddy_rpc::Status;

use tddy_task::IdleTimeoutTracker;

use crate::pull_request_view::SessionPullRequests;

use tddy_daemon_extensions::NoSessionRooms;

use crate::worktrees::WorktreeSizeCalculator;

use std::sync::Arc;

use tddy_daemon_kernel::SessionUserResolver;

use std::path::PathBuf;

use tddy_daemon_kernel::config::DaemonConfig;

use super::WorktreeServiceImpl;

impl WorktreeServiceImpl {
    /// A service reading and writing this daemon's own stats cache under `tddy_data_dir`.
    pub fn new(
        config: DaemonConfig,
        tddy_data_dir: PathBuf,
        user_resolver: SessionUserResolver,
    ) -> Self {
        let cache_root = worktrees::projects_stats_cache_root(&tddy_data_dir);
        Self {
            worktree_stats_cache: Arc::new(WorktreeStatsCache::new(cache_root.clone())),
            worktree_size_calculator: Arc::new(WorktreeSizeCalculator::new(
                cache_root,
                DAEMON_SIZE_WALK_PERMITS,
            )),
            session_rooms: Arc::new(NoSessionRooms),
            idle_tracker: None,
            target_shells: Arc::new(crate::target_worktrees::NoTargetShells),
            target_row_bus: Arc::default(),
            pull_requests: Arc::default(),
            tddy_data_dir,
            user_resolver,
            config,
        }
    }

    /// The rooms to close before a checkout is removed.
    #[must_use]
    pub fn with_session_rooms(mut self, rooms: Arc<dyn WorktreeRoomCloser>) -> Self {
        self.session_rooms = rooms;
        self
    }

    /// Reach target sessions' worktrees through `shells` (`#target-parity` 2/7).
    #[must_use]
    pub fn with_target_shells(
        mut self,
        shells: Arc<dyn crate::target_worktrees::TargetShells>,
    ) -> Self {
        self.target_shells = shells;
        self
    }

    /// Read sessions' PRs through `pull_requests` (`#workqueue` 4/14). Bound after construction
    /// because the host that owns the inbox is built after this service; shared by every clone, and
    /// bound once.
    ///
    /// # Panics
    /// When called a second time — the wiring is one decision, not a replaceable setting.
    pub fn attach_pull_requests(&self, pull_requests: Arc<dyn SessionPullRequests>) {
        assert!(
            self.pull_requests.set(pull_requests).is_ok(),
            "the PR source is attached once"
        );
    }

    /// Count worktree RPCs as activity for relay mode's idle timer.
    #[must_use]
    pub fn with_idle_tracker(mut self, tracker: Arc<IdleTimeoutTracker>) -> Self {
        self.idle_tracker = Some(tracker);
        self
    }

    /// Swap the size calculator — a test wants a deterministic, instant sizer via
    /// [`WorktreeSizeCalculator::with_sizer`] rather than a real disk walk.
    #[must_use]
    pub fn with_worktree_size_calculator(
        mut self,
        calculator: Arc<WorktreeSizeCalculator>,
    ) -> Self {
        self.worktree_size_calculator = calculator;
        self
    }

    /// The stats cache this service reads, so a caller that also writes it shares one.
    #[must_use]
    pub fn worktree_stats_cache(&self) -> Arc<WorktreeStatsCache> {
        Arc::clone(&self.worktree_stats_cache)
    }

    /// The size calculator this service streams and sizes from, so a caller that lists worktrees
    /// (the project-home tools) reads the same calculation state the Worktrees screen does.
    #[must_use]
    pub fn worktree_size_calculator(&self) -> Arc<WorktreeSizeCalculator> {
        Arc::clone(&self.worktree_size_calculator)
    }

    pub(crate) fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    /// Authenticate, then answer with the OS user this daemon runs the caller's work as.
    pub(crate) fn authorize(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    /// The directory of session `session_id` under the caller's own sessions base — the one rule
    /// that says a session is the caller's: token → OS user → `<sessions base>/sessions/<id>`.
    /// Not checked against disk; see [`Self::resolve_owned_session_dir`] for that.
    pub(crate) fn session_dir_for(
        &self,
        os_user: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        tddy_core::session_lifecycle::validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions base"))?;
        Ok(sessions_base
            .join(tddy_core::output::SESSIONS_SUBDIR)
            .join(session_id))
    }

    /// The shell of `session`'s target, resolved as the caller (the subject a session start
    /// resolves a Coder target's account with). A refusal is returned as it is.
    pub(crate) async fn shell_of_target(
        &self,
        session_token: &str,
        session: &TargetSession,
    ) -> Result<Arc<dyn tddy_remote_shell::RemoteShell>, Status> {
        let subject = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.target_shells
            .shell_for(&session.target, &subject)
            .await
    }

    pub(crate) fn target_row_bus(&self) -> &Arc<crate::target_worktree_stats::TargetRowBus> {
        &self.target_row_bus
    }

    /// The caller's sessions directory: `<sessions base>/sessions`.
    pub(crate) fn sessions_dir_of(&self, os_user: &str) -> Option<PathBuf> {
        sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .map(|base| base.join(tddy_core::output::SESSIONS_SUBDIR))
    }

    /// The caller's target session of `project_id` whose worktree on its target is `worktree`.
    pub(crate) fn target_session_of(
        &self,
        os_user: &str,
        project_id: &str,
        worktree: &str,
    ) -> Option<TargetSession> {
        let sessions_dir = self.sessions_dir_of(os_user)?;
        crate::target_worktrees::target_session_of_worktree(&sessions_dir, project_id, worktree)
    }

    /// Authenticates the caller and returns the directory of `session_id` when it exists under the
    /// caller's own OS user. A session of another user, and one that does not exist, are the same
    /// `NotFound` — the answer must not reveal which session ids exist.
    pub fn resolve_owned_session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        let os_user = self.authorize(session_token)?;
        let session_dir = self.session_dir_for(&os_user, session_id.trim())?;
        if !session_dir.is_dir() {
            return Err(Status::not_found("session not found"));
        }
        Ok(session_dir)
    }

    /// The project's main repository **on this host**, or the refusal that says why not.
    ///
    /// Every method here starts with this, in this order, and the order is the contract: an invalid
    /// token is `Unauthenticated` before any path is resolved, an unknown project is `NotFound`
    /// before any directory is read, and a registered project whose checkout is missing on disk is
    /// `InvalidArgument` rather than an internal error — the registration is real, the path is not.
    pub(crate) fn resolve_main_repo(
        &self,
        os_user: &str,
        project_id: &str,
    ) -> Result<PathBuf, Status> {
        project_main_repo_on_host(
            &self.tddy_data_dir,
            os_user,
            &local_instance_id_for_config(&self.config),
            project_id,
        )
    }

    /// Authenticates the caller, resolves the project's main repo on this host, and confirms
    /// `worktree_path` appears in that repo's `git worktree list`, returning the validated worktree
    /// root.
    ///
    /// The membership gate is what keeps filesystem access from escaping a real worktree: the path
    /// arrives as free text from a browser, and this daemon can reach files its caller cannot.
    pub fn resolve_listed_worktree(
        &self,
        session_token: &str,
        project_id: &str,
        worktree_path: &str,
    ) -> Result<PathBuf, Status> {
        let os_user = self.authorize(session_token)?;
        let worktree_path_raw = worktree_path.trim();
        if worktree_path_raw.is_empty() {
            return Err(Status::invalid_argument("worktree_path is required"));
        }
        let main_repo = self.resolve_main_repo(&os_user, project_id.trim())?;
        let worktree_path = PathBuf::from(worktree_path_raw);
        if !worktrees::worktree_path_is_listed(&main_repo, &worktree_path) {
            log::warn!(
                "resolve_listed_worktree: worktree_path not in git worktree list: {:?}",
                worktree_path
            );
            return Err(Status::failed_precondition(
                "worktree_path is not a worktree of this project",
            ));
        }
        Ok(worktree_path)
    }

    pub(crate) fn request_timeout(&self) -> Duration {
        self.config.spawn_worker_request_timeout()
    }
}
