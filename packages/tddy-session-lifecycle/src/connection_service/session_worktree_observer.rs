//! Telling the daemon that a started session's worktree exists.
//!
//! What a daemon does with a fresh worktree (warming its code index, say) lives in crates above this
//! one, which this crate cannot name. [`SessionWorktreeObserver`] is the port the composition root
//! installs on [`DaemonSessionHost`] so those crates hear about it, without the start waiting on
//! whatever they do.

use std::path::Path;

use crate::connection_service::launch_ports::LaunchSessions;

/// Hears that a session's worktree now exists on this host.
///
/// Called once per successfully started session, after the start has finished, from the start's own
/// task: an implementation must return promptly and move slow work (an index load) onto a task of
/// its own, so that starting a session never waits on it.
pub trait SessionWorktreeObserver: Send + Sync {
    fn worktree_ready(&self, session_id: &str, worktree: &Path);
}

impl LaunchSessions {
    /// Report the worktree of the session `session_id`, which has just started under
    /// `sessions_base`, to the installed observer. Nothing happens when none is installed.
    ///
    /// The worktree is read back from the session's own `.session.yaml`, the one place every
    /// session type records it. A session whose record cannot be read is logged, not failed: it
    /// already started, and the observer only enhances it.
    pub(crate) fn announce_worktree_ready(&self, sessions_base: &Path, session_id: &str) {
        let Some(observer) = self.worktree_observer.as_ref() else {
            return;
        };
        match tddy_session_agents::peer_session_answer::resolve_worktree_root_for_session(
            sessions_base,
            session_id,
        ) {
            Ok(worktree) => observer.worktree_ready(session_id, &worktree),
            Err(status) => log::warn!(
                "session {session_id}: its worktree could not be announced: {}",
                status.message
            ),
        }
    }
}
