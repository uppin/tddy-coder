//! Git worktree listing, stats cache, path policy, and removal helpers (Worktrees manager PRD).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Semaphore};

/// One row from `git worktree list` after parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeListRow {
    pub path: PathBuf,
    /// Branch name, or a clear marker for detached HEAD (e.g. `(detached)`).
    pub branch_label: String,
    pub lock_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeStatSnapshot {
    pub path: PathBuf,
    pub branch_label: String,
    pub disk_bytes: u64,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub updated_at_unix_ms: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreePathError {
    OutsideRepoRoot {
        repo_root: PathBuf,
        candidate: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveWorktreeError {
    GitFailed { message: String },
    NotListed,
    CannotRemovePrimary,
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanWorktreeError {
    GitFailed { message: String },
    NotListed,
    CannotCleanPrimary,
    Io(String),
}

/// Parse `git worktree list` stdout into structured rows (fixtures: see acceptance tests).
///
/// Baseline format matches default `git worktree list` (non-porcelain): path, abbreviated
/// commit, then `[branch]` or `(detached HEAD)`. Detached rows are normalized to branch
/// label `(detached)` for UI consistency.
pub fn parse_git_worktree_list(stdout: &str) -> Vec<WorktreeListRow> {
    debug!("parse_git_worktree_list: {} bytes of stdout", stdout.len());
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if let Some(row) = parse_git_worktree_list_line(line) {
            out.push(row);
        }
    }
    info!(
        "parse_git_worktree_list: parsed {} worktree row(s)",
        out.len()
    );
    out
}

fn parse_git_worktree_list_line(line: &str) -> Option<WorktreeListRow> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let (rest, branch_label, lock_path) = if let Some(detached) = line.find("(detached HEAD)") {
        let rest = line[..detached].trim_end();
        (rest, "(detached)".to_string(), None)
    } else if let Some(open) = line.rfind(" [") {
        let close = line.rfind(']')?;
        let branch = line[open + 2..close].to_string();
        let rest = line[..open].trim_end();
        (rest, branch, None)
    } else {
        warn!(
            "parse_git_worktree_list_line: unrecognized line: {:?}",
            line
        );
        return None;
    };

    let rest = rest.trim_end();
    let commit_end = rest.rfind(char::is_whitespace)?;
    let path_part = rest[..commit_end].trim_end();
    if path_part.is_empty() {
        return None;
    }

    Some(WorktreeListRow {
        path: PathBuf::from(path_part),
        branch_label,
        lock_path,
    })
}

/// Root directory for persisted per-project worktree stats (`{base}/projects/`).
pub fn projects_stats_cache_root(base: &Path) -> PathBuf {
    debug!("projects_stats_cache_root: base={:?}", base);
    base.join("projects")
}

/// Lexical path normalization (resolves `.` and `..`) without filesystem access.
/// Used so policy checks work when paths do not exist yet and to detect `..` escapes.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    let mut has_root = false;
    for c in path.components() {
        match c {
            Component::RootDir => {
                has_root = true;
                stack.clear();
            }
            Component::Prefix(p) => {
                stack.push(p.as_os_str().to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            Component::Normal(s) => stack.push(s.to_owned()),
        }
    }
    let mut out = PathBuf::new();
    if has_root {
        out.push(Component::RootDir);
    }
    for s in stack {
        out.push(s);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Ensure `candidate` resolves to a path under `repo_root` (lexical prefix policy).
pub fn validate_worktree_path_within_repo_root(
    repo_root: &Path,
    candidate: &Path,
) -> Result<PathBuf, WorktreePathError> {
    debug!(
        "validate_worktree_path_within_repo_root: repo={:?} candidate={:?}",
        repo_root, candidate
    );
    let resolved = if candidate.is_absolute() {
        lexical_normalize(candidate)
    } else {
        lexical_normalize(&repo_root.join(candidate))
    };
    let repo_norm = lexical_normalize(repo_root);
    if !resolved.starts_with(&repo_norm) {
        warn!(
            "validate_worktree_path_within_repo_root: rejected {:?} (not under {:?})",
            resolved, repo_norm
        );
        return Err(WorktreePathError::OutsideRepoRoot {
            repo_root: repo_root.to_path_buf(),
            candidate: candidate.to_path_buf(),
        });
    }
    info!("validate_worktree_path_within_repo_root: ok {:?}", resolved);
    Ok(resolved)
}

#[derive(Serialize, Deserialize)]
struct WorktreeStatsCacheFile {
    snapshots: Vec<WorktreeStatSnapshot>,
}

fn git_worktree_list_stdout(main_repo: &Path) -> String {
    match Command::new("git")
        .current_dir(main_repo)
        .args(["worktree", "list"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            warn!(
                "git_worktree_list_stdout: git worktree list failed: {:?}",
                o.status
            );
            String::new()
        }
        Err(e) => {
            warn!("git_worktree_list_stdout: git worktree list: {}", e);
            String::new()
        }
    }
}

fn build_worktree_stat_snapshots(rows: &[WorktreeListRow]) -> Vec<WorktreeStatSnapshot> {
    let mut snapshots = Vec::with_capacity(rows.len());
    for row in rows {
        let disk_bytes = directory_size_bytes_best_effort(&row.path);
        let (changed_files, lines_added, lines_removed) = git_diff_numstat_summary(&row.path);
        let updated_at_unix_ms = chrono::Utc::now().timestamp_millis();
        snapshots.push(WorktreeStatSnapshot {
            path: row.path.clone(),
            branch_label: row.branch_label.clone(),
            disk_bytes,
            changed_files,
            lines_added,
            lines_removed,
            updated_at_unix_ms,
            stale: false,
        });
    }
    snapshots
}

fn write_worktree_stats_cache_file(path: &Path, snapshots: Vec<WorktreeStatSnapshot>) {
    let payload = WorktreeStatsCacheFile { snapshots };
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => {
            if let Err(e) = fs::write(path, json) {
                warn!("write_worktree_stats_cache_file: write {:?}: {}", path, e);
            } else {
                debug!("write_worktree_stats_cache_file: wrote {:?}", path);
            }
        }
        Err(e) => warn!("write_worktree_stats_cache_file: serialize: {}", e),
    }
}

/// Tracks expensive git/stat work; used by acceptance tests to ensure list RPC does not re-diff.
pub struct WorktreeStatsCache {
    root: PathBuf,
    /// Simulates `git diff` / stat work invoked during refresh.
    pub test_git_diff_invocations: AtomicU64,
}

impl WorktreeStatsCache {
    pub fn new(root: PathBuf) -> Self {
        info!("WorktreeStatsCache::new root={:?}", root);
        Self {
            root,
            test_git_diff_invocations: AtomicU64::new(0),
        }
    }

    pub fn cache_root(&self) -> &Path {
        &self.root
    }

    fn project_cache_dir(&self, project_id: &str) -> PathBuf {
        let safe = project_id.replace(['/', '\\', ':'], "_");
        self.root.join(safe)
    }

    fn cache_file_path(&self, project_id: &str) -> PathBuf {
        self.project_cache_dir(project_id)
            .join("worktree_stats.json")
    }

    /// Background / explicit refresh: runs `git worktree list` and per-worktree diff/size once per call, persists.
    pub fn refresh_stats_for_project(&self, project_id: &str, main_repo: &Path) {
        debug!(
            "refresh_stats_for_project: project_id={} main_repo={:?}",
            project_id, main_repo
        );
        let dir = self.project_cache_dir(project_id);
        if let Err(e) = fs::create_dir_all(&dir) {
            warn!("refresh_stats_for_project: create_dir_all {:?}: {}", dir, e);
        }

        let list_out = git_worktree_list_stdout(main_repo);

        let rows = parse_git_worktree_list(&list_out);
        info!(
            "refresh_stats_for_project: {} worktree row(s) for project {}",
            rows.len(),
            project_id
        );

        let snapshots = build_worktree_stat_snapshots(&rows);

        self.test_git_diff_invocations
            .fetch_add(1, Ordering::SeqCst);

        let path = self.cache_file_path(project_id);
        write_worktree_stats_cache_file(&path, snapshots);
    }

    /// List path used by RPC: must serve last snapshot without re-running diff each time.
    pub fn list_cached_stats(&self, project_id: &str) -> Vec<WorktreeStatSnapshot> {
        debug!("list_cached_stats: project_id={}", project_id);
        let path = self.cache_file_path(project_id);
        let data = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("list_cached_stats: no cache file yet {:?}", path);
                return Vec::new();
            }
            Err(e) => {
                warn!("list_cached_stats: read {:?}: {}", path, e);
                return Vec::new();
            }
        };
        match serde_json::from_str::<WorktreeStatsCacheFile>(&data) {
            Ok(f) => {
                info!(
                    "list_cached_stats: served {} snapshot(s) from disk for {}",
                    f.snapshots.len(),
                    project_id
                );
                f.snapshots
            }
            Err(e) => {
                warn!("list_cached_stats: parse {:?}: {}", path, e);
                Vec::new()
            }
        }
    }

    pub fn invalidate_project(&self, project_id: &str) {
        debug!("invalidate_project: {}", project_id);
        let path = self.cache_file_path(project_id);
        match fs::remove_file(&path) {
            Ok(()) => info!("invalidate_project: removed {:?}", path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                debug!("invalidate_project: no file {:?}", path)
            }
            Err(e) => warn!("invalidate_project: remove {:?}: {}", path, e),
        }
    }
}

/// Lifecycle of a worktree's on-disk size calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeSizeStatus {
    /// Never calculated and no walk currently in flight.
    None,
    /// A size walk is in progress (queued or running).
    Calculating,
    /// A size has been computed (in memory or persisted) and is available.
    Cached,
}

/// The current known size state of a single worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSizeState {
    pub status: WorktreeSizeStatus,
    pub disk_bytes: Option<u64>,
    pub calculated_at_unix_ms: Option<i64>,
}

