//! The registry as the core's [`SessionRooms`] port: closing and hosting, nothing more.
//!
//! A sibling module rather than more of `session_room.rs`, which is already over its size budget.

use std::path::Path;

use tddy_daemon_extensions::SessionRooms;

use crate::session_room::SessionRoomRegistry;

impl SessionRooms for SessionRoomRegistry {
    fn hosts(&self, session_id: &str) -> bool {
        let _ = session_id;
        todo!("SessionRoomRegistry as SessionRooms: hosts")
    }

    fn close(&self, session_id: &str) {
        let _ = session_id;
        todo!("SessionRoomRegistry as SessionRooms: close")
    }

    fn close_for_worktree(&self, worktree: &Path) {
        let _ = worktree;
        todo!("SessionRoomRegistry as SessionRooms: close_for_worktree")
    }
}
