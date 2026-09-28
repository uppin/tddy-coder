//! Integration tests: `Grep` returns the lines around each of its matches when asked.
//!
//! A match without its context costs a second `READ` of the whole file — the signature above, the
//! body below, are why anyone greps in the first place. ripgrep already emits `context` events
//! for `-B`/`-A`; the tool just never passed the flags or kept the events.
//!
//! The match window keeps counting **matches** — context lines never consume it — so a caller's
//! windowing logic does not change shape when it asks for context.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Tool set (Grep context)

use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;
use tempfile::TempDir;

const A_FILE: &str = "src/lib.rs";
const A_BODY: &str = "line 1\nlet match_here = 1;\nline 3\nline 4\n";

/// A worktree holding `A_FILE` with `A_BODY`.
fn a_worktree() -> TempDir {
    let worktree = TempDir::new().expect("a worktree to search");
    let path = worktree.path().join(A_FILE);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the source tree");
    std::fs::write(&path, A_BODY).expect("the file to search");
    worktree
}

async fn a_grep(args: serde_json::Value) -> serde_json::Value {
    let outcome = execute_tool(
        a_worktree().path(),
        "Grep",
        &args.to_string(),
        &TaskRegistry::default(),
        "grep-context-test",
    )
    .await;
    assert!(
        !outcome.is_error,
        "the grep ran: {}",
        &outcome.error_message
    );
    serde_json::from_str(&outcome.result_json).expect("a JSON grep result")
}

fn the_first_match(result: &serde_json::Value) -> &serde_json::Value {
    result["matches"]
        .as_array()
        .expect("a matches array")
        .first()
        .expect("at least one match")
}

#[tokio::test]
async fn a_grep_with_before_and_after_returns_context_lines_per_match() {
    let result = a_grep(serde_json::json!({
        "pattern": "match_here", "path": A_FILE, "before": 1, "after": 2
    }))
    .await;
    let context = the_first_match(&result)["context"]
        .as_array()
        .expect("the match carries its context lines")
        .clone();
    let as_triples: Vec<(u64, String, String)> = context
        .iter()
        .map(|line| {
            (
                line["lineNumber"].as_u64().expect("a line number"),
                line["text"].as_str().expect("line text").to_string(),
                line["relation"].as_str().expect("a relation").to_string(),
            )
        })
        .collect();
    assert_eq!(
        as_triples,
        vec![
            (1, "line 1".to_string(), "before".to_string()),
            (3, "line 3".to_string(), "after".to_string()),
            (4, "line 4".to_string(), "after".to_string()),
        ]
    );
}

#[tokio::test]
async fn context_lines_are_clamped_at_file_edges() {
    let result = a_grep(serde_json::json!({
        "pattern": "line 1", "path": A_FILE, "before": 5, "after": 1
    }))
    .await;
    let context = the_first_match(&result)["context"]
        .as_array()
        .expect("the match carries its context lines");
    // The match is the file's first line: nothing before it, one line after — fewer than asked,
    // not an error and not padding.
    assert_eq!(context.len(), 1);
    assert_eq!(context[0]["relation"], serde_json::json!("after"));
}

#[tokio::test]
async fn a_grep_without_context_arguments_returns_its_unchanged_shape() {
    let result = a_grep(serde_json::json!({ "pattern": "match_here", "path": A_FILE })).await;
    assert!(
        the_first_match(&result).get("context").is_none(),
        "no context key when none was asked for"
    );
    assert_eq!(result["total_matches"], serde_json::json!(1));
}

#[tokio::test]
async fn the_match_window_counts_matches_not_context_lines() {
    let result = a_grep(serde_json::json!({
        "pattern": "line", "path": A_FILE, "before": 5, "after": 5, "limit": 1
    }))
    .await;
    // Three real matches ("line" is not in line 2) — a window never counts context lines toward it.
    assert_eq!(result["total_matches"], serde_json::json!(3));
    assert_eq!(
        result["matches"].as_array().expect("a window").len(),
        1,
        "the window holds one match, however many context lines it carries"
    );
}
