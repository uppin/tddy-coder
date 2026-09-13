//! `worktree.WorktreeService` — the nine RPCs a browser asks about a project's checkouts.
//!
//! Nothing here routes to a peer, and that is a property of the subject rather than an omission: a
//! worktree is a directory on the daemon that holds it, and no request in this proto carries a
//! `daemon_instance_id` to route by. What every method does instead is resolve the caller to an OS
//! user, the project to *this* host's main repo, and — for anything that touches a path — gate on
//! git's own `worktree list`, so filesystem access can never escape a real checkout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::user_paths::{projects_path_for_user, sessions_base_for_user};
use tddy_daemon_kernel::SessionUserResolver;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::worktree::{
    CalculateWorktreeSizeRequest, CalculateWorktreeSizeResponse, CleanWorktreeRequest,
    CleanWorktreeResponse, ListWorktreeDirectoryRequest, ListWorktreeDirectoryResponse,
    ListWorktreesForProjectRequest, ListWorktreesForProjectResponse, ReadWorktreeFileRequest,
    ReadWorktreeFileResponse, RemoveWorktreeRequest, RemoveWorktreeResponse,
    RestoreSessionWorktreeRequest, RestoreSessionWorktreeResponse, StreamWorktreeStatsRequest,
    WorktreeDirEntry, WorktreeFileChunk, WorktreeRow, WorktreeService, WorktreeStatsEvent,
};
use tddy_task::IdleTimeoutTracker;

use crate::project_storage;
use crate::stream::{worktree_file_frames, MpscResultStream, MpscWorktreeStatsStream};
use crate::worktrees::{
    self, CleanWorktreeError, RemoveWorktreeError, WorktreeDiffRow, WorktreeSizeCalculator,
    WorktreeSizeStatus, WorktreeStatsCache,
};

/// What must stop watching a checkout before it is taken away.
///
/// A port rather than a direct call, because the thing that watches is a **session room** — family
/// C, which stays in `tddy-daemon` deliberately. The ordering it exists to preserve is the whole
/// point: a room measures its directory on an interval, so one still open on a path that has just
/// been removed shells out to git in a directory that no longer exists, and warns at the poll rate
/// for the life of the daemon. `RemoveWorktree` knows a path and never a session id, so the ask is
/// by path.
pub trait WorktreeRoomCloser: Send + Sync {
    fn close_for_worktree(&self, worktree_path: &Path);
}

/// A daemon with no session rooms — every fixture, and any embedder that hosts no LiveKit rooms.
///
/// Named rather than an `Option<…>` on the service so the "nothing to close" case is a decision
/// somebody wrote down, not a `None` that could equally mean "wiring forgotten".
pub struct NoSessionRooms;

impl WorktreeRoomCloser for NoSessionRooms {
    fn close_for_worktree(&self, _worktree_path: &Path) {}
}

