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
        reset_from_answer(&answer)
    }
}

/// What the daemon's answer to a reset says: the reset it made (`None` when there was nothing to
/// take back), or why it could not.
fn reset_from_answer(answer: &str) -> Result<Option<WorktreeReset>, SubagentError> {
    let mut answer: serde_json::Value = serde_json::from_str(answer).map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_carrying_a_reset_yields_where_it_stands_and_what_it_dropped() {
        let answer = r#"{"reset":{"to":"c1","droppedCommits":["c2","c3"]}}"#;

        let reset = reset_from_answer(answer).unwrap();

        assert_eq!(
            reset,
            Some(WorktreeReset {
                to: "c1".to_string(),
                dropped_commits: vec!["c2".to_string(), "c3".to_string()],
            })
        );
    }

    #[test]
    fn an_answer_with_a_null_reset_yields_no_reset() {
        let reset = reset_from_answer(r#"{"reset":null}"#).unwrap();

        assert_eq!(reset, None);
    }

    #[test]
    fn an_error_answer_is_refused_naming_the_daemons_error() {
        let answer = r#"{"error":"abc is not a commit of the branch","is_error":true}"#;

        let refused = reset_from_answer(answer).unwrap_err();

        assert!(
            refused
                .to_string()
                .contains("abc is not a commit of the branch"),
            "the refusal must name the daemon's error, got: {refused}"
        );
    }

    #[test]
    fn an_answer_that_is_not_json_is_refused_quoting_it() {
        let refused = reset_from_answer("daemon unreachable").unwrap_err();

        assert!(
            refused.to_string().contains("daemon unreachable"),
            "the refusal must quote the answer, got: {refused}"
        );
    }

    #[test]
    fn a_reset_object_missing_its_fields_is_refused_as_malformed() {
        let refused = reset_from_answer(r#"{"reset":{"to":7}}"#).unwrap_err();

        assert!(
            refused.to_string().contains("malformed"),
            "the refusal must call the reset malformed, got: {refused}"
        );
    }
}
