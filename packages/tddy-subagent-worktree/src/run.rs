//! The per-call rule, in one place: which root a conversation's tool call runs at, and what
//! follows a mutating one.

use std::future::Future;
use std::path::PathBuf;

use crate::change_facts::WorktreeChange;
use crate::conversation_id::ConversationId;
use crate::tool_effect::ToolEffect;
use crate::worktree::{ConversationWorktrees, WorktreeError};

/// What one call produced, and what it changed.
#[derive(Debug)]
pub struct ConversationRun<T> {
    /// Whatever the executor returned.
    pub output: T,
    /// Present for every [`ToolEffect::Mutating`] call; `None` for a read.
    pub change: Option<WorktreeChange>,
}

/// Run one tool call of `conversation`:
///
/// - a **read-only** call runs in the conversation's worktree if it has one, else in the session
///   worktree — a conversation that only reads never creates one;
/// - a **mutating** call creates the worktree if needed, runs in it, and is followed by
///   [`ConversationWorktree::commit_changes`](crate::ConversationWorktree::commit_changes) with the
///   tool's name as the subject.
///
/// `execute` receives the root to run at.
pub async fn run_in_conversation<T, F, Fut>(
    worktrees: &ConversationWorktrees,
    conversation: &ConversationId,
    tool_name: &str,
    execute: F,
) -> Result<ConversationRun<T>, WorktreeError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = T>,
{
    // TODO(isolated-edits): implement
    let _ = (worktrees, conversation, execute, ToolEffect::of);
    todo!("run_in_conversation({tool_name:?})")
}

/// `result_json` with `"worktreeChange": change` added to its top-level object. A result that is
/// not a JSON object is wrapped as `{"result": <it>, "worktreeChange": …}` so the change is never
/// dropped.
pub fn with_worktree_change(result_json: &str, change: &WorktreeChange) -> String {
    // TODO(isolated-edits): implement
    todo!("with_worktree_change({result_json:?}, {change:?})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_facts::{FileCounts, LineCounts};

    fn a_change() -> WorktreeChange {
        WorktreeChange {
            commit: Some("3f9c2ab".into()),
            files: FileCounts {
                created: 1,
                updated: 0,
                removed: 0,
            },
            lines: LineCounts {
                added: 3,
                removed: 0,
            },
        }
    }

    #[test]
    fn the_change_is_added_beside_the_tools_own_fields() {
        let merged = with_worktree_change(r#"{"bytes_written":12}"#, &a_change());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&merged).unwrap(),
            serde_json::json!({
                "bytes_written": 12,
                "worktreeChange": {
                    "commit": "3f9c2ab",
                    "files": { "created": 1, "updated": 0, "removed": 0 },
                    "lines": { "added": 3, "removed": 0 }
                }
            })
        );
    }

    #[test]
    fn a_result_that_is_not_an_object_is_wrapped_so_the_change_is_kept() {
        let merged = with_worktree_change(r#""plain text""#, &a_change());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&merged).unwrap()["result"],
            serde_json::json!("plain text")
        );
    }
}
