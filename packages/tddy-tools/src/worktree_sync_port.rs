//! The sync port an in-process subagent conversation is given: before every turn it asks the
//! facilitating daemon to merge the caller's current files into the conversation's worktree, over
//! the same transport its tool calls use.

use async_trait::async_trait;
use serde::Deserialize;
use tddy_discovery::subagent::{SubagentError, SyncAnswer, WorktreeSyncPort};
use tddy_session_tool_client::sync_conversation_worktree;

use crate::worktree_answer::WorktreeOperation;

/// Syncs one conversation's worktree through `ConversationWorktree { sync }`.
pub(crate) struct ConversationWorktreeSyncPort {
    conversation_id: String,
}

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
        let answer = sync_conversation_worktree(&self.conversation_id).await;
        sync_from_answer(&answer)
    }
}

/// How the sync's answer and its refusals are named.
const SYNC: WorktreeOperation = WorktreeOperation {
    name: "sync",
    failure: "could not sync the conversation's worktree with the caller",
};

/// The daemon's conflict answer: the conflicted paths it names (at most `SYNC_NOTICE_PATHS`) and how
/// many more it left out.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conflicts {
    conflicts: Vec<String>,
    more_conflicts: usize,
}

/// What the daemon's answer to a sync says: `{"sync": {…}}` merged, `{"sync": null}` nothing,
/// `{"conflicts": [...], "moreConflicts": n}` conflicted, an `is_error` body an error.
fn sync_from_answer(answer: &str) -> Result<SyncAnswer, SubagentError> {
    let mut answer = SYNC.parse(answer)?;
    if answer.get("conflicts").is_some() {
        let Conflicts {
            conflicts,
            more_conflicts,
        } = serde_json::from_value(answer).map_err(|e| SYNC.malformed(e))?;
        return Ok(SyncAnswer::Conflicted {
            paths: conflicts,
            more_paths: more_conflicts,
        });
    }
    match answer.get_mut("sync").map(serde_json::Value::take) {
        Some(serde_json::Value::Null) => Ok(SyncAnswer::Nothing),
        Some(sync) => serde_json::from_value(sync)
            .map(SyncAnswer::Merged)
            .map_err(|e| SYNC.malformed(e)),
        None => Err(SubagentError::from(format!(
            "the daemon's answer to the worktree sync named neither a sync nor conflicts: {answer}"
        ))),
    }
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
    fn an_answer_naming_conflicts_yields_them_and_the_count_left_out() {
        assert_eq!(
            sync_from_answer(r#"{"conflicts":["src/lib.rs","README.md"],"moreConflicts":4}"#)
                .unwrap(),
            SyncAnswer::Conflicted {
                paths: vec!["src/lib.rs".to_string(), "README.md".to_string()],
                more_paths: 4,
            }
        );
    }

    #[test]
    fn conflicts_without_their_left_out_count_are_refused_as_malformed() {
        let refused = sync_from_answer(r#"{"conflicts":["src/lib.rs"]}"#).unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the daemon's worktree sync was malformed: missing field `moreConflicts`"
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

    #[test]
    fn an_error_answer_without_an_error_is_an_unknown_error() {
        let refused = sync_from_answer(r#"{"is_error":true}"#).unwrap_err();

        assert_eq!(
            refused.to_string(),
            "could not sync the conversation's worktree with the caller: unknown error"
        );
    }

    #[test]
    fn an_answer_that_is_not_json_is_refused_quoting_it() {
        let refused = sync_from_answer("daemon unreachable").unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the daemon's answer to the worktree sync was not JSON (expected value at line 1 \
             column 1): daemon unreachable"
        );
    }

    #[test]
    fn an_answer_naming_neither_a_sync_nor_conflicts_is_refused() {
        let refused = sync_from_answer(r#"{"pulled":null}"#).unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the daemon's answer to the worktree sync named neither a sync nor conflicts: \
             {\"pulled\":null}"
        );
    }

    #[test]
    fn a_sync_object_missing_its_fields_is_refused_as_malformed() {
        let refused = sync_from_answer(r#"{"sync":{"commit":7}}"#).unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the daemon's worktree sync was malformed: invalid type: integer `7`, expected a string"
        );
    }
}
