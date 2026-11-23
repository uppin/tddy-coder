//! The free helpers of [`crate::service`]: run blocking work under a deadline, build a
//! `WorktreeRow` from a worktree's diff and size state, and map the library errors of Remove and
//! Clean to RPC statuses.

use std::time::Duration;

use tddy_rpc::Status;
use tddy_service::proto::worktree::{
    WorktreeCommitLine, WorktreeDirtyFile, WorktreeRow, WorktreeSyncState, WorktreeSyncStatus,
};

use crate::worktree_sync::{CommitLine, DirtyChange, SyncState, SyncStatus};
use crate::worktrees::{
    CleanWorktreeError, RemoveWorktreeError, WorktreeDiffRow, WorktreeSizeStatus,
};

/// Run blocking git/filesystem work under a wall-clock cap, so a hung git cannot block an RPC
/// forever.
///
/// `op_label` names the RPC in the deadline message, because the operator reading it is looking at
/// one call and needs to know which.
pub(crate) async fn blocking_within<T: Send + 'static>(
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
pub(crate) fn proto_worktree_size_status(
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
pub(crate) fn worktree_row_from_diff(
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

pub(crate) fn map_remove_worktree_error(e: RemoveWorktreeError) -> Status {
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

pub(crate) fn map_clean_worktree_error(e: CleanWorktreeError) -> Status {
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

/// The wire form of a worktree's sync state, with the Pull / Push offers it implies.
pub(crate) fn sync_state_to_proto(state: SyncState) -> WorktreeSyncState {
    let commit_lines = |lines: &[CommitLine]| {
        lines
            .iter()
            .map(|line| WorktreeCommitLine {
                sha: line.sha.clone(),
                subject: line.subject.clone(),
            })
            .collect()
    };
    WorktreeSyncState {
        can_pull: state.can_pull(),
        can_push: state.can_push(),
        unpushed: commit_lines(&state.unpushed),
        unpulled: commit_lines(&state.unpulled),
        dirty_files: state
            .dirty_files
            .iter()
            .map(|file| WorktreeDirtyFile {
                path: file.path.clone(),
                change: match file.change {
                    DirtyChange::Modified => "modified",
                    DirtyChange::Staged => "staged",
                    DirtyChange::Untracked => "untracked",
                    DirtyChange::Deleted => "deleted",
                    DirtyChange::Renamed => "renamed",
                }
                .to_string(),
            })
            .collect(),
        status: match state.status {
            SyncStatus::InSync => WorktreeSyncStatus::InSync,
            SyncStatus::Unpushed => WorktreeSyncStatus::Unpushed,
            SyncStatus::Unpulled => WorktreeSyncStatus::Unpulled,
            SyncStatus::Diverged => WorktreeSyncStatus::Diverged,
            SyncStatus::Unpublished => WorktreeSyncStatus::Unpublished,
        } as i32,
        branch: state.branch,
        upstream: state.upstream.unwrap_or_default(),
        unavailable_reason: String::new(),
    }
}
