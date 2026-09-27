//! Integration tests: the Local `GREP` path returns the same context shapes the engine's does.
//!
//! The engine's path shells ripgrep and folds its `context` events; the Local path
//! (`CodebaseAccess::Local`) computes the same windows from the file's own lines. A caller must
//! not be able to tell which path answered — the shapes, the relations and the clamping at file
//! edges are the same contract.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Tool set (Grep context)

use tddy_discovery::subagent::{CodebaseAccess, GrepContext, GREP_CONTEXT_LINE_CEILING};
use tempfile::TempDir;

const A_FILE_BODY: &str = "line 1\nlet match_here = 1;\nline 3\nline 4\n";

/// A worktree holding one file with `A_FILE_BODY`, searched locally.
fn a_local_worktree() -> (TempDir, String) {
    let worktree = TempDir::new().expect("a worktree to search");
    let path = worktree.path().join("src/lib.rs");
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the tree");
    std::fs::write(&path, A_FILE_BODY).expect("the file to search");
    let path = path.to_string_lossy().into_owned();
    (worktree, path)
}

/// One match's context lines as `(line_number, text, relation)` triples.
fn context_triples(result: &serde_json::Value) -> Vec<(u64, String, String)> {
    result["matches"]
        .as_array()
        .expect("a matches array")
        .first()
        .expect("at least one match")["context"]
        .as_array()
        .expect("the match carries its context lines")
        .iter()
        .map(|line| {
            (
                line["lineNumber"].as_u64().expect("a line number"),
                line["text"].as_str().expect("line text").to_string(),
                line["relation"].as_str().expect("a relation").to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_local_grep_path_returns_the_same_context_shapes() {
    let (_worktree, path) = a_local_worktree();
    let result = CodebaseAccess::Local
        .grep_with_context(
            "match_here",
            Some(&path),
            None,
            GrepContext {
                before: 1,
                after: 2,
            },
        )
        .await
        .expect("the local grep runs");
    assert_eq!(
        context_triples(&result),
        vec![
            (1, "line 1".to_string(), "before".to_string()),
            (3, "line 3".to_string(), "after".to_string()),
            (4, "line 4".to_string(), "after".to_string()),
        ]
    );
}

#[tokio::test]
async fn the_local_path_clamps_its_context_at_file_edges() {
    let (_worktree, path) = a_local_worktree();
    let result = CodebaseAccess::Local
        .grep_with_context(
            "line 1",
            Some(&path),
            None,
            GrepContext {
                before: 5,
                after: 1,
            },
        )
        .await
        .expect("the local grep runs");
    assert_eq!(
        context_triples(&result),
        vec![(2, "let match_here = 1;".to_string(), "after".to_string())],
        "nothing before the file's first line, however many were asked for"
    );
}

#[tokio::test]
async fn the_local_path_without_context_keeps_its_unchanged_shape() {
    let (_worktree, path) = a_local_worktree();
    let result = CodebaseAccess::Local
        .grep_with_context(
            "match_here",
            Some(&path),
            None,
            GrepContext {
                before: 0,
                after: 0,
            },
        )
        .await
        .expect("the local grep runs");
    assert!(
        result["matches"]
            .as_array()
            .expect("a matches array")
            .first()
            .expect("a match")
            .get("context")
            .is_none(),
        "no context key when none was asked for"
    );
}

#[tokio::test]
async fn a_context_count_past_the_ceiling_is_rejected_rather_than_read() {
    let past = GREP_CONTEXT_LINE_CEILING + 1;
    let args = serde_json::json!({ "pattern": "x", "before": past });
    assert_eq!(
        GrepContext::from_args(&args),
        None,
        "a rejection rather than a clamp: the bound is the caller's to re-ask within, and a \
         clamped answer would report a window that was never requested"
    );
    assert_eq!(
        GrepContext::from_args(&serde_json::json!({ "pattern": "x" })),
        None,
        "no context arguments asked for none"
    );
    assert_eq!(
        GrepContext::from_args(&serde_json::json!({ "pattern": "x", "before": 1, "after": 2 })),
        Some(GrepContext {
            before: 1,
            after: 2
        })
    );
}
