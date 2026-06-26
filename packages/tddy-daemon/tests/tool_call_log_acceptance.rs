//! Acceptance tests: durable tool-call log + `ListSessionToolCalls` RPC.
//!
//! Changeset: `session-inspector-tools-tab`
//! PRD: `docs/ft/web/session-drawer.md` (Tools Tab section)
//!
//! These tests verify that:
//! - Every `ExecuteTool` call writes a durable JSONL record capturing `args_json`.
//! - The record persists independently of the in-memory `TaskRegistry`.
//! - `ListSessionToolCalls` reads from the JSONL log and is scoped to `session_id`.
//! - Auth is enforced on `ListSessionToolCalls`.
//!
//! ⚠️ RED PHASE — these tests are intentionally failing until:
//!   1. `ListSessionToolCalls` is added to `connection.proto`.
//!   2. The handler is implemented in `connection_service.rs`.
//!   3. `append_tool_call` is wired into the `execute_tool` handler.

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_daemon::tool_call_log::{read_tool_calls, TOOL_CALLS_FILENAME};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ListSessionToolCallsRequest,
};
use tddy_testing_commons::a_session_metadata;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Seed a session directory with a `.session.yaml` pointing at a temporary worktree path.
fn seed_session(sessions_base: &std::path::Path, session_id: &str, repo_path: &std::path::Path) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("failed to create session dir in test");
    let metadata = a_session_metadata()
        .with_session_id(session_id)
        .with_project_id("proj-tools-tab")
        .with_repo_path(repo_path.to_str().expect("repo_path must be valid UTF-8"))
        .build();
    tddy_core::write_session_metadata(&session_dir, &metadata)
        .expect("failed to write session metadata in test");
}

// ---------------------------------------------------------------------------
// AC1: ExecuteTool writes a durable record whose args_json matches the request
// ---------------------------------------------------------------------------

/// AC1: After a successful `ExecuteTool("Read")`, a `tool-calls.jsonl` record is written
/// to the session directory. The record's `args_json` field matches the request exactly —
/// proving that the previously-dropped input is now durably captured.
#[tokio::test]
async fn execute_tool_writes_durable_record_with_args_json() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_tmp = tempfile::tempdir().unwrap();
    let session_id = "log-accept-read-0000-0000-0000-000000000001";
    seed_session(sessions_tmp.path(), session_id, repo_tmp.path());
    // Seed a readable file in the repo root.
    std::fs::write(repo_tmp.path().join("README.md"), b"hello").unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());
    let args_json = r#"{"path":"README.md"}"#;

    // When
    service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "Read".to_string(),
            args_json: args_json.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ExecuteTool Read must succeed");

    // Then — the JSONL file must exist and contain exactly one record.
    let session_dir = unified_session_dir_path(sessions_tmp.path(), session_id);
    let records = read_tool_calls(&session_dir).expect("read_tool_calls must not fail");
    assert_eq!(
        records.len(),
        1,
        "one ExecuteTool call must produce exactly one JSONL record"
    );
    assert_eq!(
        records[0].tool_name, "Read",
        "record must capture the tool name"
    );
    assert_eq!(
        records[0].args_json, args_json,
        "record must capture args_json exactly — this is the formerly-dropped input"
    );
    assert!(
        !records[0].result_json.is_empty(),
        "record must capture result_json"
    );
}

// ---------------------------------------------------------------------------
// AC2: Durable record persists without the in-memory registry
// ---------------------------------------------------------------------------

