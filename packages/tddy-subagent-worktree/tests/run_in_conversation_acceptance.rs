//! The per-call rule, driven by the real tool engine: where a conversation's call runs, and what a
//! mutating one leaves behind.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md

mod support;

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::json;
use support::{a_caller_worktree, conversation, short_head, CallerWorktree};
use tddy_subagent_worktree::{
    run_in_conversation, ConversationRun, FileCounts, LineCounts, WorktreeChange,
    SUBAGENT_WORKTREES_DIR,
};
use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool, ToolOutcome};

/// One conversation's calls, run through [`run_in_conversation`] with the real engine as executor.
struct AConversation<'a> {
    caller: &'a CallerWorktree,
    id: &'static str,
    registry: TaskRegistry,
}

fn a_conversation_over(caller: &CallerWorktree) -> AConversation<'_> {
    AConversation {
        caller,
        id: "explore",
        registry: TaskRegistry::new(),
    }
}

impl AConversation<'_> {
    async fn calls(&self, tool: &str, args: serde_json::Value) -> ConversationRun<ToolOutcome> {
        let args = args.to_string();
        run_in_conversation(
            &self.caller.conversations(),
            &conversation(self.id),
            tool,
            |root: PathBuf| async move {
                execute_tool(&root, tool, &args, &self.registry, support::SESSION_ID).await
            },
        )
        .await
        .expect("the conversation's git steps succeed")
    }

    fn worktree_root(&self) -> PathBuf {
        self.caller
            .path()
            .join(SUBAGENT_WORKTREES_DIR)
            .join(self.id)
    }
}

fn result_of(run: &ConversationRun<ToolOutcome>) -> serde_json::Value {
    serde_json::from_str(&run.output.result_json).expect("result JSON")
}

#[tokio::test]
async fn a_read_before_any_write_reads_the_session_worktree_and_creates_nothing() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "pub fn a() {}\n")
        .build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls("Read", json!({ "path": "src/lib.rs" }))
        .await;

    // Then
    assert_eq!(
        (
            run.output.is_error,
            run.output.result_json.contains("pub fn a() {}"),
            caller.exists(SUBAGENT_WORKTREES_DIR)
        ),
        (false, true, false)
    );
}

#[tokio::test]
async fn a_write_lands_in_the_conversation_worktree_and_reports_its_commit() {
    // Given
    let caller = a_caller_worktree().build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls(
            "Write",
            json!({ "path": "src/new.rs", "contents": "pub fn new() {}\n" }),
        )
        .await;

    // Then
    let root = conversation.worktree_root();
    assert_eq!(
        (
            std::fs::read_to_string(root.join("src/new.rs")).unwrap(),
            caller.exists("src/new.rs"),
            run.change,
        ),
        (
            "pub fn new() {}\n".to_string(),
            false,
            Some(WorktreeChange {
                commit: Some(short_head(&root)),
                files: FileCounts {
                    created: 1,
                    updated: 0,
                    removed: 0
                },
                lines: LineCounts {
                    added: 1,
                    removed: 0
                },
            })
        )
    );
}

#[tokio::test]
async fn a_read_after_a_write_sees_the_subagents_own_write() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "before\n")
        .build();
    let conversation = a_conversation_over(&caller);
    conversation
        .calls(
            "Write",
            json!({ "path": "src/lib.rs", "contents": "after\n" }),
        )
        .await;

    // When
    let run = conversation
        .calls("Read", json!({ "path": "src/lib.rs" }))
        .await;

    // Then
    assert_eq!(
        (
            run.output.result_json.contains("after"),
            caller.read("src/lib.rs")
        ),
        (true, "before\n".to_string())
    );
}

#[tokio::test]
async fn a_shell_call_that_creates_files_reports_them() {
    // Given
    let caller = a_caller_worktree().build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls(
            "Shell",
            json!({ "command": "mkdir -p gen && printf 'a\\nb\\n' > gen/one.txt && printf 'c\\n' > gen/two.txt" }),
        )
        .await;

    // Then
    assert_eq!(
        run.change.map(|change| (change.files, change.lines)),
        Some((
            FileCounts {
                created: 2,
                updated: 0,
                removed: 0
            },
            LineCounts {
                added: 3,
                removed: 0
            }
        ))
    );
}

#[tokio::test]
async fn a_delete_reports_a_removed_file() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/old.rs", "one\ntwo\n")
        .build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls("Delete", json!({ "path": "src/old.rs" }))
        .await;

    // Then
    assert_eq!(
        (
            run.change.map(|change| (change.files, change.lines)),
            caller.read("src/old.rs")
        ),
        (
            Some((
                FileCounts {
                    created: 0,
                    updated: 0,
                    removed: 1
                },
                LineCounts {
                    added: 0,
                    removed: 2
                }
            )),
            "one\ntwo\n".to_string()
        )
    );
}

#[tokio::test]
async fn await_is_treated_as_mutating_and_commits_what_the_background_job_wrote() {
    // Given
    let caller = a_caller_worktree().build();
    let conversation = a_conversation_over(&caller);
    let started = conversation
        .calls(
            "Shell",
            json!({ "command": "sleep 0.2 && printf 'late\\n' > late.txt", "block_until_ms": 0 }),
        )
        .await;
    let job_id = started.output.job_id.clone();

    // When
    let awaited = conversation
        .calls("Await", json!({ "job_id": job_id, "timeout_ms": 10_000 }))
        .await;

    // Then
    assert_eq!(
        awaited.change.map(|change| change.files),
        Some(FileCounts {
            created: 1,
            updated: 0,
            removed: 0
        })
    );
}

#[tokio::test]
async fn a_read_only_call_carries_no_worktree_change() {
    // Given
    let caller = a_caller_worktree()
        .with_committed_file("src/lib.rs", "x\n")
        .build();
    let conversation = a_conversation_over(&caller);
    conversation
        .calls("Write", json!({ "path": "a.txt", "contents": "a\n" }))
        .await;

    // When
    let run = conversation.calls("Grep", json!({ "pattern": "x" })).await;

    // Then
    assert_eq!(run.change, None);
}

#[tokio::test]
async fn a_mutating_call_that_changed_nothing_reports_zero_counts_without_a_commit() {
    // Given
    let caller = a_caller_worktree().build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls("Shell", json!({ "command": "true" }))
        .await;

    // Then
    assert_eq!(run.change, Some(WorktreeChange::default()));
}

#[tokio::test]
async fn the_tool_result_itself_is_passed_through_unchanged() {
    // Given
    let caller = a_caller_worktree().build();
    let conversation = a_conversation_over(&caller);

    // When
    let run = conversation
        .calls("Write", json!({ "path": "a.txt", "contents": "abc" }))
        .await;

    // Then
    assert_eq!(result_of(&run)["bytes_written"], json!(3));
}
