//! The reset port an in-process subagent conversation is given: a rewind asks the facilitating
//! daemon to take the conversation's worktree back, over the same transport its tool calls use.

use async_trait::async_trait;
use tddy_discovery::subagent::{ResetTarget, SubagentError, WorktreeReset, WorktreeResetPort};
use tddy_session_tool_client::reset_conversation_worktree;

/// Resets one conversation's worktree through `ConversationWorktree { reset }`.
pub(crate) struct ConversationWorktreeResetPort {
    conversation_id: String,
}

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
        let commit = match &target {
            ResetTarget::Base => None,
            ResetTarget::Commit(commit) => Some(commit.as_str()),
        };
        let answer = reset_conversation_worktree(&self.conversation_id, commit).await;
        let mut answer: serde_json::Value = serde_json::from_str(&answer).map_err(|e| {
            SubagentError::from(format!(
                "the daemon's answer to the worktree reset was not JSON ({e}): {answer}"
            ))
        })?;
        if answer.get("is_error") == Some(&serde_json::Value::Bool(true)) {
            return Err(SubagentError::from(format!(
                "could not reset the conversation's worktree: {}",
                answer["error"].as_str().unwrap_or("unknown error")
            )));
        }
        match answer["reset"].take() {
            serde_json::Value::Null => Ok(None),
            reset => serde_json::from_value(reset).map(Some).map_err(|e| {
                SubagentError::from(format!("the daemon's worktree reset was malformed: {e}"))
            }),
        }
    }
}
