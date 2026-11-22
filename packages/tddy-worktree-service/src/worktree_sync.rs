//! A session worktree's sync state — dirty files and the branch's relation to its upstream — and the
//! two actions it offers (`#workqueue` 4/14, the inspector's Worktree tab).
//!
//! The relation is decided by ancestry both ways (`merge-base --is-ancestor`), so a merge commit
//! counts as containing what it merged. Pull is fast-forward only; Push never forces. Nothing here
//! commits: committing in a live agent's worktree from the UI would race the agent.

use std::path::Path;
use std::process::{Command, Output};

/// How a branch relates to its upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    InSync,
    /// Local commits the upstream lacks.
    Unpushed,
    /// Upstream commits the local branch lacks.
    Unpulled,
    /// Both.
    Diverged,
    /// No upstream yet.
    Unpublished,
}

/// One commit, as a list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLine {
    pub sha: String,
    pub subject: String,
}

/// How a path differs from `HEAD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyChange {
    Modified,
    Staged,
    Untracked,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyFile {
    pub path: String,
    pub change: DirtyChange,
}

/// A worktree's state as the Worktree tab shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    pub branch: String,
    /// `origin/<branch>`; `None` when unpublished.
    pub upstream: Option<String>,
    pub status: SyncStatus,
    /// Oldest first.
    pub unpushed: Vec<CommitLine>,
    /// Oldest first.
    pub unpulled: Vec<CommitLine>,
    pub dirty_files: Vec<DirtyFile>,
}

impl SyncState {
    /// Pull is offered only when it can fast-forward: unpulled and nothing unpushed.
    pub fn can_pull(&self) -> bool {
        self.status == SyncStatus::Unpulled
    }

    /// Push is offered when there is something to push and nothing to pull first.
    pub fn can_push(&self) -> bool {
        matches!(self.status, SyncStatus::Unpushed | SyncStatus::Unpublished)
    }
}

/// Why a sync-state read or action failed (git output included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSyncError(pub String);

impl std::fmt::Display for WorktreeSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WorktreeSyncError {}

/// The remote a branch is first published to.
const PUBLISH_REMOTE: &str = "origin";

fn run_git(worktree: &Path, args: &[&str]) -> Result<Output, WorktreeSyncError> {
    Command::new("git")
        .args(args)
        .current_dir(worktree)
        // A credential prompt would hang a daemon thread; a missing credential is an error instead.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| WorktreeSyncError(format!("git {args:?} could not run: {e}")))
}

