//! The registry as the core's [`SessionRooms`] port: closing and hosting, nothing more.
//!
//! A sibling module rather than more of `session_room.rs`, which is already over its size budget.

use std::path::Path;

use tddy_daemon_extensions::SessionRooms;

use crate::session_room::SessionRoomRegistry;

impl SessionRooms for SessionRoomRegistry {
    fn hosts(&self, session_id: &str) -> bool {
        self.contains(session_id)
    }

    fn close(&self, session_id: &str) {
        SessionRoomRegistry::close(self, session_id);
    }

    fn close_for_worktree(&self, worktree: &Path) {
        SessionRoomRegistry::close_for_worktree(self, worktree);
    }
}
