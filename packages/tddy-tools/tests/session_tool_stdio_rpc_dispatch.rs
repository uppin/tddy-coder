//! Unit tests for the sandbox tool-IPC migration (`dispatch_via_sandbox_ipc` — an unframed
//! single-`read()`/`write_all()` JSON-over-Unix-socket protocol) onto `tddy-rpc`/`tddy-stdio`.
//!
//! Production API under test: `tddy_tools::session_tool_client::dispatch_via_rpc_transport(client,
//! envelope, tool_name, args) -> String`, taking an already-connected
//! `Arc<dyn tddy_rpc::RpcClientTransport>`
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
use tddy_stdio::{spawn_child_endpoint, ChildEndpoint};
use tddy_tools::session_tool_client::SessionToolEnvelope;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

/// How long the fixture process gets to start and announce itself. A safety net covering
/// fork/exec, dynamic linking and tokio start-up, all of which stretch under a parallel suite —
/// deliberately generous, because waiting longer costs nothing when the announcement is prompt.
const FIXTURE_READY: Duration = Duration::from_secs(5);

/// Bounded safety net around a single tool call, which by then is driven entirely by async
/// channels (see fluent-tests "Testing Async Code"). It covers the call and nothing else: the
/// fixture is already serving before any test starts the clock.
const CALL_TIMEOUT: Duration = Duration::from_secs(1);

/// The parent half of the fixture's readiness handshake. The fixture calls
/// `parent.FixtureReadyService/Ready` once its endpoint is serving, and calls nothing else — any
/// other inbound request is a bug, so it fails loudly rather than silently no-op'ing.
struct FixtureReadySignal {
    announced: mpsc::Sender<()>,
}

#[async_trait]
impl RpcService for FixtureReadySignal {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        match (service, method) {
            ("parent.FixtureReadyService", "Ready") => {
                self.announced
                    .send(())
                    .await
                    .expect("hand the readiness announcement to the waiting test");
                RpcResult::Unary(Ok(Vec::new()))
            }
            _ => RpcResult::Unary(Err(Status::unimplemented(format!(
                "test process hosts only the readiness handshake, got {service}/{method}"
            )))),
        }
    }
}

fn execute_tool_fixture_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_execute-tool-stdio-fixture"))
}

/// A spawned fixture that has announced, over the RPC channel itself, that it is serving.
///
/// Waiting for that announcement instead of budgeting for it is what lets [`CALL_TIMEOUT`] measure
/// the call alone.
async fn a_fixture_endpoint_ready_to_serve() -> ChildEndpoint {
    let (announced, mut announcements) = mpsc::channel(1);
    let endpoint = spawn_child_endpoint(
        execute_tool_fixture_command(),
        FixtureReadySignal { announced },
    )
    .await
    .expect("spawn execute-tool-stdio-fixture");

    timeout(FIXTURE_READY, announcements.recv())
        .await
        .expect("the fixture never announced itself ready")
        .expect("the fixture's readiness channel closed before it announced itself");

    endpoint
}

#[tokio::test]
async fn dispatches_a_tool_call_over_stdio_rpc_and_returns_the_result_json() {
    // Given an RPC client wired to a fake ConnectionService/ExecuteTool handler over stdio
    let endpoint = a_fixture_endpoint_ready_to_serve().await;
    let client: Arc<dyn tddy_rpc::RpcClientTransport> = endpoint.client.clone();

    // When dispatching a tool call through the new stdio-RPC path
    let args = serde_json::json!({"path": "README.md"});
    let result = timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_rpc_transport(
            &client,
            &SessionToolEnvelope::default(),
            "Read",
            &args,
        ),
    )
    .await
    .expect("dispatch_via_rpc_transport timed out");

    // Then the result is exactly the echoed args_json the fake service returned
    assert_eq!(result, args.to_string());
}

#[tokio::test]
async fn round_trips_a_payload_larger_than_a_single_socket_read_without_truncation() {
    // Given an RPC client wired to a fake ConnectionService/ExecuteTool handler over stdio, and a
    // tool result payload comfortably larger than the 64KB buffer the old unframed tool-IPC
    // protocol used for its single read() — the old protocol would silently truncate this
    let endpoint = a_fixture_endpoint_ready_to_serve().await;
    let client: Arc<dyn tddy_rpc::RpcClientTransport> = endpoint.client.clone();
    let large_value = "x".repeat(256 * 1024);
    let args = serde_json::json!({"content": large_value});

    // When dispatching a tool call through the new stdio-RPC path
    let result = timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_rpc_transport(
            &client,
            &SessionToolEnvelope::default(),
            "Write",
            &args,
        ),
    )
    .await
    .expect("dispatch_via_rpc_transport timed out");

    // Then the full 256KB payload round-trips byte-for-byte — proving tddy-rpc's length-prefixed
    // framing (not a single read()/write_all()) carries the whole message
    assert_eq!(result, args.to_string());
    assert_eq!(result.len(), args.to_string().len());
}
