//! Acceptance test: `execute_tool` dispatches file tools against a worktree root.

use std::path::PathBuf;

use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool, execute_tool_with_env, tool_catalog};

fn registry() -> TaskRegistry {
    TaskRegistry::new()
}

#[tokio::test]
async fn execute_tool_writes_then_reads_a_file_under_the_worktree_root() {
    // Given — a tempdir worktree root and a fresh task registry
    let tmp = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = tmp.path().to_path_buf();
    let registry = registry();
    let session_id = "test-session";

    // When — Write a file
    let write_outcome = execute_tool(
        &root,
        "Write",
        r#"{"path":"hello.txt","contents":"hi there"}"#,
        &registry,
        session_id,
    )
    .await;

    // Then — Write succeeds
    assert!(
        !write_outcome.is_error,
        "Write should succeed; got: {}",
        write_outcome.error_message
    );

    // And — Read returns the written contents
    let read_outcome = execute_tool(
        &root,
        "Read",
        r#"{"path":"hello.txt"}"#,
        &registry,
        session_id,
    )
    .await;
    assert!(
        !read_outcome.is_error,
        "Read should succeed; got: {}",
        read_outcome.error_message
    );
    let parsed: serde_json::Value = serde_json::from_str(&read_outcome.result_json).expect("json");
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("hi there"),
        "Read should return the written contents; got: {}",
        read_outcome.result_json
    );
}

#[tokio::test]
async fn execute_tool_rejects_a_path_that_escapes_the_worktree_root() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let registry = registry();

    // When — Read with a traversal path
    let outcome = execute_tool(
        &root,
        "Read",
        r#"{"path":"../../../etc/passwd"}"#,
        &registry,
        "test-session",
    )
    .await;

    // Then — rejected as an error (no silent fallback)
    assert!(outcome.is_error, "traversal must be rejected");
    assert!(
        outcome.error_message.contains("..") || outcome.error_message.contains("escapes"),
        "error should mention the traversal; got: {}",
        outcome.error_message
    );
}

#[tokio::test]
async fn execute_tool_returns_an_error_for_an_unknown_tool() {
    // Given
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let registry = registry();

    // When
    let outcome = execute_tool(&root, "NotARealTool", "{}", &registry, "test-session").await;

    // Then — honest error, no silent fallback
    assert!(outcome.is_error, "unknown tool must error");
    assert!(
        outcome.error_message.contains("unknown tool"),
        "error should mention unknown tool; got: {}",
        outcome.error_message
    );
}

/// SemanticSearch is only meaningful when the session has a semantic index. When it is invoked with
/// no available index (the DB env points at a path that does not exist), it must error rather than
/// silently degrading to a lexical/ripgrep search. See docs/ft/coder/semantic-index.md (criterion 15).
#[tokio::test]
async fn semantic_search_errors_when_no_index_is_available() {
    // Given — a worktree with searchable text but no populated semantic index
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::write(root.join("code.rs"), "fn find_me() {}").expect("write file");
    let missing_db = tmp.path().join("semantic-index.db");
    let registry = registry();

    // When — SemanticSearch runs with the index DB env pointing at a non-existent index
    let outcome = execute_tool_with_env(
        &root,
        "SemanticSearch",
        r#"{"query":"find_me"}"#,
        &registry,
        "test-session",
        &[(
            "TDDY_SEMANTIC_INDEX_DB".to_string(),
            missing_db.to_string_lossy().into_owned(),
        )],
    )
    .await;

    // Then — it errors instead of falling back to a lexical search
    assert!(
        outcome.is_error,
        "SemanticSearch must error without an index (no ripgrep fallback); got: {}",
        outcome.result_json
    );
}

#[test]
fn the_catalog_lists_every_tool_the_engine_dispatches() {
    // Given
    let catalog = tool_catalog();

    // Then — the core tools are present
    let names: Vec<&str> = catalog.iter().map(|t| t.name.as_str()).collect();
    for required in [
        "Read",
        "Write",
        "StrReplace",
        "Delete",
        "Grep",
        "Glob",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
    ] {
        assert!(
            names.contains(&required),
            "catalog must list {required}; got: {names:?}"
        );
    }
}
