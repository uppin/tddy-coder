//! Acceptance tests: `ExecuteTool` and `ListExecTools` RPCs (PRD: docs/ft/daemon/remote-codebase-mode.md).
//!
//! AC4-AC11: tool catalog listing, path containment security, unknown-tool error shape,
//! connect-by-id against a pre-existing session, background Shell + Await round-trip.

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::SessionMetadata;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ListExecToolsRequest,
};
use tddy_testing_commons::a_session_metadata;

/// Seed a session directory with a `.session.yaml` pointing at a given worktree path.
fn seed_session(sessions_base: &std::path::Path, session_id: &str, repo_path: &std::path::Path) {
    let session_dir = unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("failed to create session dir in test");
    let metadata = a_session_metadata()
        .with_session_id(session_id)
        .with_project_id("proj-1")
        .with_repo_path(repo_path.to_str().expect("repo_path must be valid UTF-8"))
        .build();
    tddy_core::write_session_metadata(&session_dir, &metadata)
        .expect("failed to write session metadata in test");
}

/// AC4: `ListExecTools` returns a non-empty list of `ToolDef` records, each with a non-empty name,
/// description, and valid JSON Schema in `input_schema_json`.
#[tokio::test]
async fn list_exec_tools_returns_non_empty_catalog_with_valid_schemas() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .list_exec_tools(Request::new(ListExecToolsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ListExecTools must not fail");

    // Then
    let tools = &resp.get_ref().tools;
    assert!(!tools.is_empty(), "ListExecTools must return at least one tool");

    for tool in tools {
        assert!(!tool.name.is_empty(), "every ToolDef must have a non-empty name");
        assert!(
            !tool.description.is_empty(),
            "every ToolDef must have a non-empty description (tool: {})",
            tool.name
        );
        let schema: serde_json::Value =
            serde_json::from_str(&tool.input_schema_json).unwrap_or_else(|_| {
                panic!("ToolDef '{}' must have valid JSON in input_schema_json", tool.name)
            });
        assert!(
            schema.is_object(),
            "ToolDef '{}' input_schema_json must be a JSON object (schema), got {:?}",
            tool.name,
            schema
        );
    }

    let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    for expected in &["Read", "Write", "StrReplace", "Delete", "Grep", "Glob", "Shell", "Await"] {
        assert!(
            names.contains(expected),
            "catalog must include cursor tool '{}', got: {:?}",
            expected,
            names
        );
    }
}

/// AC7: `ExecuteTool` with a path that escapes the worktree root via `..` returns a
/// `permission_denied` RPC status — not an `is_error` tool-level result.
#[tokio::test]
async fn execute_tool_path_traversal_returns_permission_denied_status() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree_dir = tempfile::tempdir().unwrap();
    let session_id = "traversal-test-session";
    seed_session(sessions_tmp.path(), session_id, worktree_dir.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let result = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"../../etc/passwd"}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await;

    // Then
    let err = result.expect_err("path traversal must be rejected with an RPC error");
    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "path traversal must yield permission_denied status, got: {:?}",
        err
    );
}

/// AC8: `ExecuteTool` with an unknown `tool_name` returns `is_error:true` with a descriptive
/// `error_message` — NOT an RPC-level error.
#[tokio::test]
async fn execute_tool_unknown_tool_name_returns_is_error_not_rpc_error() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree_dir = tempfile::tempdir().unwrap();
    let session_id = "unknown-tool-session";
    seed_session(sessions_tmp.path(), session_id, worktree_dir.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "NonExistentTool".to_string(),
            args_json: "{}".to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("unknown tool must be a tool-level error, not an RPC error");

    // Then
    let body = resp.get_ref();
    assert!(body.is_error, "unknown tool must set is_error=true");
    assert!(
        !body.error_message.is_empty(),
        "unknown tool must include a non-empty error_message"
    );
}

/// AC9+AC10: `ExecuteTool("Shell")` with `block_until_ms:0` returns a non-empty `job_id` and
/// `job_running:true` immediately; a subsequent `ExecuteTool("Await")` with that `job_id` blocks
/// until the shell finishes and returns the exit code.
#[tokio::test]
async fn execute_tool_background_shell_then_await_round_trips() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree_dir = tempfile::tempdir().unwrap();
    let session_id = "shell-await-session";
    seed_session(sessions_tmp.path(), session_id, worktree_dir.path());
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When — launch a background shell: `echo hello` completes quickly
    let shell_resp = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "Shell".to_string(),
            args_json: r#"{"command":"echo hello","block_until_ms":0}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("Shell(background) must not return an RPC error");

    // Then
    let shell_body = shell_resp.get_ref();
    assert!(!shell_body.is_error, "Shell background launch must succeed (is_error=false)");
    assert!(!shell_body.job_id.is_empty(), "Shell background must return a non-empty job_id");
    assert!(shell_body.job_running, "Shell background must return job_running=true immediately");

    let job_id = shell_body.job_id.clone();

    // When — await the job; it finishes fast (echo), so this should complete without timing out
    let await_resp = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "Await".to_string(),
            args_json: format!(r#"{{"task_id":"{}","block_until_ms":5000}}"#, job_id),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("Await must not return an RPC error");

    // Then
    let await_body = await_resp.get_ref();
    assert!(
        !await_body.is_error,
        "Await must succeed, got error: {:?}",
        await_body.error_message
    );
    assert!(!await_body.job_running, "Await must return job_running=false when the job completes");

    let result: serde_json::Value =
        serde_json::from_str(&await_body.result_json).expect("Await result_json must be valid JSON");
    let exit_code = result
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .expect("Await result_json must have an 'exit_code' integer");
    assert_eq!(exit_code, 0, "echo exits 0; Await must report exit_code:0");
}

/// AC11: `ExecuteTool` works against any worktree-backed session — not just `workspace` sessions.
/// Seed a session with `session_type:"claude-cli"` and verify Read succeeds against its `repo_path`.
#[tokio::test]
async fn execute_tool_connect_by_id_works_on_cli_session_worktree() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let worktree_dir = tempfile::tempdir().unwrap();

    let test_content = "this file lives in the remote worktree";
    std::fs::write(worktree_dir.path().join("remote_file.txt"), test_content).unwrap();

    let session_id = "cli-session-by-id";
    let session_dir = unified_session_dir_path(sessions_tmp.path(), session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    // This session has a specific session_type — use struct literal for clarity on the intent
    let metadata = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "proj-2".to_string(),
        created_at: "2026-06-13T10:00:00Z".to_string(),
        updated_at: "2026-06-13T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(worktree_dir.path().to_str().unwrap().to_string()),
        pid: Some(1),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-4-8".to_string()),
        activity_status: None,
        hook_token: None,
    };
    tddy_core::write_session_metadata(&session_dir, &metadata).unwrap();
    let service = test_service(sessions_tmp.path().to_path_buf());

    // When
    let resp = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"remote_file.txt"}"#.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("ExecuteTool Read against a claude-cli session must not fail");

    // Then
    assert!(!resp.get_ref().is_error, "Read must succeed against a claude-cli session worktree");
    let result: serde_json::Value =
        serde_json::from_str(&resp.get_ref().result_json).expect("result_json must be valid JSON");
    assert_eq!(
        result.get("content").and_then(|v| v.as_str()),
        Some(test_content),
        "Read must return the file content from the claude-cli session worktree"
    );
}
