//! Integration tests: the engine's `Read` honours the line window it has always advertised.
//!
//! `catalog.rs` has offered `offset` and `limit` on `Read` since the catalog was written, and
//! `tool_read` reads neither. The sending side of that contract *is* tested —
//! `packages/tddy-discovery/tests/read_window_red.rs` asserts a managed subagent forwards both to
//! the daemon — so the whole contract has been green while half of it was missing.
//!
//! The consequence is not merely wasted bytes. `CodebaseAccess::read_window` applies its
//! `DEFAULT_READ_LINE_CAP` of 200 lines only on the Local path; on the Managed path it forwards
//! the window and trusts the answer. So a jailed subagent — every subagent, in practice — has no
//! read cap at all, and pulls whole files into a 32k context. In the 2026-09-26 session one file
//! crossed that boundary twice at 42 KB a time, with `offset` and `limit` set on both calls.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Remote daemon: tool execution

use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;
use tempfile::TempDir;

const A_FILE: &str = "src/terminal.rs";

struct AReadCall {
    worktree: TempDir,
    offset: Option<u64>,
    limit: Option<u64>,
}

/// A worktree holding one file of `line_count` numbered lines: `line 1`, `line 2`, …
fn a_read_of_a_file_with_lines(line_count: usize) -> AReadCall {
    let worktree = TempDir::new().expect("a worktree to read from");
    let path = worktree.path().join(A_FILE);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the source tree");
    std::fs::write(&path, numbered_lines(line_count)).expect("the file to read");
    AReadCall {
        worktree,
        offset: None,
        limit: None,
    }
}

fn numbered_lines(count: usize) -> String {
    (1..=count)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl AReadCall {
    fn starting_at_line(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    fn of_at_most(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    async fn run(&self) -> ReadResult {
        let mut args = serde_json::json!({ "path": A_FILE });
        if let Some(offset) = self.offset {
            args["offset"] = serde_json::json!(offset);
        }
        if let Some(limit) = self.limit {
            args["limit"] = serde_json::json!(limit);
        }
        let outcome = execute_tool(
            self.worktree.path(),
            "Read",
            &args.to_string(),
            &TaskRegistry::new(),
            "read-window-test",
        )
        .await;
        assert!(
            !outcome.is_error,
            "Read must succeed, got: {}",
            outcome.error_message
        );
        ReadResult {
            value: serde_json::from_str(&outcome.result_json).expect("a Read result is JSON"),
        }
    }
}

struct ReadResult {
    value: serde_json::Value,
}

impl ReadResult {
    fn content(&self) -> &str {
        self.value["content"]
            .as_str()
            .expect("a Read result carries content")
    }

    fn truncated(&self) -> bool {
        self.value["truncated"]
            .as_bool()
            .expect("a windowed Read reports whether more lines follow")
    }

    fn total_lines(&self) -> u64 {
        self.value["total_lines"]
            .as_u64()
            .expect("a windowed Read reports the file's true length")
    }
}

#[tokio::test]
async fn reading_a_file_window_returns_only_the_requested_lines() {
    // Given a thousand-line file
    let call = a_read_of_a_file_with_lines(1000)
        .starting_at_line(400)
        .of_at_most(3);

    // When three lines are requested from line 400
    let result = call.run().await;

    // Then exactly those three come back, and the file's real size is reported alongside
    assert_eq!(result.content(), "line 401\nline 402\nline 403");
    assert!(result.truncated(), "597 lines follow the window");
    assert_eq!(result.total_lines(), 1000);
}

#[tokio::test]
async fn a_window_reaching_the_end_of_the_file_is_not_truncated() {
    // Given a ten-line file
    let call = a_read_of_a_file_with_lines(10)
        .starting_at_line(8)
        .of_at_most(50);

    // When a window larger than the remainder is requested
    let result = call.run().await;

    // Then it stops at the last line and says nothing follows
    assert_eq!(result.content(), "line 9\nline 10");
    assert!(!result.truncated(), "nothing follows the last line");
    assert_eq!(result.total_lines(), 10);
}

#[tokio::test]
async fn a_window_starting_past_the_end_of_the_file_is_empty_rather_than_an_error() {
    // Given a ten-line file
    let call = a_read_of_a_file_with_lines(10)
        .starting_at_line(99)
        .of_at_most(5);

    // When the window starts beyond the last line
    let result = call.run().await;

    // Then the answer is an empty window, not a failure — a caller paging forward runs off the
    // end exactly once and must be able to tell that from a broken read
    assert_eq!(result.content(), "");
    assert!(!result.truncated());
    assert_eq!(result.total_lines(), 10);
}

/// The compatibility guarantee. Every caller that has ever issued a bare `Read` gets what it
/// always got, byte for byte — including the trailing newline, which a lines-and-rejoin
/// implementation silently drops.
#[tokio::test]
async fn reading_without_a_window_returns_the_whole_file_unchanged() {
    // Given a file whose bytes end in a newline
    let worktree = TempDir::new().expect("a worktree to read from");
    let path = worktree.path().join(A_FILE);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("the source tree");
    let original = "line 1\nline 2\nline 3\n";
    std::fs::write(&path, original).expect("the file to read");

    // When it is read with no offset and no limit
    let outcome = execute_tool(
        worktree.path(),
        "Read",
        &serde_json::json!({ "path": A_FILE }).to_string(),
        &TaskRegistry::new(),
        "read-window-test",
    )
    .await;

    // Then the bytes come back exactly as they are on disk
    let value: serde_json::Value =
        serde_json::from_str(&outcome.result_json).expect("a Read result is JSON");
    assert_eq!(value["content"].as_str(), Some(original));
}