/// Everything `worktree.WorktreeService` answers from.
///
/// `Clone` is shallow and shared: the stats cache and the size calculator sit behind `Arc`s, so a
/// clone talks to the same ones. The streaming handler needs that — it hands its forwarding loop to
/// a `tokio::spawn`ed task which must own a `'static` service.
#[derive(Clone)]
pub struct WorktreeServiceImpl {
    config: DaemonConfig,
    tddy_data_dir: PathBuf,
    user_resolver: SessionUserResolver,
    /// The eager per-project stats walk behind `ListWorktreesForProject`.
    worktree_stats_cache: Arc<WorktreeStatsCache>,
    /// The lazy, semaphore-bounded per-worktree disk-size calculator behind `StreamWorktreeStats`
    /// and `CalculateWorktreeSize`. Shares the stats cache root, so a fresh calculator serves
    /// persisted sizes without re-walking.
    worktree_size_calculator: Arc<WorktreeSizeCalculator>,
    /// Closed before a checkout is removed — see [`WorktreeRoomCloser`].
    session_rooms: Arc<dyn WorktreeRoomCloser>,
    /// Bumped on the streaming and sizing RPCs, so relay mode's idle timer sees worktree traffic.
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
}

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
            // Daemon-global cap of 2 concurrent size walks.
            worktree_size_calculator: Arc::new(WorktreeSizeCalculator::new(cache_root, 2)),
            session_rooms: Arc::new(NoSessionRooms),
            idle_tracker: None,
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

    fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    /// Authenticate, then answer with the OS user this daemon runs the caller's work as.
    fn authorize(&self, session_token: &str) -> Result<String, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        self.config
            .os_user_for_github(&github_user)
            .map(str::to_string)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))
    }

    /// The project's main repository **on this host**, or the refusal that says why not.
    ///
    /// Every method here starts with this, in this order, and the order is the contract: an invalid
    /// token is `Unauthenticated` before any path is resolved, an unknown project is `NotFound`
    /// before any directory is read, and a registered project whose checkout is missing on disk is
    /// `InvalidArgument` rather than an internal error — the registration is real, the path is not.
    fn resolve_main_repo(&self, os_user: &str, project_id: &str) -> Result<PathBuf, Status> {
        if project_id.is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve projects path"))?;
        project_storage::find_project(&projects_dir, project_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("project not found"))?;

        let local_id = local_instance_id_for_config(&self.config);
        let main_repo_str =
            project_storage::main_repo_path_for_host(&projects_dir, project_id, local_id.as_str())
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;

        let main_repo = PathBuf::from(&main_repo_str);
        if !main_repo.exists() {
            return Err(Status::invalid_argument(
                "project main repo path does not exist",
            ));
        }
        Ok(main_repo)
    }

    /// Authenticates the caller, resolves the project's main repo on this host, and confirms
    /// `worktree_path` appears in that repo's `git worktree list`, returning the validated worktree
    /// root.
    ///
    /// The membership gate is what keeps filesystem access from escaping a real worktree: the path
    /// arrives as free text from a browser, and this daemon can reach files its caller cannot.
    fn resolve_listed_worktree(
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

    fn request_timeout(&self) -> Duration {
        self.config.spawn_worker_request_timeout()
    }
}

/// Run blocking git/filesystem work under a wall-clock cap, so a hung git cannot block an RPC
/// forever.
///
/// `op_label` names the RPC in the deadline message, because the operator reading it is looking at
/// one call and needs to know which.
async fn blocking_within<T: Send + 'static>(
    timeout: Duration,
    op_label: &'static str,
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(v))) => Ok(v),
        Ok(Ok(Err(e))) => {
            log::error!("{} failed: {}", op_label, e);
            Err(Status::internal(e.to_string()))
        }
        Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
        Err(_elapsed) => Err(Status::deadline_exceeded(format!(
            "{}: timed out after {}s (spawn_worker_request_timeout_secs)",
            op_label,
            timeout.as_secs()
        ))),
    }
}

/// Map the library disk-size lifecycle status to its wire enum.
fn proto_worktree_size_status(
    status: WorktreeSizeStatus,
) -> tddy_service::proto::worktree::WorktreeSizeStatus {
    use tddy_service::proto::worktree::WorktreeSizeStatus as Wire;
    match status {
        WorktreeSizeStatus::None => Wire::None,
        WorktreeSizeStatus::Calculating => Wire::Calculating,
        WorktreeSizeStatus::Cached => Wire::Cached,
    }
}

/// Build a `WorktreeRow` from a worktree's branch/diff summary plus its current size state. The
/// size fields (`disk_bytes`, `size_status`, `size_calculated_at_unix_ms`) come from the
/// calculator; `disk_bytes`/timestamp are 0 until a size has been computed.
fn worktree_row_from_diff(
    diff: &WorktreeDiffRow,
    status: WorktreeSizeStatus,
    disk_bytes: Option<u64>,
    calculated_at_unix_ms: Option<i64>,
) -> WorktreeRow {
    WorktreeRow {
        path: diff.path.to_string_lossy().to_string(),
        branch_label: diff.branch_label.clone(),
        disk_bytes: disk_bytes.unwrap_or(0),
        changed_files: diff.changed_files,
        lines_added: diff.lines_added,
        lines_removed: diff.lines_removed,
        updated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
        stale: false,
        size_status: proto_worktree_size_status(status) as i32,
        size_calculated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
    }
}

