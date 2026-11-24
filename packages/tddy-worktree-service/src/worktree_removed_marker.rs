//! The record that a close removed a session's worktrees: the file `worktree-removed` in the
//! session directory, holding the RFC 3339 time of the removal
//! (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! A file beside `.session.yaml` rather than a field of it: `SessionMetadata` refuses unknown
//! fields, so a daemon that predates the field could no longer read a closed session's record.
//! `RestoreSessionWorktree` removes it once the worktree is back.

use std::io;
use std::path::Path;

/// The marker's file name in the session directory.
pub const WORKTREE_REMOVED_MARKER: &str = "worktree-removed";

/// Record in `session_dir` that its session's worktrees were removed, now.
pub fn record_worktree_removed(session_dir: &Path) -> io::Result<()> {
    std::fs::write(
        session_dir.join(WORKTREE_REMOVED_MARKER),
        format!("{}\n", chrono::Utc::now().to_rfc3339()),
    )
}

/// Whether `session_dir` records that its session's worktrees were removed.
#[must_use]
pub fn worktree_removal_recorded(session_dir: &Path) -> bool {
    session_dir.join(WORKTREE_REMOVED_MARKER).is_file()
}

/// Forget that `session_dir`'s worktrees were removed — the worktree is back. A session that
/// records no removal has nothing to forget.
pub fn clear_worktree_removed(session_dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(session_dir.join(WORKTREE_REMOVED_MARKER)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_removal_is_read_back_until_it_is_cleared() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let before = worktree_removal_recorded(dir.path());
        record_worktree_removed(dir.path()).unwrap();
        let recorded = worktree_removal_recorded(dir.path());
        clear_worktree_removed(dir.path()).unwrap();
        clear_worktree_removed(dir.path()).unwrap();

        // Then
        assert_eq!(
            (before, recorded, worktree_removal_recorded(dir.path())),
            (false, true, false)
        );
    }
}
