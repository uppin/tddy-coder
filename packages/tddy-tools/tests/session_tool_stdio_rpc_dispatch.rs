//! Unit tests for the sandbox tool-IPC migration (`dispatch_via_sandbox_ipc` — an unframed
//! single-`read()`/`write_all()` JSON-over-Unix-socket protocol) onto `tddy-rpc`/`tddy-stdio`.
//!
//! Production API under test: `tddy_tools::session_tool_client::dispatch_via_stdio_rpc(client,
//! tool_name, args) -> String`, taking an already-connected `Arc<dyn tddy_rpc::RpcClientTransport>`
//! (dependency-injected, unlike `dispatch_via_sandbox_ipc`'s socket path — this is what makes it
//! testable against an in-process fixture instead of a real Unix socket / sandbox). It calls
//! `connection.ConnectionService/ExecuteTool` with the same `ExecuteToolRequest`/
//! `ExecuteToolResponse` prost messages the existing HTTP path already uses, so a future server
//! side can host one handler for both transports.
//!
//! The peer is `execute-tool-stdio-fixture` (`tests/fixtures/execute_tool_fixture.rs`), a real
//! spawned child process hosting a fake `ConnectionService/ExecuteTool` handler that echoes
//! `args_json` back as `result_json` — enough to prove a tool call round-trips over the stdio RPC
//! channel without a real daemon/sandbox.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{RpcMessage, RpcResult, RpcService, Status};
use tddy_stdio::spawn_child_endpoint;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Bounded safety net around calls otherwise driven entirely by async channels (see fluent-tests
/// "Testing Async Code"). Generous enough to absorb the fixture process's startup even under a
/// loaded machine (a full `cargo test --workspace` run alongside this one has been observed to
/// need close to 500ms for the child's first response), but still well under the 1s
/// integration-test ceiling.
const CALL_TIMEOUT: Duration = Duration::from_millis(900);

/// The fixture never calls back into this test process — any inbound request here would be a
/// bug, so it fails loudly rather than silently no-op'ing.
struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

fn execute_tool_fixture_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_execute-tool-stdio-fixture"))
}

#[tokio::test]
async fn dispatches_a_tool_call_over_stdio_rpc_and_returns_the_result_json() {
    // Given an RPC client wired to a fake ConnectionService/ExecuteTool handler over stdio
    let endpoint = spawn_child_endpoint(execute_tool_fixture_command(), NoCallbackService)
        .await
        .expect("spawn execute-tool-stdio-fixture");
    let client: Arc<dyn tddy_rpc::RpcClientTransport> = endpoint.client.clone();

    // When dispatching a tool call through the new stdio-RPC path
    let args = serde_json::json!({"path": "README.md"});
    let result = timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_stdio_rpc(&client, "Read", &args),
    )
    .await
    .expect("dispatch_via_stdio_rpc timed out");

    // Then the result is exactly the echoed args_json the fake service returned
    assert_eq!(result, args.to_string());
}

#[tokio::test]
async fn round_trips_a_payload_larger_than_a_single_socket_read_without_truncation() {
    // Given an RPC client wired to a fake ConnectionService/ExecuteTool handler over stdio, and a
    // tool result payload comfortably larger than the 64KB buffer the old unframed tool-IPC
    // protocol used for its single read() — the old protocol would silently truncate this
    let endpoint = spawn_child_endpoint(execute_tool_fixture_command(), NoCallbackService)
        .await
        .expect("spawn execute-tool-stdio-fixture");
    let client: Arc<dyn tddy_rpc::RpcClientTransport> = endpoint.client.clone();
    let large_value = "x".repeat(256 * 1024);
    let args = serde_json::json!({"content": large_value});

    // When dispatching a tool call through the new stdio-RPC path
    let result = timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_stdio_rpc(&client, "Write", &args),
    )
    .await
    .expect("dispatch_via_stdio_rpc timed out");

    // Then the full 256KB payload round-trips byte-for-byte — proving tddy-rpc's length-prefixed
    // framing (not a single read()/write_all()) carries the whole message
    assert_eq!(result, args.to_string());
    assert_eq!(result.len(), args.to_string().len());
}