fn map_remove_worktree_error(e: RemoveWorktreeError) -> Status {
    match e {
        RemoveWorktreeError::NotListed => {
            Status::not_found("worktree path is not in git worktree list")
        }
        RemoveWorktreeError::CannotRemovePrimary => {
            Status::failed_precondition("cannot remove primary worktree")
        }
        RemoveWorktreeError::GitFailed { message } | RemoveWorktreeError::Io(message) => {
            Status::internal(message)
        }
    }
}

fn map_clean_worktree_error(e: CleanWorktreeError) -> Status {
    match e {
        CleanWorktreeError::NotListed => {
            Status::not_found("worktree path is not in git worktree list")
        }
        CleanWorktreeError::CannotCleanPrimary => {
            Status::failed_precondition("cannot clean primary worktree")
        }
        CleanWorktreeError::GitFailed { message } | CleanWorktreeError::Io(message) => {
            Status::internal(message)
        }
    }
}

#[async_trait::async_trait]
impl WorktreeService for WorktreeServiceImpl {
    type StreamWorktreeStatsStream = MpscWorktreeStatsStream;
    type StreamReadWorktreeFileStream = MpscResultStream<WorktreeFileChunk>;

    /// Every worktree of a project, with its branch/diff summary and whatever is known of its size.
    ///
    /// `refresh` re-walks the eager stats cache; the lazy calculator's view is overlaid on top of
    /// it, because a size it has computed is newer than the cache's and a size it is still
    /// computing must report `Calculating` rather than the last number anyone saw.
    async fn list_worktrees_for_project(
        &self,
        request: Request<ListWorktreesForProjectRequest>,
    ) -> Result<Response<ListWorktreesForProjectResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let main_repo = self.resolve_main_repo(&os_user, project_id)?;

        let cache = Arc::clone(&self.worktree_stats_cache);
        let pid = project_id.to_string();
        let repo = main_repo.clone();
        let refresh = req.refresh;

        let snapshots = blocking_within(
            self.request_timeout(),
            "ListWorktreesForProject: cache read/refresh",
            move || {
                if refresh {
                    cache.refresh_stats_for_project(&pid, &repo);
                }
                Ok(cache.list_cached_stats(&pid))
            },
        )
        .await?;

        let worktrees: Vec<WorktreeRow> = snapshots
            .into_iter()
            .map(|s| {
                // Overlay the lazy calculator's view of this worktree's size: report its status and
                // (when Cached) prefer its byte count/timestamp over the stats cache's eager walk.
                let size = self.worktree_size_calculator.state(project_id, &s.path);
                let disk_bytes = match size.status {
                    WorktreeSizeStatus::Cached => size.disk_bytes.unwrap_or(s.disk_bytes),
                    _ => s.disk_bytes,
                };
                WorktreeRow {
                    path: s.path.to_string_lossy().to_string(),
                    branch_label: s.branch_label,
                    disk_bytes,
                    changed_files: s.changed_files,
                    lines_added: s.lines_added,
                    lines_removed: s.lines_removed,
                    updated_at_unix_ms: s.updated_at_unix_ms,
                    stale: s.stale,
                    size_status: proto_worktree_size_status(size.status) as i32,
                    size_calculated_at_unix_ms: size.calculated_at_unix_ms.unwrap_or(0),
                }
            })
            .collect();

