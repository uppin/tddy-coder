//! The reset port an in-process subagent conversation is given: a rewind asks the facilitating
//! daemon to take the conversation's worktree back, over the same transport its tool calls use.

use async_trait::async_trait;
use tddy_discovery::subagent::{ResetTarget, SubagentError, WorktreeReset, WorktreeResetPort};

/// Resets one conversation's worktree through `ConversationWorktree { reset }`.
#[allow(dead_code)] // TODO(rewind-reset): given to each Managed conversation in `subagent_new_session`
pub(crate) struct ConversationWorktreeResetPort {
    conversation_id: String,
}

#[allow(dead_code)] // TODO(rewind-reset): see the struct
impl ConversationWorktreeResetPort {
    pub(crate) fn new(conversation_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
        }
    }
}

#[async_trait]
impl WorktreeResetPort for ConversationWorktreeResetPort {
    async fn reset(&self, target: ResetTarget) -> Result<Option<WorktreeReset>, SubagentError> {
        // TODO(rewind-reset): implement — `tddy_session_tool_client::reset_conversation_worktree`,
        // `{"reset": null}` → `None`, an `is_error` body → `Err`
        let _ = (&self.conversation_id, target);
        todo!("ConversationWorktreeResetPort::reset")
    }
}