impl WorktreeSizeState {
    fn none() -> Self {
        Self {
            status: WorktreeSizeStatus::None,
            disk_bytes: None,
            calculated_at_unix_ms: None,
        }
    }
}

/// A published state transition for one worktree, delivered over the per-project broadcast channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSizeUpdate {
    pub path: PathBuf,
    pub status: WorktreeSizeStatus,
    pub disk_bytes: Option<u64>,
    pub calculated_at_unix_ms: Option<i64>,
}

/// One persisted worktree size entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedWorktreeSize {
    disk_bytes: u64,
    calculated_at_unix_ms: i64,
}

/// On-disk shape of `{root}/{project}/worktree_sizes.json` (map of worktree path -> size).
#[derive(Debug, Default, Serialize, Deserialize)]
struct WorktreeSizesCacheFile {
    sizes: HashMap<String, PersistedWorktreeSize>,
}

/// In-memory state shared between the calculator handle and its spawned walk tasks.
#[derive(Default)]
struct WorktreeSizeInner {
    /// (project_id, worktree_path) -> current state.
    states: HashMap<(String, PathBuf), WorktreeSizeState>,
    /// project_id -> broadcast sender for that project's updates.
    senders: HashMap<String, broadcast::Sender<WorktreeSizeUpdate>>,
}

