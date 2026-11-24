//! What removing a worktree would lose — the dirty-state reader behind `PreviewCloseSession`
//! (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! Two kinds of loss, per worktree:
//! - every `git status --porcelain --untracked-files=all` entry: staged, unstaged, deleted, renamed,
//!   and untracked files that are not ignored;
//! - every commit on `HEAD` that no remote-tracking ref and not the session's recorded base hold
//!   (`git log HEAD --not --remotes [<base>]`), listed up to [`LISTED_UNPUSHED_COMMITS`], counted in
//!   full.
//!
//! Read here for a worktree on this host, and through a [`RemoteShell`] for one on a session's
//! target — the same commands, the same parser.

use std::path::Path;

use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;

use crate::worktree_sync::{CommitLine, DirtyFile, WorktreeSyncError};

/// How many unpushed commits a loss lists; [`WorktreeLoss::unpushed_count`] counts them all.
pub const LISTED_UNPUSHED_COMMITS: usize = 20;

/// What removing one worktree would lose.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeLoss {
    /// Uncommitted files, in `git status` order.
    pub dirty_files: Vec<DirtyFile>,
    /// Commits no remote and not the base hold, newest first, at most [`LISTED_UNPUSHED_COMMITS`].
    pub unpushed: Vec<CommitLine>,
    /// How many such commits there are in all.
    pub unpushed_count: u32,
}

impl WorktreeLoss {
    /// Whether removing the worktree would lose nothing.
    pub fn is_empty(&self) -> bool {
        self.dirty_files.is_empty() && self.unpushed_count == 0
    }
}

/// Parse `git status --porcelain -z --untracked-files=all` output into the files it names.
pub fn parse_porcelain_status(_status: &str) -> Result<Vec<DirtyFile>, WorktreeSyncError> {
    // TODO(session-close): share the parser `worktree_sync::dirty_files` uses.
    Err(WorktreeSyncError(
        "parse_porcelain_status is not implemented yet".to_string(),
    ))
}

/// What removing `worktree`, on this host, would lose. `base` is the commit the session was cut
/// from, when it recorded one: commits it holds are not losses.
pub fn read_local_worktree_loss(
    _worktree: &Path,
    _base: Option<&str>,
) -> Result<WorktreeLoss, WorktreeSyncError> {
    // TODO(session-close): git status + git log HEAD --not --remotes [base].
    Err(WorktreeSyncError(
        "read_local_worktree_loss is not implemented yet".to_string(),
    ))
}

/// What removing `worktree` on a session's target would lose, read through `shell`.
pub async fn read_target_worktree_loss(
    _shell: &dyn RemoteShell,
    _worktree: &str,
    _base: Option<&str>,
) -> Result<WorktreeLoss, Status> {
    // TODO(session-close): the same commands as read_local_worktree_loss, over the remote shell.
    Err(Status::unimplemented(
        "read_target_worktree_loss is not implemented yet",
    ))
}
