//! Unit tests: `CodebaseAccess` — the abstraction that lets a subagent's internal READ/GLOB/GREP
//! tool loop read either the local filesystem or a proxied (managed) codebase through an injected
//! dispatch function, without `tddy-discovery` depending on `tddy-tools`.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 4-5)
//! Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md

use std::sync::{Arc, Mutex};

use tddy_discovery::subagent::CodebaseAccess;

type RecordedCalls = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

/// A `CodebaseAccess::Managed` backed by a fake dispatch fn that records every call and always
/// returns `response` — enough to prove the name mapping and error surfacing without a real
/// daemon/HTTP round trip.
fn managed_access_with(response: &'static str) -> (RecordedCalls, CodebaseAccess) {
    let calls: RecordedCalls = Arc::new(Mutex::new(Vec::new()));
    let calls_for_closure = calls.clone();
    let access = CodebaseAccess::managed(move |tool_name: String, args: serde_json::Value| {
        let calls = calls_for_closure.clone();
        Box::pin(async move {
            calls.lock().unwrap().push((tool_name, args));
            response.to_string()
        })
    });
    (calls, access)
}

// ─── Managed: name mapping ─────────────────────────────────────────────────────

/// A managed READ dispatches the capitalized `"Read"` tool name with a `{"path": ...}` argument
/// payload — the same shape `tddy-tools`' exec-tool catalog expects.
#[tokio::test]
async fn managed_codebase_access_maps_read_to_the_capitalized_read_tool_name() {
    // Given
    let (calls, access) = managed_access_with(r#"{"content":"fn main() {}"}"#);

    // When
    let result = access
        .read("src/main.rs")
        .await
        .expect("managed READ must succeed when the dispatch fn returns a success payload");

    // Then
    assert_eq!(result["content"].as_str(), Some("fn main() {}"));
    let recorded = calls.lock().unwrap();
    assert_eq!(recorded.len(), 1, "exactly one dispatch call must be made");
    assert_eq!(
        recorded[0].0, "Read",
        "dispatched tool name must be 'Read', not 'READ'"
    );
    assert_eq!(recorded[0].1, serde_json::json!({"path": "src/main.rs"}));
}

/// A managed GLOB dispatches the capitalized `"Glob"` tool name with a `{"pattern": ...}` payload.
#[tokio::test]
async fn managed_codebase_access_maps_glob_to_the_capitalized_glob_tool_name() {
    // Given
    let (calls, access) = managed_access_with(r#"{"paths":["src/lib.rs","src/main.rs"]}"#);

    // When
    let result = access
        .glob("src/**/*.rs")
        .await
        .expect("managed GLOB must succeed when the dispatch fn returns a success payload");

    // Then
    let paths = result["paths"]
        .as_array()
        .expect("result must carry a 'paths' array");
    assert_eq!(paths.len(), 2);
    let recorded = calls.lock().unwrap();
    assert_eq!(
        recorded[0].0, "Glob",
        "dispatched tool name must be 'Glob', not 'GLOB'"
    );
    assert_eq!(recorded[0].1, serde_json::json!({"pattern": "src/**/*.rs"}));
}

/// A managed GREP dispatches the capitalized `"Grep"` tool name, including the optional `path`
/// argument only when the caller provided one.
#[tokio::test]
async fn managed_codebase_access_maps_grep_to_the_capitalized_grep_tool_name_with_optional_path() {
    // Given
    let (calls, access) = managed_access_with(r#"{"matches":[]}"#);

    // When
    access
        .grep("fn authenticate", Some("src/auth.rs"))
        .await
        .expect("managed GREP must succeed when the dispatch fn returns a success payload");

    // Then
    let recorded = calls.lock().unwrap();
    assert_eq!(
        recorded[0].0, "Grep",
        "dispatched tool name must be 'Grep', not 'GREP'"
    );
    assert_eq!(
        recorded[0].1,
        serde_json::json!({"pattern": "fn authenticate", "path": "src/auth.rs"})
    );
}

/// When the injected dispatch fn's response carries `is_error: true` (the same convention
/// `session_tool_client::format_tool_dispatch_result` uses), `CodebaseAccess::Managed` surfaces it
/// as an `Err` rather than returning the raw error envelope as if it were a successful result.
#[tokio::test]
async fn managed_codebase_access_surfaces_is_error_responses_as_errors() {
    // Given
    let (_calls, access) =
        managed_access_with(r#"{"error":"path escapes worktree root","is_error":true}"#);

    // When
    let result = access.read("../../etc/passwd").await;

    // Then
    assert!(
        result.is_err(),
        "managed READ must return Err when the dispatch response has is_error:true; got Ok"
    );
    let message = result.err().unwrap().to_string();
    assert!(
        message.contains("path escapes worktree root"),
        "error message must contain the dispatch fn's error text; got: {message:?}"
    );
}

// ─── Local: direct host filesystem ─────────────────────────────────────────────

/// `CodebaseAccess::Local` reads a file straight from the host filesystem, with the same
/// `{"content": ...}` result shape as the managed path.
#[tokio::test]
async fn local_codebase_access_reads_a_file_from_the_local_filesystem() {
    // Given
    let dir = tempfile::tempdir().expect("temp dir");
    let file_path = dir.path().join("hello.rs");
    std::fs::write(&file_path, "fn hello() {}\n").expect("write temp file");

    // When
    let result = CodebaseAccess::Local
        .read(file_path.to_str().unwrap())
        .await
        .expect("local READ must succeed for an existing file");

    // Then
    assert_eq!(result["content"].as_str(), Some("fn hello() {}\n"));
}

/// `CodebaseAccess::Local` globs matching paths straight from the host filesystem.
#[tokio::test]
async fn local_codebase_access_globs_matching_paths_from_the_local_filesystem() {
    // Given
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("a.rs"), "").expect("write a.rs");
    std::fs::write(dir.path().join("b.txt"), "").expect("write b.txt");
    let pattern = format!("{}/*.rs", dir.path().display());

    // When
    let result = CodebaseAccess::Local
        .glob(&pattern)
        .await
        .expect("local GLOB must succeed");

    // Then
    let paths = result["paths"]
        .as_array()
        .expect("result must carry a 'paths' array");
    assert_eq!(
        paths.len(),
        1,
        "only the .rs file must match; got: {paths:?}"
    );
}

/// `CodebaseAccess::Local` greps matching lines straight from the host filesystem.
#[tokio::test]
async fn local_codebase_access_greps_matching_lines_from_the_local_filesystem() {
    // Given
    let dir = tempfile::tempdir().expect("temp dir");
    let file_path = dir.path().join("auth.rs");
    std::fs::write(&file_path, "fn authenticate() {\n    let x = 1;\n}\n").expect("write auth.rs");

    // When
    let result = CodebaseAccess::Local
        .grep("fn authenticate", Some(file_path.to_str().unwrap()))
        .await
        .expect("local GREP must succeed");

    // Then
    let matches = result["matches"]
        .as_array()
        .expect("result must carry a 'matches' array");
    assert_eq!(
        matches.len(),
        1,
        "exactly one match for 'fn authenticate'; got: {matches:?}"
    );
}