/// Capacity of each project's broadcast channel; ample so a slow subscriber created before a walk
/// still observes the full `Calculating` -> `Cached` sequence.
const SIZE_UPDATE_CHANNEL_CAPACITY: usize = 1024;

/// Lazy, semaphore-bounded per-worktree disk-size calculator.
///
/// Sizes are computed off the async runtime via `spawn_blocking`, capped by a central semaphore so
/// at most `permits` walks run at once. Results are broadcast per project and persisted separately
/// from [`WorktreeStatsCache`] so a fresh calculator over the same `root` serves cached sizes
/// without re-walking.
pub struct WorktreeSizeCalculator {
    root: PathBuf,
    sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync>,
    semaphore: Arc<Semaphore>,
    inner: Arc<Mutex<WorktreeSizeInner>>,
    /// Serializes read-modify-write of each project's persisted sizes file.
    persist_lock: Arc<Mutex<()>>,
}

impl WorktreeSizeCalculator {
    /// Production constructor: uses the real best-effort directory walk as the sizer.
    pub fn new(root: PathBuf, permits: usize) -> Self {
        let sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync> =
            Arc::new(directory_size_bytes_best_effort);
        Self::with_sizer(root, permits, sizer)
    }

    /// Constructor with an injectable sizer (used by tests to observe concurrency and gate walks).
    pub fn with_sizer(
        root: PathBuf,
        permits: usize,
        sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync>,
    ) -> Self {
        info!(
            "WorktreeSizeCalculator::with_sizer root={:?} permits={}",
            root, permits
        );
        Self {
            root,
            sizer,
            semaphore: Arc::new(Semaphore::new(permits)),
            inner: Arc::new(Mutex::new(WorktreeSizeInner::default())),
            persist_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Current size state of one worktree.
    ///
    /// Reads in-memory state first; if unknown there, lazily falls back to the persisted file so a
    /// freshly-constructed calculator reports `Cached` without triggering a walk. Reports `None`
    /// only when nothing has ever been computed and no walk is in flight.
    pub fn state(&self, project_id: &str, path: &Path) -> WorktreeSizeState {
        let key = (project_id.to_string(), path.to_path_buf());
        {
            let inner = self.inner.lock().unwrap();
            if let Some(state) = inner.states.get(&key) {
                return state.clone();
            }
        }
        if let Some(persisted) = self.load_persisted_entry(project_id, path) {
            let state = WorktreeSizeState {
                status: WorktreeSizeStatus::Cached,
                disk_bytes: Some(persisted.disk_bytes),
                calculated_at_unix_ms: Some(persisted.calculated_at_unix_ms),
            };
            let mut inner = self.inner.lock().unwrap();
            return inner.states.entry(key).or_insert(state).clone();
        }
        WorktreeSizeState::none()
    }

    /// Subscribe to a project's stream of size updates. Creates the channel on first use.
    pub fn subscribe(&self, project_id: &str) -> broadcast::Receiver<WorktreeSizeUpdate> {
        let mut inner = self.inner.lock().unwrap();
        Self::sender_for(&mut inner, project_id).subscribe()
    }

    /// Start (or de-duplicate) a size calculation for one worktree.
    ///
    /// Synchronously marks the worktree `Calculating` and broadcasts that update, then spawns a task
    /// that acquires one semaphore permit, runs the sizer under `spawn_blocking`, and on completion
    /// marks it `Cached`, broadcasts, and persists. Re-enqueuing a worktree that is already
    /// `Calculating` is a no-op (the in-flight walk is reused).
    pub async fn enqueue(&self, project_id: &str, path: &Path) {
        let key = (project_id.to_string(), path.to_path_buf());
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(state) = inner.states.get(&key) {
                if state.status == WorktreeSizeStatus::Calculating {
                    debug!(
                        "WorktreeSizeCalculator::enqueue: {:?} already calculating; reusing walk",
                        path
                    );
                    return;
                }
            }
            inner.states.insert(
                key.clone(),
                WorktreeSizeState {
                    status: WorktreeSizeStatus::Calculating,
                    disk_bytes: None,
                    calculated_at_unix_ms: None,
                },
            );
            let update = WorktreeSizeUpdate {
                path: path.to_path_buf(),
                status: WorktreeSizeStatus::Calculating,
                disk_bytes: None,
                calculated_at_unix_ms: None,
            };
            let _ = Self::sender_for(&mut inner, project_id).send(update);
        }

        let sizer = Arc::clone(&self.sizer);
        let semaphore = Arc::clone(&self.semaphore);
        let inner = Arc::clone(&self.inner);
        let persist_lock = Arc::clone(&self.persist_lock);
        let root = self.root.clone();
        let project_id = project_id.to_string();
        let path = path.to_path_buf();

        tokio::spawn(async move {
            let _permit = match semaphore.acquire().await {
                Ok(permit) => permit,
                Err(e) => {
                    warn!("WorktreeSizeCalculator: semaphore closed: {}", e);
                    return;
                }
            };

            let sizer_path = path.clone();
            let bytes = match tokio::task::spawn_blocking(move || sizer(&sizer_path)).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    warn!(
                        "WorktreeSizeCalculator: size walk for {:?} panicked: {}",
                        path, e
                    );
                    return;
                }
            };
            let calculated_at_unix_ms = chrono::Utc::now().timestamp_millis();

            {
                let mut guard = inner.lock().unwrap();
                guard.states.insert(
                    (project_id.clone(), path.clone()),
                    WorktreeSizeState {
                        status: WorktreeSizeStatus::Cached,
                        disk_bytes: Some(bytes),
                        calculated_at_unix_ms: Some(calculated_at_unix_ms),
                    },
                );
                if let Some(sender) = guard.senders.get(&project_id) {
                    let _ = sender.send(WorktreeSizeUpdate {
                        path: path.clone(),
                        status: WorktreeSizeStatus::Cached,
                        disk_bytes: Some(bytes),
                        calculated_at_unix_ms: Some(calculated_at_unix_ms),
                    });
                }
            }

            persist_worktree_size(
                &root,
                &persist_lock,
                &project_id,
                &path,
                bytes,
                calculated_at_unix_ms,
            );
            debug!(
                "WorktreeSizeCalculator: cached {:?} = {} bytes",
                path, bytes
            );
        });
    }

    /// Snapshot of every known worktree's state in a project (the stream's first frame). Merges
    /// persisted entries with the (authoritative) in-memory states.
    pub fn snapshot(&self, project_id: &str) -> Vec<WorktreeSizeUpdate> {
        let mut merged: HashMap<PathBuf, WorktreeSizeState> = HashMap::new();

        let cache = read_worktree_sizes_file(&project_sizes_file(&self.root, project_id));
        for (path, persisted) in cache.sizes {
            merged.insert(
                PathBuf::from(path),
                WorktreeSizeState {
                    status: WorktreeSizeStatus::Cached,
                    disk_bytes: Some(persisted.disk_bytes),
                    calculated_at_unix_ms: Some(persisted.calculated_at_unix_ms),
                },
            );
        }
        {
            let inner = self.inner.lock().unwrap();
            for ((proj, path), state) in inner.states.iter() {
                if proj == project_id {
                    merged.insert(path.clone(), state.clone());
                }
            }
        }

        merged
            .into_iter()
            .map(|(path, state)| WorktreeSizeUpdate {
                path,
                status: state.status,
                disk_bytes: state.disk_bytes,
                calculated_at_unix_ms: state.calculated_at_unix_ms,
            })
            .collect()
    }

    fn sender_for<'a>(
        inner: &'a mut WorktreeSizeInner,
        project_id: &str,
    ) -> &'a broadcast::Sender<WorktreeSizeUpdate> {
        inner
            .senders
            .entry(project_id.to_string())
            .or_insert_with(|| broadcast::channel(SIZE_UPDATE_CHANNEL_CAPACITY).0)
    }

    fn load_persisted_entry(&self, project_id: &str, path: &Path) -> Option<PersistedWorktreeSize> {
        let file = project_sizes_file(&self.root, project_id);
        let cache = read_worktree_sizes_file(&file);
        cache.sizes.get(&worktree_size_key(path)).cloned()
    }
}

