//! A session worktree's sync state — dirty files and the branch's relation to its upstream — and the
//! two actions it offers (`#workqueue` 4/14, the inspector's Worktree tab).
//!
//! The relation is decided by ancestry both ways (`merge-base --is-ancestor`), so a merge commit
//! counts as containing what it merged. Pull is fast-forward only; Push never forces. Nothing here
//! commits: committing in a live agent's worktree from the UI would race the agent.

use std::path::Path;

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
        // TODO(n4 session-pr-tabs): implement
        todo!("TODO(n4): implement SyncState::can_pull")
    }

    /// Push is offered when there is something to push and nothing to pull first.
    pub fn can_push(&self) -> bool {
        // TODO(n4 session-pr-tabs): implement
        todo!("TODO(n4): implement SyncState::can_push")
    }
}

/// Why a sync-state read or action failed (git output included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSyncError(pub String);

/// Read `worktree`'s sync state after fetching its upstream (remote-tracking refs only).
pub fn read_sync_state(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    // TODO(n4 session-pr-tabs): implement
    let _ = worktree;
    todo!("TODO(n4): implement read_sync_state")
}

/// Fast-forward `worktree`'s branch to its upstream; refused when it cannot fast-forward.
pub fn pull_fast_forward(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    // TODO(n4 session-pr-tabs): implement
    let _ = worktree;
    todo!("TODO(n4): implement pull_fast_forward")
}

/// Push `worktree`'s branch (setting its upstream when unpublished); never forces.
pub fn push_branch(worktree: &Path) -> Result<SyncState, WorktreeSyncError> {
    // TODO(n4 session-pr-tabs): implement
    let _ = worktree;
    todo!("TODO(n4): implement push_branch")
}
