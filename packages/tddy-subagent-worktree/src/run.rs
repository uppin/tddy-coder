//! The per-call rule, in one place: which root a conversation's tool call runs at, and what
//! follows a mutating one.

use std::future::Future;
use std::path::PathBuf;

use crate::change_facts::WorktreeChange;
use crate::conversation_id::ConversationId;
use crate::tool_effect::ToolEffect;
use crate::worktree::{ConversationWorktrees, WorktreeError};

/// The key a call's result carries its [`WorktreeChange`] under — or, when the call ran but its
/// commit failed, `{"error": …}`.
pub const WORKTREE_CHANGE_KEY: &str = "worktreeChange";

/// What one call produced, and what it changed.
#[derive(Debug)]
pub struct ConversationRun<T> {
    /// Whatever the executor returned.
    pub output: T,
    /// Present for every [`ToolEffect::Mutating`] call whose commit succeeded; `None` for a read.
    pub change: Option<WorktreeChange>,
    /// Why recording a mutating call that already ran failed. The tool's `output` is still the
    /// truth about what it did — its effects are on disk, uncommitted — so it is carried, not
    /// dropped for the error.
    pub commit_error: Option<String>,
}

impl<T> ConversationRun<T> {
    /// `result_json` as the caller should see it: with the call's [`WorktreeChange`] merged in, or
    /// with `"worktreeChange": {"error": …}` when the commit failed, or untouched for a read.
    pub fn merge(&self, result_json: &str) -> String {
        match (&self.change, &self.commit_error) {
            (Some(change), _) => with_worktree_change(result_json, change),
            (None, Some(error)) => with_worktree_value(
                result_json,
                serde_json::json!({ "error": format!("the call ran, but its worktree commit failed: {error}") }),
            ),
            (None, None) => result_json.to_string(),
        }
    }
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
    match ToolEffect::of(tool_name) {
        ToolEffect::ReadOnly => {
            let root = match worktrees.existing(conversation).await? {
                Some(worktree) => worktree.root().to_path_buf(),
                None => worktrees.session_worktree().to_path_buf(),
            };
            Ok(ConversationRun {
                output: execute(root).await,
                change: None,
                commit_error: None,
            })
        }
        ToolEffect::Mutating => {
            let worktree = worktrees.ensure(conversation).await?;
            let output = execute(worktree.root().to_path_buf()).await;
            // The tool has already run: a commit failure is reported beside its output, never
            // instead of it.
            let (change, commit_error) = match worktree.commit_changes(tool_name).await {
                Ok(change) => (Some(change), None),
                Err(error) => (None, Some(error.to_string())),
            };
            Ok(ConversationRun {
                output,
                change,
                commit_error,
            })
        }
    }
}

/// `result_json` with `"worktreeChange": change` added to its top-level object. A result that is
/// not a JSON object is wrapped as `{"result": <it>, "worktreeChange": …}` so the change is never
/// dropped.
pub fn with_worktree_change(result_json: &str, change: &WorktreeChange) -> String {
    let change = serde_json::to_value(change).expect("a WorktreeChange serializes to JSON");
    with_worktree_value(result_json, change)
}

fn with_worktree_value(result_json: &str, change: serde_json::Value) -> String {
    let mut object = match serde_json::from_str::<serde_json::Value>(result_json) {
        Ok(serde_json::Value::Object(object)) => object,
        Ok(other) => serde_json::Map::from_iter([("result".to_string(), other)]),
        // Text that is not JSON is still a result; keep it as a string.
        Err(_) => serde_json::Map::from_iter([(
            "result".to_string(),
            serde_json::Value::String(result_json.to_string()),
        )]),
    };
    object.insert(WORKTREE_CHANGE_KEY.to_string(), change);
    serde_json::Value::Object(object).to_string()
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
