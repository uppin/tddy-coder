//! Integration tests: the engine's `Glob` and `Grep` honour a result window and report what they
//! left out.
//!
//! The same shape as `read_window_engine_red.rs`, one tool along. There, `catalog.rs` advertised
//! `offset`/`limit` on `Read` and `tool_read` read neither, so the contract was green while half
//! of it was missing. Here neither half exists at all: `tool_glob` collects every match into one
//! answer, `Grep` does the same, and the catalog schemas offer no way to ask for fewer.
//!
//! Session 01a0e200 is the bill. A `Glob **/*` from a jailed subagent returned **408,282 bytes**
//! — the whole repository tree — and a `Grep` in the same turn returned 180,475. Both crossed the
//! wire in full before anything on the subagent side could trim them, which is exactly why the
//! window has to be honoured *here*: a cap applied after the transfer bounds the model's context
//! and not the wire, and `packages/tddy-discovery/tests/subagent_search_result_cap.rs` pins
//! the sending side that this half has to answer.
//!
//! `truncated` and the true total are not decoration. Without them a capped answer and a complete
//! one are the same JSON, so a caller cannot tell a finished search from a clipped one — and a
//! model cannot page.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Remote daemon: tool execution

use tddy_task::TaskRegistry;
use tddy_tool_engine::{execute_tool, tool_catalog};
use tempfile::TempDir;

const A_NEEDLE: &str = "PAGE_SCROLLBACK";

// ─── Fixtures ────────────────────────────────────────────────────────────────

/// A worktree of `count` files named `file-000.txt` … so glob order is lexicographic and a capped
/// prefix is exactly the first `n`.
fn a_worktree_of_files(count: usize) -> TempDir {
    let worktree = TempDir::new().expect("a worktree to search");
    for i in 0..count {
        std::fs::write(worktree.path().join(format!("file-{i:03}.txt")), "x")
            .expect("a file to match");
    }
    worktree
}