/// AC2: The durable JSONL record can be read back by `read_tool_calls` without
/// consulting the `TaskRegistry`. Simulated by reading directly from the file
/// after the service call — if the log were backed by the registry (evictable),
/// this would still work superficially, but the test makes the independence
/// explicit by reading via the file-level API, not through the RPC.
#[tokio::test]
async fn durable_record_readable_independently_of_task_registry() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_tmp = tempfile::tempdir().unwrap();
    let session_id = "log-accept-durable-0000-0000-0000-000000000002";
    seed_session(sessions_tmp.path(), session_id, repo_tmp.path());
    std::fs::write(repo_tmp.path().join("notes.txt"), b"content").unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When — execute two tools
    for (tool_name, args) in [
        ("Read", r#"{"path":"notes.txt"}"#),
        ("Glob", r#"{"pattern":"*.txt"}"#),
    ] {
        service
            .execute_tool(Request::new(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool_name.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("ExecuteTool must succeed");
    }

    // Then — two records in the file; order is chronological.
    let session_dir = unified_session_dir_path(sessions_tmp.path(), session_id);
    let records = read_tool_calls(&session_dir).expect("read_tool_calls must not fail");
    assert_eq!(records.len(), 2, "two tool calls must produce two records");
    assert_eq!(records[0].tool_name, "Read");
    assert_eq!(records[1].tool_name, "Glob");
}

// ---------------------------------------------------------------------------
// AC3: ListSessionToolCalls returns records scoped to the session
// ---------------------------------------------------------------------------

/// AC3: `ListSessionToolCalls` returns only the tool calls made in the requested
/// `session_id`. Calls made in a different session are not included.
#[tokio::test]
async fn list_session_tool_calls_is_scoped_to_session_id() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_a = tempfile::tempdir().unwrap();
    let repo_b = tempfile::tempdir().unwrap();
    let session_a = "log-accept-scope-aaaa-0000-0000-0000-000000000003";
    let session_b = "log-accept-scope-bbbb-0000-0000-0000-000000000004";
    seed_session(sessions_tmp.path(), session_a, repo_a.path());
    seed_session(sessions_tmp.path(), session_b, repo_b.path());
    std::fs::write(repo_a.path().join("a.txt"), b"a").unwrap();
    std::fs::write(repo_b.path().join("b.txt"), b"b").unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    // Call a tool in session A twice and session B once.
    for args in [r#"{"path":"a.txt"}"#, r#"{"path":"a.txt"}"#] {
        service
            .execute_tool(Request::new(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_a.to_string(),
                tool_name: "Read".to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("ExecuteTool must succeed in session A");
    }
    service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_b.to_string(),
            tool_name: "Glob".to_string(),
            args_json: r#"{"pattern":"*.txt"}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ExecuteTool must succeed in session B");

    // When — list tool calls for session A only.
    let resp = service
        .list_session_tool_calls(Request::new(ListSessionToolCallsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_a.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ListSessionToolCalls must not fail");

    // Then — exactly two records, both from session A.
    let calls = &resp.get_ref().tool_calls;
    assert_eq!(
        calls.len(),
        2,
        "session A must have exactly 2 tool call records"
    );
    for call in calls {
        assert_eq!(
            call.tool_name, "Read",
            "all session A records must be 'Read', not session B's 'Glob'"
        );
    }
}

// ---------------------------------------------------------------------------
// AC4: ListSessionToolCalls returns records chronologically
// ---------------------------------------------------------------------------

/// AC4: Records are returned in chronological order (oldest first); `args_json`
/// of each record matches what was passed in the original `ExecuteToolRequest`.
#[tokio::test]
async fn list_session_tool_calls_returns_records_chronologically_with_args_json() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_tmp = tempfile::tempdir().unwrap();
    let session_id = "log-accept-chrono-0000-0000-0000-000000000005";
    seed_session(sessions_tmp.path(), session_id, repo_tmp.path());
    std::fs::write(repo_tmp.path().join("file.txt"), b"data").unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    let calls_in = vec![
        ("Read", r#"{"path":"file.txt"}"#),
        ("Glob", r#"{"pattern":"**/*.txt"}"#),
        ("Grep", r#"{"pattern":"data","path":"."}"#),
    ];
    for (tool, args) in &calls_in {
        service
            .execute_tool(Request::new(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("ExecuteTool must succeed");
    }

    // When
    let resp = service
        .list_session_tool_calls(Request::new(ListSessionToolCallsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ListSessionToolCalls must not fail");

    // Then
    let calls_out = &resp.get_ref().tool_calls;
    assert_eq!(
        calls_out.len(),
        calls_in.len(),
        "must return all recorded calls"
    );
    for (i, ((exp_tool, exp_args), got)) in calls_in.iter().zip(calls_out.iter()).enumerate() {
        assert_eq!(got.tool_name, *exp_tool, "call {} tool_name must match", i);
        assert_eq!(got.args_json, *exp_args, "call {} args_json must match", i);
    }
}

// ---------------------------------------------------------------------------
// AC5: ListSessionToolCalls rejects invalid session token
// ---------------------------------------------------------------------------

/// AC5: `ListSessionToolCalls` with a missing or invalid `session_token` must return
/// an `UNAUTHENTICATED` gRPC status — consistent with all other authenticated RPCs.
#[tokio::test]
async fn list_session_tool_calls_rejects_invalid_token() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let result = service
        .list_session_tool_calls(Request::new(ListSessionToolCallsRequest {
            session_token: "not-a-valid-token".to_string(),
            session_id: "any-session".to_string(),
            daemon_instance_id: String::new(),
        }))
        .await;

    // Then
    let err = result.expect_err("must fail with invalid token");
    assert_eq!(
        err.code(),
        Code::Unauthenticated,
        "invalid token must yield UNAUTHENTICATED, got: {:?}",
        err
    );
}

// ---------------------------------------------------------------------------
// AC6: ListSessionToolCalls for session with no calls returns empty list
// ---------------------------------------------------------------------------

/// AC6: When a session exists but no `ExecuteTool` calls have been made yet,
/// `ListSessionToolCalls` returns an empty list (not an error).
#[tokio::test]
async fn list_session_tool_calls_for_session_with_no_calls_returns_empty() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let repo_tmp = tempfile::tempdir().unwrap();
    let session_id = "log-accept-empty-0000-0000-0000-000000000006";
    seed_session(sessions_tmp.path(), session_id, repo_tmp.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    // Verify the JSONL file does not exist yet.
    let session_dir = unified_session_dir_path(sessions_tmp.path(), session_id);
    assert!(
        !session_dir.join(TOOL_CALLS_FILENAME).exists(),
        "tool-calls.jsonl must not exist before any calls"
    );

    // When
    let resp = service
        .list_session_tool_calls(Request::new(ListSessionToolCallsRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ListSessionToolCalls must not fail for a session with no calls");

    // Then
    assert!(
        resp.get_ref().tool_calls.is_empty(),
        "empty session must return an empty tool_calls list"
    );
}
