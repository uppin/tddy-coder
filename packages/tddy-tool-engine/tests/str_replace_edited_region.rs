//! Integration tests: a successful `StrReplace` shows the caller what the file now says.
//!
//! Today it answers `{"replaced": true, "bytes_written": 40926}` — two numbers, neither of which
//! describes the text. A model editing through this tool is therefore writing blind, and in
//! session `01a0e285` that is exactly what went wrong: Gemma deleted a block, then tried to tidy
//! the blank lines the deletion had fused together, and guessed at how many there were:
//!
//! ```text
//! searched: '  }, [canScrollBack]);\n\n  // The "Load earlier output" affordance…'
//! actual:   '  }, [canScrollBack]);\n\n\n\n  // The "Load earlier output" affordance…'
//! ```
//!
//! Four newlines, not two — `old_string not found in file`. Five further attempts followed, and
//! the agent's own report claimed it had read the edited lines back when what it quoted came from
//! elsewhere in the file. Every one of those calls was a guess about text the tool had just
//! written and declined to show.
//!
//! Runs of blank lines are the worst case for reading back by eye, and the one where an editing
//! agent most needs to see bytes rather than count them. Showing the region costs a few lines;
//! not showing it cost six failed calls and a wrong answer about the file's own contents.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Remote daemon: tool execution

use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool, EDITED_REGION_CONTEXT_LINES};
use tempfile::TempDir;

const A_FILE: &str = "src/component.tsx";

/// A worktree holding `A_FILE` with `body`.
fn a_worktree_holding(body: &str) -> TempDir {
    let worktree = TempDir::new().expect("a worktree to edit");
    let path = worktree.path().join(A_FILE);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the source tree");
    std::fs::write(&path, body).expect("the file to edit");
    worktree
}

/// The shape of Gemma's edit: a deleted block leaves a run of blank lines behind it.
fn a_file_whose_deletion_left_blank_lines() -> String {
    "const a = 1;\n}, [canScrollBack]);\n\n\n\n  // The affordance\nconst b = 2;\n".to_string()
}

