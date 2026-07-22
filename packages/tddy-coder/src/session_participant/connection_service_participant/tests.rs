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
    // Terminals spawn in a real worktree. The entry-based tests pass a directory as
    // `tool_calls_path` (the tempdir itself); the file-based tests pass a path inside it. Point the
    // worktree at whichever directory the tempdir represents so started shells have a valid cwd.
    let worktree = if tool_calls_path.is_dir() {
        tool_calls_path.to_path_buf()
    } else {
        tool_calls_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    };
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
        worktree: worktree.clone(),
        terminal_manager: std::sync::Arc::new(
            crate::session_participant::terminal_manager::TerminalManager::new(),
        ),
        agent_activity_dir: worktree,
        presenter_events: None,
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
        toolcall_socket_path: None,
        session_dir: root.clone(),
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

// ---------------------------------------------------------------------------
// Multiple terminals per session — the coder participant serves the
// terminal_id-addressed terminal RPCs (bash tabs). Exercised through the public
// `session_connection_service_entry` → `RpcService::handle_rpc` seam.
// ---------------------------------------------------------------------------

use prost::Message as _;
use tddy_rpc::{Code, RequestMetadata, RpcMessage, RpcResult};
use tddy_service::proto::connection as pb;

const TERM_SID: &str = "sess-aaaaaaaa-0000-4000-8000-000000000001";
const TERM_TOKEN: &str = "caller-token";

fn a_service_entry(tool_calls_path: &std::path::Path) -> tddy_rpc::ServiceEntry {
    crate::session_participant::session_connection_service_entry(a_service(tool_calls_path))
}

async fn call(entry: &tddy_rpc::ServiceEntry, method: &str, payload: Vec<u8>) -> RpcResult {
    let msg = RpcMessage::new(payload, RequestMetadata::default());
    entry
        .service
        .handle_rpc("connection.ConnectionService", method, &msg)
        .await
}

/// Unwrap a unary success payload, panicking with the status when the call errored or streamed.
fn unary_ok(result: RpcResult) -> Vec<u8> {
    match result {
        RpcResult::Unary(Ok(bytes)) => bytes,
        RpcResult::Unary(Err(status)) => {
            panic!(
                "expected a unary success, got error {:?}: {}",
                status.code(),
                status.message()
            )
        }
        RpcResult::ServerStream(_) => panic!("expected a unary success, got a server stream"),
    }
}

#[tokio::test]
async fn start_terminal_session_returns_a_fresh_non_main_terminal_id() {
    // Given a session participant
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());

    // When the web opens a new terminal
    let req = pb::StartTerminalSessionRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        control_token: String::new(),
    };
    let resp = pb::StartTerminalSessionResponse::decode(
        &unary_ok(call(&entry, "StartTerminalSession", req.encode_to_vec()).await)[..],
    )
    .expect("decode StartTerminalSessionResponse");

    // Then it gets a fresh terminal id that is never the reserved "main"
    assert!(
        !resp.terminal_id.is_empty(),
        "a started terminal must have an id"
    );
    assert_ne!(
        resp.terminal_id, "main",
        "a started terminal must not reuse the reserved main id"
    );
}

#[tokio::test]
async fn list_terminal_sessions_includes_a_started_bash_terminal() {
    // Given a session participant with one started terminal
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());
    let start = pb::StartTerminalSessionRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        control_token: String::new(),
    };
    let started = pb::StartTerminalSessionResponse::decode(
        &unary_ok(call(&entry, "StartTerminalSession", start.encode_to_vec()).await)[..],
    )
    .expect("decode StartTerminalSessionResponse");

    // When the terminals are listed
    let list = pb::ListTerminalSessionsRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
    };
    let listed = pb::ListTerminalSessionsResponse::decode(
        &unary_ok(call(&entry, "ListTerminalSessions", list.encode_to_vec()).await)[..],
    )
    .expect("decode ListTerminalSessionsResponse");

    // Then the started terminal appears, labelled as a bash shell
    let found = listed
        .terminals
        .iter()
        .find(|t| t.terminal_id == started.terminal_id)
        .expect("started terminal must be listed");
    assert_eq!(
        found.kind, "bash",
        "a started shell terminal is kind 'bash'"
    );
}

