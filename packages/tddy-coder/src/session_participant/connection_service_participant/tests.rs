//! Unit tests for `SessionConnectionService` — list_exec_tools returns a non-empty catalog;
//! execute_tool records a tool-call entry; claim_terminal_control grants the session's own
//! terminal.
//!
//! `DeleteSession` / `SignalSession` are intentionally NOT served here (daemon-direct) — see the
//! module doc.

use super::{SessionConnectionService, ToolDef, ToolExecutor, ToolOutcome};

/// Fake executor returning a canned success result.
struct FakeExecutor;
#[async_trait::async_trait]
impl ToolExecutor for FakeExecutor {
    async fn execute(&self, _tool_name: &str, _args_json: &str) -> ToolOutcome {
        ToolOutcome {
            result_json: r#"{"ok":true}"#.to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }
}

fn a_service(tool_calls_path: &std::path::Path) -> SessionConnectionService {
    SessionConnectionService {
        session_id: "sess-aaaaaaaa-0000-4000-8000-000000000001".to_string(),
        session_token: "caller-token".to_string(),
        tool_calls_path: tool_calls_path.to_path_buf(),
        tools: vec![ToolDef {
            name: "Echo".to_string(),
            description: "Echo a message".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        }],
        executor: std::sync::Arc::new(FakeExecutor),
    }
}

#[tokio::test]
async fn list_exec_tools_returns_a_non_empty_catalog() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let service = a_service(dir.path());

    // When
    let tools = service.list_exec_tools();

    // Then
    assert!(
        !tools.is_empty(),
        "session participant must expose a non-empty tool catalog"
    );
    assert_eq!(tools[0].name, "Echo");
}

#[tokio::test]
async fn execute_tool_appends_a_tool_calls_entry_and_returns_a_success_result() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool-calls.jsonl");
    let service = a_service(&path);

    // When
    let result = service.execute_tool("Echo", r#"{"message":"hi"}"#).await;

    // Then — the call returns a non-error result and records a schema-compatible tool-call line
    assert!(
        !result.is_error,
        "ExecuteTool must succeed; error='{}'",
        result.error_message
    );
    assert!(
        !result.result_json.is_empty(),
        "ExecuteTool must return a result_json"
    );
    let logged = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        logged.contains(r#""tool_name":"Echo""#),
        "ExecuteTool must append a ToolCallRecord with tool_name; got: {logged}"
    );
}

#[tokio::test]
async fn coder_session_tool_catalog_lists_every_shared_engine_tool() {
    // Given — the shared catalog
    let shared = tddy_tool_engine::tool_catalog();
    let catalog = super::coder_session_tool_catalog();

    // Then — the coder catalog mirrors the shared engine catalog (names + schemas preserved)
    assert_eq!(catalog.len(), shared.len(), "catalog length must match");
    for (got, want) in catalog.iter().zip(shared.iter()) {
        assert_eq!(got.name, want.name);
        assert_eq!(got.description, want.description);
        assert_eq!(got.input_schema_json, want.input_schema_json);
    }
}

#[tokio::test]
async fn coder_session_tool_executor_runs_a_real_read_after_write_against_the_worktree_root() {
    // Given — a tempdir worktree root and a real CoderSessionToolExecutor
    use super::CoderSessionToolExecutor;
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let executor = CoderSessionToolExecutor {
        worktree_root: root.clone(),
        task_registry: tddy_task::TaskRegistry::new(),
        session_id: "test-session".to_string(),
    };

    // When — Write a file via the executor seam
    let write_outcome = executor
        .execute(
            "Write",
            r#"{"path":"hello.txt","contents":"hi from coder"}"#,
        )
        .await;

    // Then — Write succeeds
    assert!(
        !write_outcome.is_error,
        "Write should succeed; got: {}",
        write_outcome.error_message
    );

    // And — Read returns the written contents
    let read_outcome = executor.execute("Read", r#"{"path":"hello.txt"}"#).await;
    assert!(
        !read_outcome.is_error,
        "Read should succeed; got: {}",
        read_outcome.error_message
    );
    let parsed: serde_json::Value = serde_json::from_str(&read_outcome.result_json).expect("json");
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("hi from coder"),
        "Read should return the written contents; got: {}",
        read_outcome.result_json
    );
}

#[tokio::test]
async fn claim_terminal_control_grants_control_for_the_sessions_own_terminal() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let service = a_service(dir.path());

    // When
    let result = service.claim_terminal_control("test-screen", false);

    // Then
    assert!(
        result.granted,
        "session participant must grant control of its own terminal"
    );
    assert!(
        !result.control_token.is_empty(),
        "grant must include a control token"
    );
}
