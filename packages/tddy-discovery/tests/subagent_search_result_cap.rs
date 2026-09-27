//! Unit tests: an un-windowed `GLOB` or `GREP` bounds how much it feeds back into the model's
//! context, the same way an un-windowed `READ` already does.
//!
//! `READ` learned this after session 019f2d14 — `DEFAULT_READ_LINE_CAP`, applied in
//! `tddy_discovery::subagent` rather than in the engine, because "what a tool call returns" and
//! "how much of it an agent may pull into a model context" are different questions
//! (`tddy-tool-engine/src/read_window.rs`). `GLOB` and `GREP` never did.
//!
//! Session 01a0e200 is what that costs. The agent, out of ideas, called `GLOB **/*` and the jail
//! answered with **408,282 bytes** — the entire repository tree — into a 32k-token window. A
//! `GREP` in the same turn returned 180,475 bytes. Neither tool advertises a `limit`, neither
//! reports whether anything was left out, and neither caps anything.
//!
//! The managed half is load-bearing: the paths cross the wire before anything on this side could
//! trim them, so the cap has to be *in the request*, exactly as `read_window` puts it there.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tddy_discovery::openai::discovery_tool_definitions;
use tddy_discovery::subagent::{CodebaseAccess, DEFAULT_GLOB_PATH_CAP, DEFAULT_GREP_MATCH_CAP};

/// Comfortably more matches than the cap, so a capped answer and an uncapped one differ.
const MORE_FILES_THAN_THE_CAP: usize = DEFAULT_GLOB_PATH_CAP + 50;
const FEWER_FILES_THAN_THE_CAP: usize = 3;
const MORE_LINES_THAN_THE_CAP: usize = DEFAULT_GREP_MATCH_CAP + 50;
const A_NEEDLE: &str = "PAGE_SCROLLBACK";

// ─── Fixtures ────────────────────────────────────────────────────────────────

/// A directory of `count` files named `file-000.txt` … so `glob::glob`'s lexicographic order is
/// deterministic and a capped prefix is exactly the first `n` of them.
fn a_directory_of_files(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    for i in 0..count {
        std::fs::write(dir.path().join(format!("file-{i:03}.txt")), "x").expect("write file");
    }
    dir
}

