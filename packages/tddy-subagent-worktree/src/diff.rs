//! Reading what a conversation changed between two of its commits.

use serde::{Deserialize, Serialize};

use crate::change_facts::{FileCounts, LineCounts};
use crate::worktree::{ConversationWorktree, WorktreeError};

/// The most diff text one answer carries. Past it the text is cut at the last whole line and
/// [`ConversationDiff::truncated`] is set; the counts still describe the whole range.
pub const DIFF_TEXT_CAP_BYTES: usize = 64 * 1024;

/// The changes a conversation made after `from`, up to and including `to` — git's `from..to`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationDiff {
    /// Short hash of the exclusive lower bound.
    pub from: String,
    /// Short hash of the inclusive upper bound.
    pub to: String,
    pub files: FileCounts,
    pub lines: LineCounts,
    /// Unified diff text (`--binary` is not used: a binary file shows as git's `Binary files … differ`).
    pub diff: String,
    pub truncated: bool,
}

impl ConversationWorktree {
    /// The diff `from..to`. `from` defaults to the base, `to` to the branch tip. Each must be the base
    /// or a commit on the branch after it, and `from` an ancestor of `to`; anything else is refused.
    pub async fn diff(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<ConversationDiff, WorktreeError> {
        // TODO(diff): implement
        todo!("diff({from:?}, {to:?})")
    }
}
