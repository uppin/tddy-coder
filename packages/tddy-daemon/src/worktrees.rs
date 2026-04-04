//! Git worktree listing, stats cache, path policy, and removal helpers (Worktrees manager PRD).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

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

/// Root directory for persisted per-project worktree stats (`~/.tddy/projects/...` by default).
/// Override with `TDDY_PROJECTS_STATS_ROOT` for integration tests.
pub fn projects_stats_cache_root() -> PathBuf {
    debug!("projects_stats_cache_root: resolving cache root");
    if let Ok(p) = std::env::var("TDDY_PROJECTS_STATS_ROOT") {
        info!(
            "projects_stats_cache_root: using TDDY_PROJECTS_STATS_ROOT={}",
            p
        );
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").expect("HOME must be set for default stats cache root");
    let root = PathBuf::from(home).join(".tddy").join("projects");
    info!("projects_stats_cache_root: default {:?}", root);
    root
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

fn directory_size_bytes_best_effort(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(m) = fs::metadata(path) {
        if m.is_file() {
            return m.len();
        }
    }
    let mut stack = vec![path.to_path_buf()];
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
    let listed = rows.iter().any(|r| paths_equal(&r.path, worktree_path));
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

#[cfg(test)]
mod tests {
    use super::*;

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
