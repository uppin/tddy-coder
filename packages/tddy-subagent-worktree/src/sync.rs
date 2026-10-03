//! Bringing a conversation's worktree up to its caller's current files before a turn.
//!
//! A conversation worktree is cut once, from the caller's files at its first write, and afterwards
//! moves only through the subagent's tool calls. A caller that edits its own worktree between turns
//! would otherwise send the subagent back into a tree the caller no longer has. The sync merges the
//! caller's current state — `HEAD` plus staged, unstaged and untracked changes, snapshotted as the
//! conversation's start is — into the conversation's branch as a **merge commit**: first parent the
//! subagent's tip, second parent the caller snapshot, merged against the last caller state the
//! conversation took in. The subagent's own work is therefore always the branch's first-parent line
//! without merges ([`crate::ConversationWorktree::subagent_commits`]).

use serde::{Deserialize, Serialize};

use crate::change_facts::{FileCounts, LineCounts};
use crate::worktree::{ConversationWorktree, WorktreeError};

/// How many changed paths a sync names; the rest are counted in [`WorktreeSync::more_paths`].
pub const SYNC_NOTICE_PATHS: usize = 20;

/// The subject a sync's merge commit carries.
pub const SYNC_MERGE_SUBJECT: &str = "Merge the caller's changes";

/// The subject of the commit a sync makes of uncommitted changes it finds in the conversation
/// worktree — the subagent's own work (a background job that finished between turns).
pub const OUTSIDE_A_TOOL_CALL_SUBJECT: &str = "Changes made outside a tool call";

/// What a sync merged — the `worktreeSync` object on a turn's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSync {
    /// Short hash of the merge commit.
    pub commit: String,
    /// What the merge changed in the conversation worktree.
    pub files: FileCounts,
    pub lines: LineCounts,
    /// The changed paths, sorted, at most [`SYNC_NOTICE_PATHS`].
    pub paths: Vec<String>,
    /// How many changed paths `paths` leaves out.
    pub more_paths: usize,
}

/// What a sync did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The caller's files are what the conversation last took in: nothing merged.
    Unchanged,
    /// The caller's changes were merged.
    Merged(WorktreeSync),
    /// The caller's changes and the subagent's work touch the same lines in these paths; nothing
    /// was merged and nothing moved.
    Conflicted { paths: Vec<String> },
}

impl ConversationWorktree {
    /// Merge the caller's current files into the conversation's branch and worktree.
    ///
    /// Uncommitted changes already in the conversation worktree are first committed as the
    /// subagent's, subject [`OUTSIDE_A_TOOL_CALL_SUBJECT`]. The caller's worktree, index and branch
    /// are only read.
    pub async fn sync_with_caller(&self) -> Result<SyncOutcome, WorktreeError> {
        // TODO(caller-sync): implement
        todo!("sync_with_caller")
    }
}
