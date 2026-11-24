use crate::worktree_sizes::directory_size_bytes_best_effort;
use crate::worktrees::git_diff_numstat;
use crate::worktrees::git_worktree_list_stdout;
use crate::worktrees::parse_git_worktree_list;
use crate::worktrees::WorktreeListRow;
use crate::worktrees::WorktreeStatSnapshot;

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn build_worktree_stat_snapshots(rows: &[WorktreeListRow]) -> Vec<WorktreeStatSnapshot> {
    let mut snapshots = Vec::with_capacity(rows.len());
    for row in rows {
        let disk_bytes = directory_size_bytes_best_effort(&row.path);
        let numstat = git_diff_numstat(&row.path);
        let updated_at_unix_ms = chrono::Utc::now().timestamp_millis();
        snapshots.push(WorktreeStatSnapshot {
            path: row.path.clone(),
            branch_label: row.branch_label.clone(),
            disk_bytes,
            changed_files: numstat.changed_files,
            lines_added: numstat.lines_added,
            lines_removed: numstat.lines_removed,
            updated_at_unix_ms,
            stale: false,
        });
    }
    snapshots
}

pub(crate) fn write_worktree_stats_cache_file(path: &Path, snapshots: Vec<WorktreeStatSnapshot>) {
    let payload = WorktreeStatsCacheFile { snapshots };
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => {
            if let Err(e) = tddy_core::atomic_file::write_atomic(path, json) {
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

#[derive(Serialize, Deserialize)]
pub(crate) struct WorktreeStatsCacheFile {
    pub(crate) snapshots: Vec<WorktreeStatSnapshot>,
}
