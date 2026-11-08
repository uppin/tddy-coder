//! Applying a changed `users.yaml` (`#agent-tools` 31/32): rebuild the per-user daemon sockets,
//! then restart every service that serves users, so it is handed the new set.
//!
//! Kept out of `supervisor.rs`, which is at its length budget
//! (`packages/tddy-supervisor/docs/code-issues/oversized-file-supervisor.md`, closed).
//!
//! **Order.** The new listeners exist before the old daemon is stopped, and a user both mappings
//! name keeps their listener ([`crate::user_sockets::UserListeners::rebuild`]), so their clients
//! queue in the kernel across the restart instead of being refused. A restart drops every relayed
//! agent and the daemon's in-memory state; sessions resume from disk.

use crate::supervisor::Supervisor;
use crate::users_file::UsersFile;

impl Supervisor {
    /// Make `next` the mapping in force: rebuild the user sockets of every service declaring
    /// `serves_users`, then restart those services with the rebuilt handover.
    pub async fn apply_users_change(&self, next: &UsersFile) -> anyhow::Result<()> {
        // TODO(user-mapping-in-supervisor): implement
        let _ = next;
        Ok(())
    }
}
