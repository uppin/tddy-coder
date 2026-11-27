use crate::worktrees::paths_equal;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, Semaphore};

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
pub(crate) struct PersistedWorktreeSize {
    disk_bytes: u64,
    calculated_at_unix_ms: i64,
}

/// On-disk shape of `{root}/{project}/worktree_sizes.json` (map of worktree path -> size).
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct WorktreeSizesCacheFile {
    sizes: HashMap<String, PersistedWorktreeSize>,
}

/// In-memory state shared between the calculator handle and its spawned walk tasks.
#[derive(Default)]
pub(crate) struct WorktreeSizeInner {
    /// (project_id, worktree_path) -> current state.
    states: HashMap<(String, PathBuf), WorktreeSizeState>,
    /// project_id -> broadcast sender for that project's updates.
    senders: HashMap<String, broadcast::Sender<WorktreeSizeUpdate>>,
}

/// Capacity of each project's broadcast channel; ample so a slow subscriber created before a walk
/// still observes the full `Calculating` -> `Cached` sequence.
pub(crate) const SIZE_UPDATE_CHANNEL_CAPACITY: usize = 1024;

/// The daemon-wide cap on concurrent size walks, shared by every calculator a daemon builds.
pub const DAEMON_SIZE_WALK_PERMITS: usize = 2;

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
    fs: Arc<dyn tddy_fs::Fs>,
}

impl WorktreeSizeCalculator {
    /// Production constructor: uses the real best-effort directory walk as the sizer.
    pub fn new(root: PathBuf, permits: usize, fs: Arc<dyn tddy_fs::Fs>) -> Self {
        let walk_fs = Arc::clone(&fs);
        let sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync> =
            Arc::new(move |path| directory_size_bytes_best_effort(&*walk_fs, path));
        Self::with_sizer(root, permits, sizer, fs)
    }

    /// Constructor with an injectable sizer (used by tests to observe concurrency and gate walks).
    pub fn with_sizer(
        root: PathBuf,
        permits: usize,
        sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync>,
        fs: Arc<dyn tddy_fs::Fs>,
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
            fs,
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
    /// persists it, then marks it `Cached` and broadcasts. Re-enqueuing a worktree that is already
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
        let fs = Arc::clone(&self.fs);
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

            // Persist before publishing `Cached`: a reader that sees the update and opens a fresh
            // calculator over the same root must find the size on disk.
            persist_worktree_size(
                &*fs,
                &root,
                &persist_lock,
                &project_id,
                &path,
                bytes,
                calculated_at_unix_ms,
            );

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

        let cache =
            read_worktree_sizes_file(&*self.fs, &project_sizes_file(&self.root, project_id));
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
        let cache = read_worktree_sizes_file(&*self.fs, &file);
        cache.sizes.get(&worktree_size_key(path)).cloned()
    }
}

/// Persisted-sizes file path for a project: `{root}/{sanitized_project}/worktree_sizes.json`.
/// Sanitization matches [`WorktreeStatsCache`] so the two caches share a per-project directory
/// while keeping distinct files.
pub(crate) fn project_sizes_file(root: &Path, project_id: &str) -> PathBuf {
    let safe = project_id.replace(['/', '\\', ':'], "_");
    root.join(safe).join("worktree_sizes.json")
}

pub(crate) fn worktree_size_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn read_worktree_sizes_file(fs: &dyn tddy_fs::Fs, file: &Path) -> WorktreeSizesCacheFile {
    match fs.read_to_string(file) {
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

pub(crate) fn persist_worktree_size(
    fs: &dyn tddy_fs::Fs,
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
        if let Err(e) = fs.create_dir_all(dir) {
            warn!("persist_worktree_size: create_dir_all {:?}: {}", dir, e);
            return;
        }
    }
    let mut cache = read_worktree_sizes_file(fs, &file);
    cache.sizes.insert(
        worktree_size_key(path),
        PersistedWorktreeSize {
            disk_bytes,
            calculated_at_unix_ms,
        },
    );
    match serde_json::to_string_pretty(&cache) {
        Ok(json) => {
            if let Err(e) = fs.write_atomic(&file, json.as_bytes(), None) {
                warn!("persist_worktree_size: write {:?}: {}", file, e);
            }
        }
        Err(e) => warn!("persist_worktree_size: serialize: {}", e),
    }
}

pub(crate) fn directory_size_bytes_best_effort(fs: &dyn tddy_fs::Fs, path: &Path) -> u64 {
    let walk_root = path.to_path_buf();
    let mut total = 0u64;
    // `metadata_follow`, as before: the question stays about the file itself.
    if let Ok(m) = fs.metadata_follow(path) {
        if m.kind == tddy_fs::FsKind::File {
            return m.len;
        }
    }
    let mut stack = vec![walk_root.clone()];
    while let Some(p) = stack.pop() {
        let entries = match fs.read_dir(&p) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for ent in entries {
            // `metadata_follow`, as before: the walk follows a link.
            let meta = match fs.metadata_follow(&ent) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.kind == tddy_fs::FsKind::Dir {
                if paths_equal(&p, &walk_root)
                    && ent.file_name().is_some_and(|n| n == ".worktrees")
                {
                    continue;
                }
                stack.push(ent);
            } else {
                total += meta.len;
            }
        }
    }
    total
}
