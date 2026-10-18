//! The rooms this daemon hosts for its sessions, as code outside the integration sees them.
//!
//! Code that only needs to stop hosting a session's room — deleting the session, removing its
//! worktree — or to ask whether one is hosted, reaches the registry through this port instead of
//! naming the integration that hosts rooms. A daemon with none wired has [`NoSessionRooms`]: nothing
//! is hosted and closing is a no-op, which is exactly what such a daemon does today.

use std::path::Path;

/// The rooms this daemon hosts, by session.
pub trait SessionRooms: Send + Sync {
    /// Whether this daemon currently hosts `session_id`'s room.
    fn hosts(&self, session_id: &str) -> bool;

    /// Stop hosting `session_id`'s room, if one is hosted.
    fn close(&self, session_id: &str);

    /// Stop hosting every room measuring `worktree`, before the checkout is removed.
    fn close_for_worktree(&self, worktree: &Path);
}

/// No integration hosting rooms: nothing is hosted, and closing is a no-op.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSessionRooms;

impl SessionRooms for NoSessionRooms {
    fn hosts(&self, _session_id: &str) -> bool {
        false
    }

    fn close(&self, _session_id: &str) {}

    fn close_for_worktree(&self, _worktree: &Path) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_wired_no_session_room_is_hosted() {
        assert!(!NoSessionRooms.hosts("1780828020298-abc"));
    }

    #[test]
    fn with_nothing_wired_closing_is_a_no_op() {
        // Closing a room nobody hosts, by session and by worktree, completes without effect
        NoSessionRooms.close("1780828020298-abc");
        NoSessionRooms.close_for_worktree(Path::new("/worktrees/my-app/feat-x"));
        assert!(!NoSessionRooms.hosts("1780828020298-abc"));
    }
}
