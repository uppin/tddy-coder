//! Taking a conversation's worktree back with its transcript, when a caller rewinds.
//!
//! `subagent_resume { fromMessageId }` rewinds the transcript to a message. The files the dropped
//! messages wrote are in the conversation's worktree (`tddy-subagent-worktree`), one commit per
//! mutating call, and each tool result's entry recorded its commit. So the rewind knows where the
//! files should go — the commit of the last entry it keeps — and asks the host that owns the
//! worktree to go there through this port, **before** the transcript is cut: a reset that fails
//! leaves the conversation exactly as it was.

use async_trait::async_trait;

pub use tddy_subagent_worktree::{ResetTarget, WorktreeReset};

use super::SubagentError;

/// How a conversation asks the host that owns its worktree for a reset.
#[async_trait]
pub trait WorktreeResetPort: Send + Sync {
    /// Reset to `target`. `Ok(None)` means the conversation has no worktree — it never made a
    /// mutating call — so there was nothing to reset.
    async fn reset(&self, target: ResetTarget) -> Result<Option<WorktreeReset>, SubagentError>;
}
