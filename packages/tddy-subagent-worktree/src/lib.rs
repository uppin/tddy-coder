//! A subagent conversation's own branch and worktree.
//!
//! A specialized subagent that edits code used to edit its caller's worktree directly. This crate
//! gives each conversation a worktree of its own instead, inside the session worktree at
//! [`SUBAGENT_WORKTREES_DIR`]`/<conversation id>` — the one directory every exec-tool route, host
//! and jail alike, can already reach:
//!
//! - it is created **lazily**, on the conversation's first [`ToolEffect::Mutating`] call, from the
//!   caller's `HEAD` plus the caller's uncommitted state as one commit on the new branch only;
//! - every call that ran against it and changed a file is **committed**, and the call's
//!   [`WorktreeChange`] says how many files it created, updated and removed, how many lines it added
//!   and removed, and the commit's short hash;
//! - [`ConversationWorktree::pull_into_caller`] hands the result to the caller as uncommitted
//!   changes, 3-way, and [`ConversationWorktree::remove`] deletes the worktree and its branch;
//! - [`ConversationWorktree::diff`] reads what the conversation changed between two of its commits
//!   (git's `from..to`, the base and the tip by default), read-only, with counts over the whole range
//!   and the text capped at [`DIFF_TEXT_CAP_BYTES`].
//!
//! Git runs through the CLI, on the host that owns the session worktree: a linked worktree's `.git`
//! points into the repository's common dir, which a jail mounting only the checkout cannot see.

mod change_facts;
mod conversation_id;
mod diff;
mod git;
mod inherit;
mod range_pull;
mod reset;
mod run;
mod serialise;
mod tool_effect;
mod worktree;

pub use change_facts::{FileCounts, LineCounts, WorktreeChange};
pub use conversation_id::{ConversationId, UnsafeConversationId};
pub use diff::{ConversationDiff, DIFF_TEXT_CAP_BYTES};
pub use range_pull::{PullRange, RangePullOutcome};
pub use reset::{ResetTarget, WorktreeReset};
pub use run::{run_in_conversation, with_worktree_change, ConversationRun, WORKTREE_CHANGE_KEY};
pub use tool_effect::ToolEffect;
pub use worktree::{
    ConversationWorktree, ConversationWorktrees, PullOutcome, WorktreeError, SUBAGENT_WORKTREES_DIR,
};