/// A directory holding one file with `count` lines that all match [`A_NEEDLE`].
fn a_directory_with_matching_lines(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let body: String = (0..count)
        .map(|i| format!("const {A_NEEDLE}_{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.path().join("constants.ts"), body).expect("write file");
    dir
}

fn every_file_in(dir: &tempfile::TempDir) -> String {
    dir.path().join("*.txt").to_string_lossy().into_owned()
}

/// A managed codebase that records the arguments it was dispatched with, and answers emptily —
/// so a test can assert on the *request* rather than on what came back.
#[derive(Clone)]
struct RecordingCodebase {
    calls: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl RecordingCodebase {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn access(&self) -> CodebaseAccess {
        let calls = Arc::clone(&self.calls);
        CodebaseAccess::managed(
            move |tool: String,
                  args: serde_json::Value|
                  -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
                calls.lock().unwrap().push((tool, args));
                Box::pin(async move {
                    serde_json::json!({ "paths": [], "truncated": false, "total_paths": 0 })
                        .to_string()
                })
            },
        )
    }

    fn arguments_of_the_only_call(&self) -> serde_json::Value {
        let calls = self.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "exactly one dispatch was expected");
        calls[0].1.clone()
    }
}

// ─── Fluent assertions over a search result ──────────────────────────────────

struct SearchResult(serde_json::Value);

impl SearchResult {
    fn assert_returned(&self, field: &str, expected: usize) -> &Self {
        let got = self.0[field]
            .as_array()
            .unwrap_or_else(|| panic!("a search result carries '{field}'; got {}", self.0))
            .len();
        assert_eq!(got, expected, "wrong number of '{field}' returned");
        self
    }

    fn assert_truncated(&self, expected: bool) -> &Self {
        assert_eq!(
            self.0["truncated"],
            serde_json::json!(expected),
            "a capped search must say so, and an uncapped one must say it did not"
        );
        self
    }

    fn assert_total(&self, field: &str, expected: usize) -> &Self {
        assert_eq!(
            self.0[field],
            serde_json::json!(expected),
            "the true size must be reported so the model can page instead of re-searching"
        );
        self
    }
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The 408 KB answer, bounded. Without this a single `GLOB **/*` is more tokens than the whole
/// window a fast local agent runs in.
#[tokio::test]
async fn an_unwindowed_glob_returns_at_most_the_default_number_of_paths() {
    // Given a directory with more files than the cap
    let dir = a_directory_of_files(MORE_FILES_THAN_THE_CAP);

    // When every file is globbed with no limit
    let result = CodebaseAccess::Local
        .glob(&every_file_in(&dir))
        .await
        .expect("a glob over a readable directory succeeds");

    // Then the answer is capped and says so
    SearchResult(result)
        .assert_returned("paths", DEFAULT_GLOB_PATH_CAP)
        .assert_truncated(true)
        .assert_total("total_paths", MORE_FILES_THAN_THE_CAP);
}

/// The control: a search that fits is returned whole, and is not reported as cut.
#[tokio::test]
async fn a_glob_within_the_cap_returns_every_path_and_reports_nothing_left_out() {
    // Given a directory with fewer files than the cap
    let dir = a_directory_of_files(FEWER_FILES_THAN_THE_CAP);

    // When every file is globbed
    let result = CodebaseAccess::Local
        .glob(&every_file_in(&dir))
        .await
        .expect("a glob over a readable directory succeeds");

    // Then everything came back
    SearchResult(result)
        .assert_returned("paths", FEWER_FILES_THAN_THE_CAP)
        .assert_truncated(false)
        .assert_total("total_paths", FEWER_FILES_THAN_THE_CAP);
}

/// `GREP` returned 180 KB in the same turn as the glob, and needs the same bound.
#[tokio::test]
async fn an_unwindowed_grep_returns_at_most_the_default_number_of_matches() {
    // Given a file with more matching lines than the cap
    let dir = a_directory_with_matching_lines(MORE_LINES_THAN_THE_CAP);

    // When it is searched with no limit
    let result = CodebaseAccess::Local
        .grep(A_NEEDLE, Some(&dir.path().to_string_lossy()))
        .await
        .expect("a grep over a readable directory succeeds");

    // Then the answer is capped and says so
    SearchResult(result)
        .assert_returned("matches", DEFAULT_GREP_MATCH_CAP)
        .assert_truncated(true)
        .assert_total("total_matches", MORE_LINES_THAN_THE_CAP);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// A model that knows it wants five answers should be able to ask for five.
#[tokio::test]
async fn an_explicit_limit_narrows_a_glob_below_the_default() {
    // Given a directory with more files than the cap
    let dir = a_directory_of_files(MORE_FILES_THAN_THE_CAP);

    // When only five are asked for
    let result = CodebaseAccess::Local
        .glob_limited(&every_file_in(&dir), Some(5))
        .await
        .expect("a glob over a readable directory succeeds");

    // Then five came back, and the true total is still reported
    SearchResult(result)
        .assert_returned("paths", 5)
        .assert_truncated(true)
        .assert_total("total_paths", MORE_FILES_THAN_THE_CAP);
}

// ─── API boundaries ──────────────────────────────────────────────────────────

/// The managed path is the one that actually failed: the paths cross the wire before anything
/// here could trim them, so a cap applied after the transfer bounds the context and not the wire.
#[tokio::test]
async fn a_managed_glob_puts_the_cap_in_the_request_rather_than_trimming_the_answer() {
    // Given a managed codebase
    let codebase = RecordingCodebase::new();

    // When an un-windowed glob is issued
    codebase
        .access()
        .glob("**/*")
        .await
        .expect("the recording codebase answers");

    // Then the request carried the cap
    assert_eq!(
        codebase.arguments_of_the_only_call(),
        serde_json::json!({ "pattern": "**/*", "limit": DEFAULT_GLOB_PATH_CAP })
    );
}

/// A cap the model cannot see is a cap it cannot page past. `READ` advertises `offset`/`limit`
/// for exactly this reason.
#[test]
fn the_glob_tool_schema_advertises_the_limit_the_model_can_page_with() {
    // Given the tool definitions a subagent is sent
    let definitions = discovery_tool_definitions();

    // When the GLOB definition is read
    let glob = definitions
        .iter()
        .find(|d| d.function.name == "GLOB")
        .expect("GLOB is a discovery tool");

    // Then it offers a limit
    assert_eq!(
        glob.function.parameters["properties"]["limit"]["type"],
        serde_json::json!("integer"),
        "a model told its answer was truncated must be able to ask for a different window"
    );
}
