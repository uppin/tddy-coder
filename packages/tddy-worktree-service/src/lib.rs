//! The git worktrees a daemon serves: listing them, cleaning and removing them, sizing them,
//! restoring one for a session and reading files out of one — served as `worktree.WorktreeService`.
//!
//! Split out of `the pre-unbundle monolithic RPC coordinate` by `#unbundle` node 1. None of these nine methods
//! routes to a peer: a worktree is a directory on the daemon that holds it, and there is no
//! `daemon_instance_id` on any request here to route by.

pub mod base_sync_cache;
pub mod branch_intent;
pub mod branch_owner;
pub use tddy_projects::{project_provision, project_storage};
pub mod remote_git_service;
pub mod worktree_files;
pub mod worktrees;

pub mod service;
pub mod stream;
/// Shared test helpers, ungated like `tddy_daemon::test_util` is: the integration suites in
/// `tests/` reach for them, and a `#[cfg(test)]` module is invisible from there.
pub mod test_util;

pub use service::{NoSessionRooms, WorktreeRoomCloser, WorktreeServiceImpl};
pub use stream::{worktree_file_frames, MpscResultStream, MpscWorktreeStatsStream};