#[tokio::test]
async fn stop_terminal_session_rejects_the_main_terminal() {
    // Given a session participant
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());

    // When the web tries to stop the reserved main terminal
    let req = pb::StopTerminalSessionRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        terminal_id: "main".to_string(),
        control_token: String::new(),
    };
    let result = call(&entry, "StopTerminalSession", req.encode_to_vec()).await;

    // Then it is rejected with INVALID_ARGUMENT (main is stopped via Delete/Signal, not here)
    match result {
        RpcResult::Unary(Err(status)) => assert_eq!(
            status.code(),
            Code::InvalidArgument,
            "stopping the main terminal must be INVALID_ARGUMENT, got: {}",
            status.message()
        ),
        RpcResult::Unary(Ok(_)) => panic!("stopping the main terminal must not succeed"),
        RpcResult::ServerStream(_) => panic!("StopTerminalSession is unary"),
    }
}

#[tokio::test]
async fn send_terminal_input_is_accepted_for_a_started_terminal() {
    // Given a session participant with one started terminal
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());
    let start = pb::StartTerminalSessionRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        control_token: String::new(),
    };
    let started = pb::StartTerminalSessionResponse::decode(
        &unary_ok(call(&entry, "StartTerminalSession", start.encode_to_vec()).await)[..],
    )
    .expect("decode StartTerminalSessionResponse");

    // When input is sent to that terminal
    let input = pb::SessionTerminalInput {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        data: b"echo hi\n".to_vec(),
        terminal_id: started.terminal_id.clone(),
        control_token: String::new(),
    };
    let result = call(&entry, "SendTerminalInput", input.encode_to_vec()).await;

    // Then it is accepted
    pb::SendTerminalInputResponse::decode(&unary_ok(result)[..])
        .expect("decode SendTerminalInputResponse");
}

#[tokio::test]
async fn stream_terminal_output_is_server_streaming() {
    // Given a session participant
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());

    // When an output stream is opened for a terminal
    let req = pb::StreamTerminalOutputRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        terminal_id: "main".to_string(),
        initial_cols: 80,
        initial_rows: 24,
    };
    let result = call(&entry, "StreamTerminalOutput", req.encode_to_vec()).await;

    // Then the RPC is served as a server stream (not a unary / unimplemented reply)
    assert!(
        matches!(result, RpcResult::ServerStream(_)),
        "StreamTerminalOutput must be a server-streaming RPC"
    );
}

#[tokio::test]
async fn stream_terminal_output_streams_a_started_shell_output() {
    // Given a session participant with a started bash terminal
    let dir = tempfile::tempdir().unwrap();
    let entry = a_service_entry(dir.path());
    let start = pb::StartTerminalSessionRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        control_token: String::new(),
    };
    let started = pb::StartTerminalSessionResponse::decode(
        &unary_ok(call(&entry, "StartTerminalSession", start.encode_to_vec()).await)[..],
    )
    .expect("decode StartTerminalSessionResponse");

    // When its output stream is opened and a command is sent
    let stream_req = pb::StreamTerminalOutputRequest {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        terminal_id: started.terminal_id.clone(),
        initial_cols: 80,
        initial_rows: 24,
    };
    let mut rx = match call(&entry, "StreamTerminalOutput", stream_req.encode_to_vec()).await {
        RpcResult::ServerStream(Ok(rx)) => rx,
        RpcResult::ServerStream(Err(status)) => {
            panic!(
                "StreamTerminalOutput stream open failed: {}",
                status.message()
            )
        }
        RpcResult::Unary(_) => panic!("StreamTerminalOutput must open a server stream, got unary"),
    };
    let input = pb::SessionTerminalInput {
        session_token: TERM_TOKEN.to_string(),
        session_id: TERM_SID.to_string(),
        data: b"echo tddy-marker\n".to_vec(),
        terminal_id: started.terminal_id.clone(),
        control_token: String::new(),
    };
    unary_ok(call(&entry, "SendTerminalInput", input.encode_to_vec()).await);

    // Then the shell's echoed output is delivered on the stream
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for shell output; saw: {seen:?}"
        );
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(Ok(chunk))) => {
                let frame = pb::SessionTerminalOutput::decode(&chunk[..])
                    .expect("decode SessionTerminalOutput");
                seen.push_str(&String::from_utf8_lossy(&frame.data));
                if seen.contains("tddy-marker") {
                    break;
                }
            }
            Ok(Some(Err(status))) => panic!("stream errored: {}", status.message()),
            Ok(None) => panic!("stream closed before the marker; saw: {seen:?}"),
            Err(_) => panic!("timed out waiting for shell output; saw: {seen:?}"),
        }
    }
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