/// Persisted-sizes file path for a project: `{root}/{sanitized_project}/worktree_sizes.json`.
/// Sanitization matches [`WorktreeStatsCache`] so the two caches share a per-project directory
/// while keeping distinct files.
fn project_sizes_file(root: &Path, project_id: &str) -> PathBuf {
    let safe = project_id.replace(['/', '\\', ':'], "_");
    root.join(safe).join("worktree_sizes.json")
}

fn worktree_size_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn read_worktree_sizes_file(file: &Path) -> WorktreeSizesCacheFile {
    match fs::read_to_string(file) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_else(|e| {
            warn!("read_worktree_sizes_file: parse {:?}: {}", file, e);
            WorktreeSizesCacheFile::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => WorktreeSizesCacheFile::default(),
        Err(e) => {
            warn!("read_worktree_sizes_file: read {:?}: {}", file, e);
            WorktreeSizesCacheFile::default()
        }
    }
}

fn persist_worktree_size(
    root: &Path,
    persist_lock: &Mutex<()>,
    project_id: &str,
    path: &Path,
    disk_bytes: u64,
    calculated_at_unix_ms: i64,
) {
    let _guard = persist_lock.lock().unwrap();
    let file = project_sizes_file(root, project_id);
    if let Some(dir) = file.parent() {
        if let Err(e) = fs::create_dir_all(dir) {
            warn!("persist_worktree_size: create_dir_all {:?}: {}", dir, e);
            return;
        }
    }
    let mut cache = read_worktree_sizes_file(&file);
    cache.sizes.insert(
        worktree_size_key(path),
        PersistedWorktreeSize {
            disk_bytes,
            calculated_at_unix_ms,
        },
    );
    match serde_json::to_string_pretty(&cache) {
        Ok(json) => {
            if let Err(e) = fs::write(&file, json) {
                warn!("persist_worktree_size: write {:?}: {}", file, e);
            }
        }
        Err(e) => warn!("persist_worktree_size: serialize: {}", e),
    }
}

fn directory_size_bytes_best_effort(path: &Path) -> u64 {
    let walk_root = path.to_path_buf();
    let mut total = 0u64;
    if let Ok(m) = fs::metadata(path) {
        if m.is_file() {
            return m.len();
        }
    }
    let mut stack = vec![walk_root.clone()];
    while let Some(p) = stack.pop() {
        let read_dir = match fs::read_dir(&p) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for ent in read_dir.flatten() {
            let meta = match ent.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                if paths_equal(&p, &walk_root)
                    && ent.file_name().as_os_str() == std::ffi::OsStr::new(".worktrees")
                {
                    continue;
                }
                stack.push(ent.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

/// Returns (changed_files, lines_added, lines_removed) from `git diff --numstat` in `cwd`.
fn git_diff_numstat_summary(cwd: &Path) -> (u32, i64, i64) {
    let out = match Command::new("git")
        .current_dir(cwd)
        .args(["diff", "--numstat", "HEAD"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return (0, 0, 0),
    };
    let mut files = 0u32;
    let mut added = 0i64;
    let mut removed = 0i64;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let a = parts.next();
        let b = parts.next();
        if a == Some("-") && b == Some("-") {
            files += 1;
            continue;
        }
        if let (Some(a), Some(b)) = (a, b) {
            if let (Ok(ai), Ok(bi)) = (a.parse::<i64>(), b.parse::<i64>()) {
                files += 1;
                added += ai;
                removed += bi;
            }
        }
    }
    (files, added, removed)
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    lexical_normalize(a) == lexical_normalize(b)
}

/// True when `worktree_path` matches one of the parsed `git worktree list` rows.
fn worktree_path_in_rows(rows: &[WorktreeListRow], worktree_path: &Path) -> bool {
    rows.iter().any(|r| paths_equal(&r.path, worktree_path))
}

/// A worktree's identity plus its git diff summary, but **without** the on-disk size walk.
///
/// The streaming size RPC ([`crate::connection_service`]) builds rows from this and fills each
/// worktree's disk size lazily via [`WorktreeSizeCalculator`], so it never pays for the eager
/// directory walk that [`WorktreeStatsCache::refresh_stats_for_project`] performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeDiffRow {
    pub path: PathBuf,
    pub branch_label: String,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
}

/// List `main_repo`'s worktrees with each one's `git diff --numstat` summary, skipping disk-size
/// calculation. Reuses the same `git worktree list` parse and diff summary as the stats refresh.
pub fn list_worktree_diff_rows(main_repo: &Path) -> Vec<WorktreeDiffRow> {
    let stdout = git_worktree_list_stdout(main_repo);
    let rows = parse_git_worktree_list(&stdout);
    info!(
        "list_worktree_diff_rows: {} worktree row(s) for repo {:?}",
        rows.len(),
        main_repo
    );
    rows.into_iter()
        .map(|row| {
            let (changed_files, lines_added, lines_removed) = git_diff_numstat_summary(&row.path);
            WorktreeDiffRow {
                path: row.path,
                branch_label: row.branch_label,
                changed_files,
                lines_added,
                lines_removed,
            }
        })
        .collect()
}

/// True when `worktree_path` appears in `git worktree list` for `repo_root`. Used to gate
/// filesystem access to a worktree behind git's own membership view.
pub fn worktree_path_is_listed(repo_root: &Path, worktree_path: &Path) -> bool {
    let stdout = git_worktree_list_stdout(repo_root);
    let rows = parse_git_worktree_list(&stdout);
    worktree_path_in_rows(&rows, worktree_path)
}

/// Remove a secondary worktree via `git worktree remove` as the project OS user.
/// The path must appear in `git worktree list` for `repo_root` and must not be the primary
/// (first-listed) worktree. Worktrees may live outside the main repo directory (sibling paths).
pub fn remove_worktree_under_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), RemoveWorktreeError> {
    info!(
        "remove_worktree_under_repo: repo_root={:?} worktree_path={:?}",
        repo_root, worktree_path
    );
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list"])
        .output()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(RemoveWorktreeError::GitFailed {
            message: format!("git worktree list failed: {:?}", out.status),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rows = parse_git_worktree_list(&stdout);
    let listed = worktree_path_in_rows(&rows, worktree_path);
    if !listed {
        warn!(
            "remove_worktree_under_repo: path not in worktree list {:?}",
            worktree_path
        );
        return Err(RemoveWorktreeError::NotListed);
    }
    if let Some(first) = rows.first() {
        if paths_equal(&first.path, worktree_path) {
            warn!("remove_worktree_under_repo: refusing to remove primary worktree");
            return Err(RemoveWorktreeError::CannotRemovePrimary);
        }
    }

    let wt_str = worktree_path
        .to_str()
        .ok_or_else(|| RemoveWorktreeError::Io("worktree path is not valid UTF-8".to_string()))?;

    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "remove", wt_str])
        .status()
        .map_err(|e| RemoveWorktreeError::Io(e.to_string()))?;

    if !status.success() {
        let msg = format!("git worktree remove failed: {:?}", status);
        warn!("remove_worktree_under_repo: {}", msg);
        return Err(RemoveWorktreeError::GitFailed { message: msg });
    }
    info!("remove_worktree_under_repo: removed {:?}", worktree_path);
    Ok(())
}

/// Clear a secondary worktree in place with `git clean -fdx` (drops untracked + ignored files,
/// e.g. build output, to reclaim disk without removing the worktree). The path must appear in
/// `git worktree list` for `repo_root` and must not be the primary (first-listed) worktree.
pub fn clean_worktree_under_repo(
    repo_root: &Path,
    worktree_path: &Path,
) -> Result<(), CleanWorktreeError> {
    info!(
        "clean_worktree_under_repo: repo_root={:?} worktree_path={:?}",
        repo_root, worktree_path
    );
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "list"])
        .output()
        .map_err(|e| CleanWorktreeError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(CleanWorktreeError::GitFailed {
            message: format!("git worktree list failed: {:?}", out.status),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rows = parse_git_worktree_list(&stdout);
    let listed = worktree_path_in_rows(&rows, worktree_path);
    if !listed {
        warn!(
            "clean_worktree_under_repo: path not in worktree list {:?}",
            worktree_path
        );
        return Err(CleanWorktreeError::NotListed);
    }
    if let Some(first) = rows.first() {
        if paths_equal(&first.path, worktree_path) {
            warn!("clean_worktree_under_repo: refusing to clean primary worktree");
            return Err(CleanWorktreeError::CannotCleanPrimary);
        }
    }

    let status = Command::new("git")
        .current_dir(worktree_path)
        .args(["clean", "-fdx"])
        .status()
        .map_err(|e| CleanWorktreeError::Io(e.to_string()))?;

    if !status.success() {
        let msg = format!("git clean -fdx failed: {:?}", status);
        warn!("clean_worktree_under_repo: {}", msg);
        return Err(CleanWorktreeError::GitFailed { message: msg });
    }
    info!("clean_worktree_under_repo: cleaned {:?}", worktree_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BUG: When the main repo has linked worktrees under `.worktrees/` (a subdirectory of the
    /// main checkout), `directory_size_bytes_best_effort` recurses into those nested worktree
    /// directories, inflating the main worktree's `disk_bytes` with bytes that are separately
    /// reported under each secondary worktree row.
    ///
    /// This test creates a realistic directory layout:
    ///   main_repo/
    ///     README.md          (100 bytes)
    ///     src/lib.rs         (200 bytes)
    ///     .worktrees/
    ///       wt1/
    ///         README.md      (100 bytes)
    ///         feature.rs     (500 bytes)
    ///
    /// The main worktree's own files total 300 bytes. But the current implementation reports
    /// 300 + 600 = 900, because it walks into `.worktrees/wt1/`.
    #[test]
    fn main_worktree_size_excludes_nested_worktree_directories() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main_repo = tmp.path().join("repo");
        let wt_dir = main_repo.join(".worktrees").join("wt1");
        fs::create_dir_all(main_repo.join("src")).unwrap();
        fs::create_dir_all(&wt_dir).unwrap();

        let main_readme = vec![b'A'; 100];
        let main_lib = vec![b'B'; 200];
        let wt_readme = vec![b'C'; 100];
        let wt_feature = vec![b'D'; 500];

        fs::write(main_repo.join("README.md"), &main_readme).unwrap();
        fs::write(main_repo.join("src").join("lib.rs"), &main_lib).unwrap();
        fs::write(wt_dir.join("README.md"), &wt_readme).unwrap();
        fs::write(wt_dir.join("feature.rs"), &wt_feature).unwrap();

        let main_own_bytes: u64 = 100 + 200; // README.md + src/lib.rs
        let wt_bytes: u64 = 100 + 500; // wt1/README.md + wt1/feature.rs

        let reported = directory_size_bytes_best_effort(&main_repo);

        // BUG: currently reports main_own_bytes + wt_bytes (900) instead of main_own_bytes (300).
        assert_eq!(
            reported, main_own_bytes,
            "main worktree size must exclude nested .worktrees/ directory; \
             got {reported} (includes {wt_bytes} bytes from nested worktree)"
        );
    }

    /// Acceptance: parser maps branch and detached HEAD rows from a fixed fixture.
    #[test]
    fn worktree_list_parsing_handles_detached_and_branch_rows() {
        let fixture = r#"/tmp/demo-main                 abcd123 [main]
/tmp/demo-main/.worktrees/wt1  efgh456 [feature-x]
/tmp/demo-main/.worktrees/wt2  1111111 (detached HEAD)
"#;
        let rows = parse_git_worktree_list(fixture);
        assert_eq!(rows.len(), 3, "expected three worktree rows");
        assert_eq!(rows[0].path, PathBuf::from("/tmp/demo-main"));
        assert_eq!(rows[0].branch_label, "main");
        assert_eq!(rows[1].branch_label, "feature-x");
        assert_eq!(rows[2].branch_label, "(detached)");
    }

    /// Acceptance: traversal / escape attempts outside `main_repo_path` are rejected.
    #[test]
    fn project_path_validation_rejects_traversal_outside_repo_root() {
        let repo = PathBuf::from("/tmp/tddy-accept-repo");
        let evil = PathBuf::from("/tmp/tddy-accept-repo/../../../etc/passwd");
        let err = validate_worktree_path_within_repo_root(&repo, &evil).unwrap_err();
        match err {
            WorktreePathError::OutsideRepoRoot { .. } => {}
        }
    }
}
