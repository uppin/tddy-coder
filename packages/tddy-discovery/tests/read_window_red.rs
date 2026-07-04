//! Unit tests: the subagent READ tool must honour an explicit `offset`/`limit` window so a model that
//! reads a large file can page through it (local slicing) instead of swallowing the whole file,
//! and a managed subagent must forward that window to the daemon's `Read` tool rather than pulling
//! the entire file across and slicing after the fact. This is the paging escape-hatch that makes
//! the default cap (see `read_output_cap_red.rs`) usable in session 019f2d14-style investigations.
//!
//! Root-cause: packages/tddy-discovery/src/subagent.rs (`CodebaseAccess` has no windowed read).

use std::sync::{Arc, Mutex};

use tddy_discovery::subagent::CodebaseAccess;

fn numbered_lines_range(start: usize, end: usize) -> String {
    (start..end)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn a_file_containing(body: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("source.rs");
    std::fs::write(&path, body).expect("write temp file");
    let path_str = path.to_str().expect("utf-8 path").to_string();
    (dir, path_str)
}

struct ReadResult(serde_json::Value);

fn read_result(value: serde_json::Value) -> ReadResult {
    ReadResult(value)
}

impl ReadResult {
    fn has_content(&self, expected: &str) -> &Self {
        let actual = self.0["content"]
            .as_str()
            .expect("READ result must carry a string 'content' field");
        assert_eq!(actual, expected, "READ content mismatch");
        self
    }

    fn is_truncated(&self, expected: bool) -> &Self {
        let actual = self.0["truncated"]
            .as_bool()
            .expect("READ result must carry a boolean 'truncated' field");
        assert_eq!(actual, expected, "READ truncation flag mismatch");
        self
    }

    fn has_total_lines(&self, expected: usize) -> &Self {
        let actual = self.0["total_lines"]
            .as_u64()
            .expect("READ result must report 'total_lines'");
        assert_eq!(actual as usize, expected, "READ total_lines mismatch");
        self
    }
}

type RecordedCalls = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// A `CodebaseAccess::Managed` whose dispatch fn records each `(tool_name, args)` and returns a
/// fixed success payload — enough to assert what arguments were forwarded to the daemon.
fn managed_access_recording() -> (RecordedCalls, CodebaseAccess) {
    let calls: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_closure = calls.clone();
    let access = CodebaseAccess::managed(move |tool_name: String, args: serde_json::Value| {
        let calls = calls_for_closure.clone();
        Box::pin(async move {
            calls.lock().unwrap().push((tool_name, args));
            r#"{"content":"line 100\nline 101"}"#.to_string()
        })
    });
    (calls, access)
}

/// An explicit `offset`/`limit` returns exactly that line window, flagged truncated when more of
/// the file follows the window.
#[tokio::test]
async fn read_window_returns_the_requested_line_range() {
    // Given — a 500-line file
    let (_dir, path) = a_file_containing(&numbered_lines_range(0, 500));

    // When — ask for 50 lines starting at line 100
    let result = CodebaseAccess::Local
        .read_window(&path, Some(100), Some(50))
        .await
        .expect("windowed READ must succeed");

    // Then — lines 100..150 only, truncated because the file continues past line 150
    read_result(result)
        .has_content(&numbered_lines_range(100, 150))
        .is_truncated(true)
        .has_total_lines(500);
}

/// A window that reaches the end of the file is not flagged truncated, even if `limit` asked for
/// more lines than remain.
#[tokio::test]
async fn read_window_reaching_end_of_file_is_not_truncated() {
    // Given — a 500-line file
    let (_dir, path) = a_file_containing(&numbered_lines_range(0, 500));

    // When — ask for 50 lines starting at line 480 (only 20 remain)
    let result = CodebaseAccess::Local
        .read_window(&path, Some(480), Some(50))
        .await
        .expect("windowed READ must succeed");

    // Then — the final 20 lines, not truncated
    read_result(result)
        .has_content(&numbered_lines_range(480, 500))
        .is_truncated(false)
        .has_total_lines(500);
}

/// A managed READ forwards the caller's `offset`/`limit` to the daemon's `Read` tool so windowing
/// happens at the source instead of after a whole-file transfer.
#[tokio::test]
async fn managed_read_window_forwards_offset_and_limit_to_the_read_tool() {
    // Given
    let (calls, access) = managed_access_recording();

    // When
    access
        .read_window("src/config.rs", Some(100), Some(50))
        .await
        .expect("managed windowed READ must succeed");

    // Then — the dispatched Read call carries path + offset + limit
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one dispatch call must be made");
    assert_eq!(recorded[0].0, "Read", "dispatched tool name must be 'Read'");
    assert_eq!(
        recorded[0].1,
        serde_json::json!({"path": "src/config.rs", "offset": 100, "limit": 50}),
        "managed READ must forward offset and limit to the daemon"
    );
}