        Ok(Response::new(ListWorktreesForProjectResponse { worktrees }))
    }

    async fn remove_worktree(
        &self,
        request: Request<RemoveWorktreeRequest>,
    ) -> Result<Response<RemoveWorktreeResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let worktree_path_raw = req.worktree_path.trim();
        if worktree_path_raw.is_empty() {
            return Err(Status::invalid_argument("worktree_path is required"));
        }
        let main_repo = self.resolve_main_repo(&os_user, project_id)?;
        let worktree_path = PathBuf::from(worktree_path_raw);

        // Before the checkout goes, not after: a session room measures its directory on an
        // interval, so one still hosted for this path would shell out to git in a directory that no
        // longer exists — warning at the poll rate for the life of the daemon. This RPC removes a
        // checkout by path and never learns a session id, so the registry is asked by path.
        self.session_rooms.close_for_worktree(&worktree_path);

        let repo_blocking = main_repo.clone();
        let wt_blocking = worktree_path.clone();
        let removed = blocking_within(self.request_timeout(), "RemoveWorktree", move || {
            Ok(worktrees::remove_worktree_under_repo(
                &repo_blocking,
                &wt_blocking,
            ))
        })
        .await?;

        match removed {
            Ok(()) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(RemoveWorktreeResponse {
                    ok: true,
                    message: String::new(),
                }))
            }
            Err(e) => Err(map_remove_worktree_error(e)),
        }
    }

    async fn clean_worktree(
        &self,
        request: Request<CleanWorktreeRequest>,
    ) -> Result<Response<CleanWorktreeResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let worktree_path_raw = req.worktree_path.trim();
        if worktree_path_raw.is_empty() {
            return Err(Status::invalid_argument("worktree_path is required"));
        }
        let main_repo = self.resolve_main_repo(&os_user, project_id)?;
        let worktree_path = PathBuf::from(worktree_path_raw);

        let repo_blocking = main_repo.clone();
        let wt_blocking = worktree_path.clone();
        let cleaned = blocking_within(self.request_timeout(), "CleanWorktree", move || {
            Ok(worktrees::clean_worktree_under_repo(
                &repo_blocking,
                &wt_blocking,
            ))
        })
        .await?;

        match cleaned {
            Ok(()) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(CleanWorktreeResponse {
                    ok: true,
                    message: String::new(),
                }))
            }
            Err(e) => Err(map_clean_worktree_error(e)),
        }
    }

    /// Cut the session's worktree again from the integration base its changeset recorded.
    ///
    /// The base is re-read rather than re-derived: the session was created against a particular
    /// commit, and restoring it against whatever the branch points at now would silently hand the
    /// agent a different tree from the one it was working in.
    async fn restore_session_worktree(
        &self,
        request: Request<RestoreSessionWorktreeRequest>,
    ) -> Result<Response<RestoreSessionWorktreeResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }
        tddy_core::session_lifecycle::validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let main_repo = self.resolve_main_repo(&os_user, project_id)?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions base"))?;
        let session_dir = sessions_base
            .join(tddy_core::output::SESSIONS_SUBDIR)
            .join(session_id);

        let repo_blocking = main_repo.clone();
        let session_dir_blocking = session_dir.clone();
        let restored = blocking_within(
            self.request_timeout(),
            "RestoreSessionWorktree",
            move || {
                Ok(
                    tddy_core::resolve_persisted_worktree_integration_base_for_session(
                        &session_dir_blocking,
                        &repo_blocking,
                    )
                    .and_then(|base_ref| {
                        tddy_core::setup_worktree_for_session_with_integration_base(
                            &repo_blocking,
                            &session_dir_blocking,
                            &base_ref,
                        )
                    }),
                )
            },
        )
        .await?;

        match restored {
            Ok(path) => {
                self.worktree_stats_cache.invalidate_project(project_id);
                Ok(Response::new(RestoreSessionWorktreeResponse {
                    ok: true,
                    message: String::new(),
                    worktree_path: path.to_string_lossy().into_owned(),
                }))
            }
            Err(e) => Err(Status::internal(e)),
        }
    }

    async fn list_worktree_directory(
        &self,
        request: Request<ListWorktreeDirectoryRequest>,
    ) -> Result<Response<ListWorktreeDirectoryResponse>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let entries = blocking_within(self.request_timeout(), "ListWorktreeDirectory", move || {
            Ok(crate::worktree_files::list_worktree_directory_entries(
                &worktree_root,
                &rel_path,
            ))
        })
        .await??;

        let entries = entries
            .into_iter()
            .map(|e| WorktreeDirEntry {
                name: e.name,
                is_dir: e.is_dir,
                size_bytes: e.size_bytes,
            })
            .collect();
        Ok(Response::new(ListWorktreeDirectoryResponse { entries }))
    }

    async fn read_worktree_file(
        &self,
        request: Request<ReadWorktreeFileRequest>,
    ) -> Result<Response<ReadWorktreeFileResponse>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let content = blocking_within(self.request_timeout(), "ReadWorktreeFile", move || {
            Ok(crate::worktree_files::read_worktree_file_utf8(
                &worktree_root,
                &rel_path,
            ))
        })
        .await??;

        Ok(Response::new(ReadWorktreeFileResponse {
            content_utf8: content.content_utf8,
            truncated: content.truncated,
            byte_size: content.byte_size,
        }))
    }

    /// The byte-exact streaming read — AC15-AC20 of `docs/ft/daemon/session-worktree-sync.md`.
    ///
    /// Same request message, same addressing and the same `resolve_listed_worktree` gate as the
    /// unary `read_worktree_file`; what differs is what comes back. No UTF-8 decoding exists on this
    /// path to fail, and the 1 MiB truncation the unary read applies is gone — the bound is
    /// `max_attachment_bytes` and an over-cap file is **refused before the first frame** rather than
    /// shortened, because a caller cannot tell a truncated file from a whole one once the frames
    /// have started.
    async fn stream_read_worktree_file(
        &self,
        request: Request<ReadWorktreeFileRequest>,
    ) -> Result<Response<Self::StreamReadWorktreeFileStream>, Status> {
        let req = request.into_inner();
        let worktree_root =
            self.resolve_listed_worktree(&req.session_token, &req.project_id, &req.worktree_path)?;

        let rel_path = req.rel_path.clone();
        let max_bytes = self.config.max_attachment_bytes;
        // The size refusal and the read are filesystem and git work, so they run off the async
        // runtime exactly as the unary read's do. Both can fail the call outright, which is why they
        // happen here rather than inside the stream: a refusal that arrived as a stream item would
        // have to be told apart from a mid-stream read error.
        let bytes = blocking_within(
            self.request_timeout(),
            "StreamReadWorktreeFile",
            move || {
                Ok(crate::worktree_files::read_worktree_file_bytes(
                    &worktree_root,
                    &rel_path,
                    max_bytes,
                ))
            },
        )
        .await??;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<WorktreeFileChunk, Status>>();
        for frame in worktree_file_frames(&bytes) {
            // The whole file is already in memory and the channel is unbounded, so this cannot
            // block; a send only fails once the client has gone, and then there is nothing left to
            // send it to.
            if tx.send(Ok(frame)).is_err() {
                break;
            }
        }
        Ok(Response::new(MpscResultStream::from(rx)))
    }

    /// Stream per-worktree disk-size status for a project.
    ///
    /// Emits one snapshot event carrying every worktree's current size state, then lazily enqueues
    /// size calculations (all worktrees when `recalculate_all`, otherwise only those never sized)
    /// and forwards each `Calculating` → `Cached` transition as a single-row `updated` event. The
    /// forwarding task ends when the client drops the stream.
    async fn stream_worktree_stats(
        &self,
        request: Request<StreamWorktreeStatsRequest>,
    ) -> Result<Response<Self::StreamWorktreeStatsStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let main_repo = self.resolve_main_repo(&os_user, project_id)?;

        // Discover worktrees + branch/diff off the async runtime, without the size walk.
        let repo = main_repo.clone();
        let diff_rows = blocking_within(
            self.request_timeout(),
            "StreamWorktreeStats: git worktree list + diff",
            move || Ok(worktrees::list_worktree_diff_rows(&repo)),
        )
        .await?;

        // Branch/diff lookup keyed by path, so each later size update rebuilds a full row.
        let diff_by_path: HashMap<PathBuf, WorktreeDiffRow> = diff_rows
            .iter()
            .map(|r| (r.path.clone(), r.clone()))
            .collect();

        let calculator = Arc::clone(&self.worktree_size_calculator);

        // Subscribe before enqueuing so no Calculating/Cached transition is missed.
        let mut updates = calculator.subscribe(project_id);

        // Snapshot: current size state per worktree (before any enqueue triggered below).
        let snapshot: Vec<WorktreeRow> = diff_rows
            .iter()
            .map(|r| {
                let state = calculator.state(project_id, &r.path);
                worktree_row_from_diff(
                    r,
                    state.status,
                    state.disk_bytes,
                    state.calculated_at_unix_ms,
                )
            })
            .collect();

        // Lazily enqueue: all worktrees on recalculate_all, otherwise only the never-sized ones.
        for r in &diff_rows {
            let status = calculator.state(project_id, &r.path).status;
            if req.recalculate_all || status == WorktreeSizeStatus::None {
                calculator.enqueue(project_id, &r.path).await;
            }
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<WorktreeStatsEvent>();
        if tx
            .send(WorktreeStatsEvent {
                snapshot,
                updated: None,
            })
            .is_err()
        {
            return Ok(Response::new(MpscWorktreeStatsStream::from(rx)));
        }

        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match updates.recv().await {
                    Ok(update) => {
                        // Only forward worktrees present in the snapshot; a worktree created after
                        // this subscribe is picked up by a fresh StreamWorktreeStats call.
                        let Some(diff) = diff_by_path.get(&update.path) else {
                            continue;
                        };
                        let row = worktree_row_from_diff(
                            diff,
                            update.status,
                            update.disk_bytes,
                            update.calculated_at_unix_ms,
                        );
                        if tx
                            .send(WorktreeStatsEvent {
                                snapshot: Vec::new(),
                                updated: Some(row),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });

        Ok(Response::new(MpscWorktreeStatsStream::from(rx)))
    }

    /// (Re)trigger the on-disk size calculation for a single worktree.
    ///
    /// Membership-gated on `git worktree list` exactly as `RemoveWorktree` is, and for the same
    /// reason: the path is free text, and a size walk is a recursive read of whatever it names. The
    /// result surfaces on any `StreamWorktreeStats` subscriber and in `ListWorktreesForProject`.
    async fn calculate_worktree_size(
        &self,
        request: Request<CalculateWorktreeSizeRequest>,
    ) -> Result<Response<CalculateWorktreeSizeResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        let os_user = self.authorize(&req.session_token)?;
        let project_id = req.project_id.trim();
        let worktree_path_raw = req.worktree_path.trim();
        if worktree_path_raw.is_empty() {
            return Err(Status::invalid_argument("worktree_path is required"));
        }
        let main_repo = self.resolve_main_repo(&os_user, project_id)?;
        let worktree_path = PathBuf::from(worktree_path_raw);

        // Membership-gate on git's own worktree list (mirrors RemoveWorktree::NotListed -> NotFound).
        let repo_check = main_repo.clone();
        let wt_check = worktree_path.clone();
        let listed = blocking_within(
            self.request_timeout(),
            "CalculateWorktreeSize: worktree membership check",
            move || Ok(worktrees::worktree_path_is_listed(&repo_check, &wt_check)),
        )
        .await?;
        if !listed {
            return Err(Status::not_found(
                "worktree path is not in git worktree list",
            ));
        }

        self.worktree_size_calculator
            .enqueue(project_id, &worktree_path)
            .await;

        Ok(Response::new(CalculateWorktreeSizeResponse {
            ok: true,
            message: String::new(),
        }))
    }
}
