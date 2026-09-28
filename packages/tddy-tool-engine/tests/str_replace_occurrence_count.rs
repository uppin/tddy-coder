//! Integration tests: `StrReplace` reports how many occurrences its `old_string` matched.
//!
//! The engine counts occurrences internally already — `content.matches(old_string).count()` —
//! but only to enforce uniqueness, and discards the count. A caller deciding whether an edit
//! landed (or a turn condition deciding whether it did not) has to infer it from `replaced`
//! alone, which says only that the edit was unique and applied.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control (result summaries)

use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;
use tempfile::TempDir;

const A_FILE: &str = "src/lib.rs";

/// A worktree holding `A_FILE` with `body`.
fn a_worktree_holding(body: &str) -> TempDir {
    let worktree = TempDir::new().expect("a worktree to edit");
    let path = worktree.path().join(A_FILE);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the source tree");
    std::fs::write(&path, body).expect("the file to edit");
    worktree
}

/// A registry the tool's background jobs can land in, the same shape every engine test hands
/// `execute_tool`.
fn a_task_registry() -> TaskRegistry {
    TaskRegistry::default()
}

#[tokio::test]
async fn str_replace_returns_its_occurrence_count() {
    let worktree = a_worktree_holding("let a = 1;\nlet b = 2;\n");
    let outcome = execute_tool(
        worktree.path(),
        "StrReplace",
        &serde_json::json!({
            "path": A_FILE,
            "old_string": "let a = 1;",
            "new_string": "let a = 0;"
        })
        .to_string(),
        &a_task_registry(),
        "str-replace-occurrence-test",
    )
    .await;
    let result: serde_json::Value =
        serde_json::from_str(&outcome.result_json).expect("a JSON tool result");
    assert_eq!(result["replaced"], serde_json::json!(true));
    assert_eq!(
        result["matchedOccurrences"],
        serde_json::json!(1),
        "the count the engine already computes internally, reported"
    );
}

#[tokio::test]
async fn a_str_replace_that_matches_nothing_reports_zero_occurrences() {
    let worktree = a_worktree_holding("let a = 1;\nlet b = 2;\n");
    let outcome = execute_tool(
        worktree.path(),
        "StrReplace",
        &serde_json::json!({
            "path": A_FILE,
            "old_string": "let c = 3;",
            "new_string": "let c = 0;"
        })
        .to_string(),
        &a_task_registry(),
        "str-replace-occurrence-test",
    )
    .await;
    let result: serde_json::Value =
        serde_json::from_str(&outcome.result_json).expect("a JSON tool result");
    assert_eq!(
        result["matchedOccurrences"],
        serde_json::json!(0),
        "a no-match edit reports zero, not an absence a caller must guess at"
    );
}
