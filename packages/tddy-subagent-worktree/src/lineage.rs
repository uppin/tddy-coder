//! Which commits on a conversation's branch are the subagent's work.
//!
//! A sync ([`crate::ConversationWorktree::sync_with_caller`]) puts merge commits on the branch whose
//! second parent is a snapshot of the caller. Walking `base..branch` would then list the caller's
//! own commits and snapshots as if the subagent had made them — a pull would hand the caller its own
//! changes back. The subagent's work is the branch's **first-parent line without merges**, and every
//! operation that lists it reads this one listing.

use crate::git::git;
use crate::worktree::{ConversationWorktree, WorktreeError};

impl ConversationWorktree {
    /// Full hashes of the subagent's commits after the base, oldest first: the first-parent line of
    /// `base..branch`, merges left out.
    pub async fn subagent_commits(&self) -> Result<Vec<String>, WorktreeError> {
        let range = format!("{}..{}", self.base(), self.branch());
        let listed = git(
            self.root(),
            [
                "rev-list",
                "--first-parent",
                "--no-merges",
                "--reverse",
                &range,
            ],
            &[],
            None,
        )
        .await?;
        Ok(listed.lines().map(str::to_string).collect())
    }

    /// The last caller state the conversation took in: the second parent of the newest merge on the
    /// branch's first-parent line, or the base when there is none. A reset past a merge drops it, so
    /// the answer follows the branch without any record to invalidate.
    pub(crate) async fn caller_state_taken_in(&self) -> Result<String, WorktreeError> {
        let range = format!("{}..{}", self.base(), self.branch());
        match self.newest_merge_in(&range).await? {
            Some(merge) => self.resolve_commit(&format!("{merge}^2")).await,
            None => Ok(self.base().to_string()),
        }
    }

    /// Whether a sync merge lies on the first-parent line of `from..to`.
    pub(crate) async fn merges_between(&self, from: &str, to: &str) -> Result<bool, WorktreeError> {
        Ok(self
            .newest_merge_in(&format!("{from}..{to}"))
            .await?
            .is_some())
    }

    /// The newest merge commit on the first-parent line of `range`, if there is one.
    async fn newest_merge_in(&self, range: &str) -> Result<Option<String>, WorktreeError> {
        let newest = git(
            self.root(),
            ["rev-list", "--first-parent", "--merges", "-n", "1", range],
            &[],
            None,
        )
        .await?;
        let newest = newest.trim();
        Ok((!newest.is_empty()).then(|| newest.to_string()))
    }
}
