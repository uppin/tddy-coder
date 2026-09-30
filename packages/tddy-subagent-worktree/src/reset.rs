//! Taking a conversation's worktree back to an earlier point, for a caller that rewound the
//! conversation's transcript to a message and wants its files to follow.

use serde::{Deserialize, Serialize};

use crate::worktree::{ConversationWorktree, WorktreeError};

/// Where a reset goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResetTarget {
    /// The conversation's base: every subagent commit is dropped.
    Base,
    /// A commit on the conversation's branch, by any unambiguous abbreviation of its hash.
    Commit(String),
}

/// What a reset did — the `worktreeReset` object on a resumed turn's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeReset {
    /// The short hash the worktree now stands at.
    pub to: String,
    /// The short hashes the reset dropped from the branch, oldest first. A list rather than a
    /// count: a caller that already took some of them (`subagent_pull`) needs to know which.
    pub dropped_commits: Vec<String>,
}

impl ConversationWorktree {
    /// Hard-reset the worktree and its branch to `target`, removing untracked files the dropped
    /// commits created (ignored files are kept — a reset must not delete a build's output). A commit
    /// that is neither the base nor on the branch after it is refused, and nothing moves.
    pub async fn reset_to(&self, target: &ResetTarget) -> Result<WorktreeReset, WorktreeError> {
        // TODO(rewind-reset): implement
        todo!("reset_to({target:?})")
    }
}