fn numbered_lines(count: usize) -> String {
    (1..=count)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn replace(worktree: &TempDir, old: &str, new: &str) -> serde_json::Value {
    let outcome = execute_tool(
        worktree.path(),
        "StrReplace",
        &serde_json::json!({ "path": A_FILE, "old_string": old, "new_string": new }).to_string(),
        &TaskRegistry::new(),
        "str-replace-region-test",
    )
    .await;
    assert!(
        !outcome.is_error,
        "StrReplace must succeed; got: {}",
        outcome.error_message
    );
    serde_json::from_str(&outcome.result_json).expect("a StrReplace result is JSON")
}

fn region_of(result: &serde_json::Value) -> &str {
    result["edited_region"].as_str().unwrap_or_else(|| {
        panic!("a successful StrReplace must show the region it wrote; got {result}")
    })
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The answer Gemma needed and never got: what the file says where the edit landed.
#[tokio::test]
async fn a_successful_replace_returns_the_region_it_wrote() {
    // Given a file with a known line in it
    let worktree = a_worktree_holding("const a = 1;\nconst target = 2;\nconst b = 3;\n");

    // When a line is replaced
    let result = replace(&worktree, "const target = 2;", "const renamed = 9;").await;

    // Then the region shows the new text
    assert!(
        region_of(&result).contains("const renamed = 9;"),
        "the region must show what was written; got: {}",
        region_of(&result)
    );
}

/// A region showing only the new text would be as blind as a byte count: what a caller needs is
/// the new text *in its surroundings*, which is how it tells whether the edit landed where it
/// meant it to.
#[tokio::test]
async fn the_region_shows_the_lines_around_the_edit_not_only_the_edit() {
    // Given a file with distinct neighbours on either side
    let worktree = a_worktree_holding("const before = 1;\nconst target = 2;\nconst after = 3;\n");

    // When the middle line is replaced
    let result = replace(&worktree, "const target = 2;", "const renamed = 9;").await;

    // Then both neighbours are visible
    let region = region_of(&result);
    assert!(
        region.contains("const before = 1;") && region.contains("const after = 3;"),
        "the region must carry the lines around the edit; got: {region}"
    );
}

/// The old text is gone from the file, so a region still showing it would describe a file that
/// no longer exists — the specific lie that would send an agent back round the loop.
#[tokio::test]
async fn the_region_does_not_show_the_text_that_was_replaced() {
    // Given a file with a known line in it
    let worktree = a_worktree_holding("const a = 1;\nconst target = 2;\nconst b = 3;\n");

    // When that line is replaced
    let result = replace(&worktree, "const target = 2;", "const renamed = 9;").await;

    // Then the old text is absent
    assert!(
        !region_of(&result).contains("const target = 2;"),
        "the region must describe the file as it is now; got: {}",
        region_of(&result)
    );
}

/// Gemma's exact situation, and the thing a byte count cannot express: how many blank lines a
/// deletion actually left. Counting them by eye is what failed; the region states them.
#[tokio::test]
async fn the_region_makes_a_run_of_blank_lines_visible_after_a_deletion() {
    // Given a file whose earlier deletion fused four newlines together
    let worktree = a_worktree_holding(&a_file_whose_deletion_left_blank_lines());

    // When the line above that run is edited
    let result = replace(&worktree, "const a = 1;", "const a = 99;").await;

    // Then the blank run below it is there to be counted
    assert!(
        region_of(&result).contains("}, [canScrollBack]);\n\n\n\n"),
        "an agent tidying whitespace has to see the whitespace; got: {:?}",
        region_of(&result)
    );
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// A whole file in the region would reintroduce the context cost the window exists to avoid.
#[tokio::test]
async fn the_region_is_bounded_by_the_context_line_budget() {
    // Given a long file edited in the middle
    let worktree = a_worktree_holding(&numbered_lines(500));

    // When one line is replaced
    let result = replace(&worktree, "line 250", "line 250 edited").await;

    // Then the region is the edit plus its budget on each side, and no more
    assert_eq!(
        region_of(&result).lines().count(),
        EDITED_REGION_CONTEXT_LINES * 2 + 1,
        "the region must be bounded; got: {:?}",
        region_of(&result)
    );
}

/// An edit on the first line has no lines above it. The region is whatever exists, not a panic
/// and not a padded window.
#[tokio::test]
async fn an_edit_on_the_first_line_returns_the_lines_that_exist_below_it() {
    // Given a file edited at its very start
    let worktree = a_worktree_holding(&numbered_lines(500));

    // When the first line is replaced
    //
    // Anchored with its newline: bare `line 1` is a prefix of `line 10`–`line 19` and
    // `line 100`–`line 199`, so it matches 111 times and `StrReplace` rightly refuses it.
    let result = replace(&worktree, "line 1\n", "line 1 edited\n").await;

    // Then the region begins at the edit and runs down
    let region = region_of(&result);
    assert!(
        region.starts_with("line 1 edited"),
        "a region clamped at the top of the file starts at the edit; got: {region:?}"
    );
}

/// The same at the other end — the case a naive `start + budget` slice panics on.
#[tokio::test]
async fn an_edit_on_the_last_line_returns_the_lines_that_exist_above_it() {
    // Given a file edited at its very end
    let worktree = a_worktree_holding(&numbered_lines(500));

    // When the last line is replaced
    let result = replace(&worktree, "line 500", "line 500 edited").await;

    // Then the region ends at the edit
    let region = region_of(&result);
    assert!(
        region.trim_end().ends_with("line 500 edited"),
        "a region clamped at the end of the file finishes at the edit; got: {region:?}"
    );
}

// ─── API boundaries ──────────────────────────────────────────────────────────

/// The existing fields are what every caller reads today; showing the region is an addition, not
/// a replacement.
#[tokio::test]
async fn the_result_still_carries_the_fields_it_always_did() {
    // Given a file with a known line in it
    let worktree = a_worktree_holding("const a = 1;\nconst target = 2;\n");

    // When a line is replaced
    let result = replace(&worktree, "const target = 2;", "const renamed = 9;").await;

    // Then `replaced` and `bytes_written` are unchanged in shape
    assert_eq!(result["replaced"], serde_json::json!(true));
    assert_eq!(
        result["bytes_written"].as_u64(),
        Some("const a = 1;\nconst renamed = 9;\n".len() as u64)
    );
}

/// A caller that wants to page around the edit needs to know where it is, and a region without a
/// line number cannot be located in the file it came from.
#[tokio::test]
async fn the_result_names_the_line_the_edit_starts_on() {
    // Given a file whose target sits on a known line
    let worktree = a_worktree_holding("const a = 1;\nconst target = 2;\nconst b = 3;\n");

    // When that line is replaced
    let result = replace(&worktree, "const target = 2;", "const renamed = 9;").await;

    // Then the 1-based line of the edit is reported
    assert_eq!(result["edited_line"].as_u64(), Some(2));
}
