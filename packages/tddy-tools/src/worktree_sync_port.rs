//! The sync port an in-process subagent conversation is given: before every turn it asks the
//! facilitating daemon to merge the caller's current files into the conversation's worktree, over
//! the same transport its tool calls use.

use async_trait::async_trait;
use tddy_discovery::subagent::{SubagentError, SyncAnswer, WorktreeSyncPort};

/// Syncs one conversation's worktree through `ConversationWorktree { sync }`.
#[allow(dead_code)] // TODO(caller-sync): given to each Managed conversation in `subagent_config_for_conversation`
pub(crate) struct ConversationWorktreeSyncPort {
    conversation_id: String,
}

#[allow(dead_code)] // TODO(caller-sync): see the struct
impl ConversationWorktreeSyncPort {
    pub(crate) fn new(conversation_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
        }
    }
}

#[async_trait]
impl WorktreeSyncPort for ConversationWorktreeSyncPort {
    async fn sync(&self) -> Result<SyncAnswer, SubagentError> {
        // TODO(caller-sync): `tddy_session_tool_client::sync_conversation_worktree` → `sync_from_answer`
        let _ = &self.conversation_id;
        todo!("ConversationWorktreeSyncPort::sync")
    }
}

/// What the daemon's answer to a sync says: `{"sync": {…}}` merged, `{"sync": null}` nothing,
/// `{"conflicts": [...]}` conflicted, an `is_error` body an error.
#[allow(dead_code)] // TODO(caller-sync): read by `sync`
fn sync_from_answer(answer: &str) -> Result<SyncAnswer, SubagentError> {
    // TODO(caller-sync): implement
    let _ = answer;
    todo!("sync_from_answer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_discovery::subagent::{FileCounts, LineCounts, WorktreeSync};

    #[test]
    fn an_answer_carrying_a_sync_yields_the_merge() {
        let answer = r#"{"sync":{"commit":"7d1e0aa","files":{"created":0,"updated":1,"removed":0},
            "lines":{"added":1,"removed":0},"paths":["src/lib.rs"],"morePaths":0}}"#;

        assert_eq!(
            sync_from_answer(answer).unwrap(),
            SyncAnswer::Merged(WorktreeSync {
                commit: "7d1e0aa".to_string(),
                files: FileCounts {
                    created: 0,
                    updated: 1,
                    removed: 0
                },
                lines: LineCounts {
                    added: 1,
                    removed: 0
                },
                paths: vec!["src/lib.rs".to_string()],
                more_paths: 0,
            })
        );
    }

    #[test]
    fn an_answer_with_a_null_sync_yields_nothing() {
        assert_eq!(
            sync_from_answer(r#"{"sync":null}"#).unwrap(),
            SyncAnswer::Nothing
        );
    }

    #[test]
    fn an_answer_naming_conflicts_yields_them() {
        assert_eq!(
            sync_from_answer(r#"{"conflicts":["src/lib.rs","README.md"]}"#).unwrap(),
            SyncAnswer::Conflicted(vec!["src/lib.rs".to_string(), "README.md".to_string()])
        );
    }

    #[test]
    fn an_error_answer_is_an_error_naming_the_cause() {
        let refused = sync_from_answer(r#"{"error":"no transport","is_error":true}"#).unwrap_err();

        assert_eq!(
            refused.to_string(),
            "could not sync the conversation's worktree with the caller: no transport"
        );
    }
}
