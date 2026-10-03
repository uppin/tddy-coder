//! Which commits on a conversation's branch are the subagent's work.
//!
//! A sync ([`crate::ConversationWorktree::sync_with_caller`]) puts merge commits on the branch whose
//! second parent is a snapshot of the caller. Walking `base..branch` would then list the caller's
//! own commits and snapshots as if the subagent had made them — a pull would hand the caller its own
//! changes back. The subagent's work is the branch's **first-parent line without merges**, and every
//! operation that lists it reads this one listing.

use crate::worktree::{ConversationWorktree, WorktreeError};

impl ConversationWorktree {
    /// Full hashes of the subagent's commits after the base, oldest first: the first-parent line of
    /// `base..branch`, merges left out.
    pub async fn subagent_commits(&self) -> Result<Vec<String>, WorktreeError> {
        // TODO(caller-sync): implement — `rev-list --first-parent --no-merges --reverse base..branch`
        todo!("subagent_commits")
    }

    /// The last caller state the conversation took in: the second parent of the newest merge on the
    /// branch's first-parent line, or the base when there is none. A reset past a merge drops it, so
    /// the answer follows the branch without any record to invalidate.
    #[allow(dead_code)] // TODO(caller-sync): read by `sync_with_caller`
    pub(crate) async fn caller_state_taken_in(&self) -> Result<String, WorktreeError> {
        // TODO(caller-sync): implement
        todo!("caller_state_taken_in")
    }
}