fn git_stdout(worktree: &Path, args: &[&str]) -> Result<String, WorktreeSyncError> {
    let out = run_git(worktree, args)?;
    if !out.status.success() {
        return Err(failed(args, &out));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn failed(args: &[&str], out: &Output) -> WorktreeSyncError {
    WorktreeSyncError(format!(
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

/// `git merge-base --is-ancestor <ancestor> <descendant>`: exit 0 yes, 1 no, anything else an error.
fn is_ancestor(
    worktree: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, WorktreeSyncError> {
    let args = ["merge-base", "--is-ancestor", ancestor, descendant];
    let out = run_git(worktree, &args)?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(failed(&args, &out)),
    }
}

fn current_branch(worktree: &Path) -> Result<String, WorktreeSyncError> {
    git_stdout(worktree, &["symbolic-ref", "--short", "HEAD"])
        .map(|name| name.trim().to_string())
        .map_err(|e| WorktreeSyncError(format!("the worktree is not on a branch: {e}")))
}

/// The branch's upstream (`origin/<branch>`), `None` when it has none.
fn upstream_of(worktree: &Path, branch: &str) -> Result<Option<String>, WorktreeSyncError> {
    let has_upstream = run_git(
        worktree,
        &["config", "--get", &format!("branch.{branch}.merge")],
    )?
    .status
    .success();
    if !has_upstream {
        return Ok(None);
    }
    git_stdout(
        worktree,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .map(|name| Some(name.trim().to_string()))
}

fn commits(worktree: &Path, range: &str) -> Result<Vec<CommitLine>, WorktreeSyncError> {
    let listing = git_stdout(worktree, &["log", "--reverse", "--format=%H%x1f%s", range])?;
    Ok(listing
        .lines()
        .filter_map(|line| line.split_once('\u{1f}'))
        .map(|(sha, subject)| CommitLine {
            sha: sha.to_string(),
            subject: subject.to_string(),
        })
        .collect())
}

fn dirty_files(worktree: &Path) -> Result<Vec<DirtyFile>, WorktreeSyncError> {
    let status = git_stdout(
        worktree,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )?;
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        let (Some(codes), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            return Err(WorktreeSyncError(format!(
                "unrecognised git status entry {entry:?}"
            )));
        };
        let mut codes = codes.chars();
        let (index, tree) = (codes.next().unwrap_or(' '), codes.next().unwrap_or(' '));
        let change = match (index, tree) {
            ('?', '?') => DirtyChange::Untracked,
            ('R' | 'C', _) | (_, 'R' | 'C') => DirtyChange::Renamed,
            ('D', _) | (_, 'D') => DirtyChange::Deleted,
            (_, ' ') => DirtyChange::Staged,
            _ => DirtyChange::Modified,
        };
        // A rename or copy is followed by its original path as a separate entry.
        if matches!(index, 'R' | 'C') || matches!(tree, 'R' | 'C') {
            entries.next();
        }
        files.push(DirtyFile {
            path: path.to_string(),
            change,
        });
    }
    Ok(files)
}

fn fetch_upstream(worktree: &Path, upstream: &str) -> Result<(), WorktreeSyncError> {
    let (remote, _) = upstream
        .split_once('/')
        .ok_or_else(|| WorktreeSyncError(format!("unrecognised upstream {upstream:?}")))?;
    git_stdout(worktree, &["fetch", "--quiet", remote]).map(|_| ())
}

/// Read `worktree`'s sync state after fetching its upstream (remote-tracking refs only).
pub fn read_sync_state(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    let branch = current_branch(worktree)?;
    let upstream = upstream_of(worktree, &branch)?;
    let dirty_files = dirty_files(worktree)?;
    let Some(upstream) = upstream else {
        return Ok(SyncState {
            branch,
            upstream: None,
            status: SyncStatus::Unpublished,
            unpushed: Vec::new(),
            unpulled: Vec::new(),
            dirty_files,
        });
    };
    fetch_upstream(worktree, &upstream)?;
    let local_is_contained = is_ancestor(worktree, "HEAD", &upstream)?;
    let upstream_is_contained = is_ancestor(worktree, &upstream, "HEAD")?;
    let status = match (local_is_contained, upstream_is_contained) {
        (true, true) => SyncStatus::InSync,
        (true, false) => SyncStatus::Unpulled,
        (false, true) => SyncStatus::Unpushed,
        (false, false) => SyncStatus::Diverged,
    };
    Ok(SyncState {
        unpushed: commits(worktree, &format!("{upstream}..HEAD"))?,
        unpulled: commits(worktree, &format!("HEAD..{upstream}"))?,
        branch,
        upstream: Some(upstream),
        status,
        dirty_files,
    })
}

/// Fast-forward `worktree`'s branch to its upstream; refused when it cannot fast-forward.
pub fn pull_fast_forward(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    let state = read_sync_state(worktree)?;
    let Some(upstream) = state.upstream.as_deref() else {
        return Err(WorktreeSyncError(format!(
            "branch {} has no upstream to pull",
            state.branch
        )));
    };
    git_stdout(worktree, &["merge", "--ff-only", upstream])?;
    read_sync_state(worktree)
}

/// Push `worktree`'s branch (setting its upstream when unpublished); never forces.
pub fn push_branch(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    let state = read_sync_state(worktree)?;
    match state.upstream {
        None => git_stdout(worktree, &["push", "-u", PUBLISH_REMOTE, &state.branch])?,
        Some(_) => git_stdout(worktree, &["push"])?,
    };
    read_sync_state(worktree)
}