/// A worktree holding one file whose `count` lines all match [`A_NEEDLE`].
fn a_worktree_with_matching_lines(count: usize) -> TempDir {
    let worktree = TempDir::new().expect("a worktree to search");
    let body: String = (0..count)
        .map(|i| format!("const {A_NEEDLE}_{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(worktree.path().join("constants.ts"), body).expect("a file to match");
    worktree
}

async fn run(worktree: &TempDir, tool: &str, args: serde_json::Value) -> SearchResult {
    let outcome = execute_tool(
        worktree.path(),
        tool,
        &args.to_string(),
        &TaskRegistry::new(),
        "search-window-test",
    )
    .await;
    assert!(
        !outcome.is_error,
        "{tool} must succeed, got: {}",
        outcome.error_message
    );
    SearchResult {
        value: serde_json::from_str(&outcome.result_json).expect("a search result is JSON"),
    }
}

// ─── Fluent assertions ───────────────────────────────────────────────────────

struct SearchResult {
    value: serde_json::Value,
}

impl SearchResult {
    fn assert_returned(&self, field: &str, expected: usize) -> &Self {
        let got = self.value[field]
            .as_array()
            .unwrap_or_else(|| panic!("a search result carries '{field}'; got {}", self.value))
            .len();
        assert_eq!(got, expected, "wrong number of '{field}' returned");
        self
    }

    fn assert_truncated(&self, expected: bool) -> &Self {
        assert_eq!(
            self.value["truncated"],
            serde_json::json!(expected),
            "a windowed search must report whether more results follow"
        );
        self
    }

    fn assert_total(&self, field: &str, expected: usize) -> &Self {
        assert_eq!(
            self.value[field],
            serde_json::json!(expected),
            "a windowed search must report the true size so the caller can page"
        );
        self
    }
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The window the engine has never honoured. Without it, `limit` in the request is a suggestion.
#[tokio::test]
async fn globbing_with_a_limit_returns_only_that_many_paths() {
    // Given a worktree of 250 files
    let worktree = a_worktree_of_files(250);

    // When ten are asked for
    let result = run(
        &worktree,
        "Glob",
        serde_json::json!({ "pattern": "*.txt", "limit": 10 }),
    )
    .await;

    // Then ten come back, and the real size is reported alongside
    result
        .assert_returned("paths", 10)
        .assert_truncated(true)
        .assert_total("total_paths", 250);
}

/// A capped answer and a complete one must not be the same JSON.
#[tokio::test]
async fn a_glob_whose_limit_covers_every_match_is_not_reported_as_truncated() {
    // Given a worktree of three files
    let worktree = a_worktree_of_files(3);

    // When ten are asked for
    let result = run(
        &worktree,
        "Glob",
        serde_json::json!({ "pattern": "*.txt", "limit": 10 }),
    )
    .await;

    // Then all three come back, uncut
    result
        .assert_returned("paths", 3)
        .assert_truncated(false)
        .assert_total("total_paths", 3);
}

/// `Grep` returned 180 KB in the same turn as the 408 KB glob, and needs the same window.
#[tokio::test]
async fn grepping_with_a_limit_returns_only_that_many_matches() {
    // Given a file with 250 matching lines
    let worktree = a_worktree_with_matching_lines(250);

    // When ten are asked for
    let result = run(
        &worktree,
        "Grep",
        serde_json::json!({ "pattern": A_NEEDLE, "limit": 10 }),
    )
    .await;

    // Then ten come back, and the real size is reported alongside
    result
        .assert_returned("matches", 10)
        .assert_truncated(true)
        .assert_total("total_matches", 250);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// The engine keeps its own default off, exactly as `line_window` does: what a tool call returns
/// and how much of it an agent may pull into a model context are different questions, and the
/// main agent has always received every match.
#[tokio::test]
async fn a_glob_with_no_limit_returns_every_match_as_it_always_has() {
    // Given a worktree of 250 files
    let worktree = a_worktree_of_files(250);

    // When no limit is given
    let result = run(&worktree, "Glob", serde_json::json!({ "pattern": "*.txt" })).await;

    // Then everything comes back, and nothing claims to have been cut
    result
        .assert_returned("paths", 250)
        .assert_truncated(false)
        .assert_total("total_paths", 250);
}

/// A limit of zero is a caller asking how big the answer is without paying for it.
#[tokio::test]
async fn a_glob_limited_to_zero_returns_no_paths_but_still_reports_the_total() {
    // Given a worktree of 250 files
    let worktree = a_worktree_of_files(250);

    // When none are asked for
    let result = run(
        &worktree,
        "Glob",
        serde_json::json!({ "pattern": "*.txt", "limit": 0 }),
    )
    .await;

    // Then the count is available without the payload
    result
        .assert_returned("paths", 0)
        .assert_truncated(true)
        .assert_total("total_paths", 250);
}

// ─── API boundaries ──────────────────────────────────────────────────────────

/// A window absent from the catalog is a window no caller will send — the half of the `Read`
/// contract that *was* right, and is missing here.
#[test]
fn the_glob_catalog_entry_advertises_the_limit_it_honours() {
    // Given the engine's tool catalog
    let catalog = tool_catalog();

    // When the Glob entry is read
    let glob = catalog
        .iter()
        .find(|tool| tool.name == "Glob")
        .expect("Glob is in the exec catalog");
    let schema: serde_json::Value =
        serde_json::from_str(&glob.input_schema_json).expect("a catalog schema is JSON");

    // Then it offers a limit
    assert_eq!(
        schema["properties"]["limit"]["type"],
        serde_json::json!("integer")
    );
}

/// Same for `Grep` — the tool that returned 180 KB.
#[test]
fn the_grep_catalog_entry_advertises_the_limit_it_honours() {
    // Given the engine's tool catalog
    let catalog = tool_catalog();

    // When the Grep entry is read
    let grep = catalog
        .iter()
        .find(|tool| tool.name == "Grep")
        .expect("Grep is in the exec catalog");
    let schema: serde_json::Value =
        serde_json::from_str(&grep.input_schema_json).expect("a catalog schema is JSON");

    // Then it offers a limit
    assert_eq!(
        schema["properties"]["limit"]["type"],
        serde_json::json!("integer")
    );
}
