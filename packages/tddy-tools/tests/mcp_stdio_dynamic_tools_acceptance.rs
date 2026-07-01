//! Acceptance tests: `tddy-tools --mcp` real MCP stdio handshake exposes and dispatches
//! dynamically-discovered tools (PRD: docs/ft/daemon/remote-codebase-mode.md, AC15/AC18).
//!
//! These spawn the actual compiled `tddy-tools` binary and speak the real newline-delimited
//! JSON-RPC wire protocol over its stdin/stdout (the exact framing `rmcp::transport::stdio()`
//! uses) — the same seam Claude Code talks to — instead of calling `dispatch_dynamic_tool` /
//! `build_dynamic_tool_list` as bare Rust functions. This is the seam that silently stayed at 3
//! static tools while `tddy-sandbox-app --remote-codebase` was running, because
//! `run_mcp_server()` never consulted the dynamic catalog.

use async_trait::async_trait;
use prost::Message;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tddy_rpc::{RpcMessage, RpcResult, RpcService};
use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn the real `tddy-tools --mcp` binary with the given extra env vars.
fn spawn_mcp_server(env: &[(&str, &str)]) -> Child {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"));
    cmd.arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.spawn().expect("spawn tddy-tools --mcp")
}

async fn send_json_line(stdin: &mut ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    tokio::time::timeout(IO_TIMEOUT, stdin.write_all(line.as_bytes()))
        .await
        .expect("write to tddy-tools stdin timed out")
        .expect("write to tddy-tools stdin");
}

async fn read_json_line(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(IO_TIMEOUT, stdout.read_line(&mut line))
        .await
        .expect("read from tddy-tools stdout timed out")
        .expect("read from tddy-tools stdout");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
}

/// Perform the MCP `initialize` handshake (request + `notifications/initialized`).
async fn initialize_mcp_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    send_json_line(
        stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-red-test-client", "version": "0.0.1"}
            }
        }),
    )
    .await;
    read_json_line(stdout).await;
    send_json_line(
        stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
}

/// AC15: `tools/list` over the real MCP stdio wire must include the dynamic exec-tool catalog
/// when `TDDY_SANDBOX_TOOL_IPC` is configured — reproducing the exact bug where
/// `tddy-sandbox-app --remote-codebase` only ever showed 3 static MCP tools to Claude.
#[tokio::test]
async fn mcp_tools_list_over_stdio_includes_dynamic_tools_when_sandbox_ipc_configured() {
    // Given
    let socket_path = tddy_sandbox::SandboxSpec::short_ipc_socket_path("mcplistred");
    let mut child = spawn_mcp_server(&[(
        "TDDY_SANDBOX_TOOL_IPC",
        socket_path.to_str().expect("socket path must be utf8"),
    )]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    send_json_line(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    .await;
    let response = read_json_line(&mut stdout).await;
    let _ = child.kill().await;

    // Then
    let names: Vec<String> = response["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list must return a tools array, got: {response}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        names.contains(&"Read".to_string()),
        "tools/list must advertise the dynamic 'Read' tool when TDDY_SANDBOX_TOOL_IPC is set; \
         got: {names:?}"
    );
    assert!(
        names.contains(&"Write".to_string()),
        "tools/list must advertise the dynamic 'Write' tool when TDDY_SANDBOX_TOOL_IPC is set; \
         got: {names:?}"
    );
}

/// Fake `connection.ConnectionService/ExecuteTool` handler, standing in for the real
/// `ToolExecService` `tddy-sandbox-runner` hosts on the tool-IPC socket. Echoes back a fixed
/// marker plus the requested tool name — enough to prove the call actually reached this fake
/// listener over the RPC-framed wire, not the old raw-JSON protocol.
struct FakeToolExecService;

#[async_trait]
impl RpcService for FakeToolExecService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "connection.ConnectionService");
        assert_eq!(method, "ExecuteTool");
        let request = ExecuteToolRequest::decode(message.payload.as_ref())
            .expect("decode ExecuteToolRequest");
        let response = ExecuteToolResponse {
            result_json: json!({
                "marker": "dynamic-tool-round-trip-ok",
                "tool": request.tool_name
            })
            .to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        };
        RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

/// AC18: a `tools/call` for a dynamically-discovered tool name must actually be forwarded over
/// the configured `TDDY_SANDBOX_TOOL_IPC` socket and return the relay's result — not the
/// "tool not found" error the unwired server returns today.
#[tokio::test]
async fn mcp_tools_call_over_stdio_forwards_dynamic_tool_through_sandbox_ipc() {
    // Given — a fake in-jail tool-IPC listener standing in for tddy-sandbox-runner, speaking the
    // same tddy-rpc-framed protocol the real ToolExecService hosts.
    let socket_path = tddy_sandbox::SandboxSpec::short_ipc_socket_path("mcpcallred");
    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind fake tool ipc socket");
    let fake_ipc = tokio::spawn(async move {
        let (stream, _addr) = listener.accept().await.expect("accept fake ipc connection");
        let (read_half, write_half) = tokio::io::split(stream);
        let (_client, endpoint) =
            tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, FakeToolExecService);
        endpoint.run().await;
    });

    let mut child = spawn_mcp_server(&[(
        "TDDY_SANDBOX_TOOL_IPC",
        socket_path.to_str().expect("socket path must be utf8"),
    )]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    send_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "Read", "arguments": {"path": "README.md"}}
        }),
    )
    .await;
    let response = read_json_line(&mut stdout).await;
    let _ = child.kill().await;
    tokio::time::timeout(IO_TIMEOUT, fake_ipc)
        .await
        .expect("fake ipc listener task timed out")
        .expect("fake ipc listener task panicked");

    // Then
    assert!(
        response.to_string().contains("dynamic-tool-round-trip-ok"),
        "tools/call for a dynamic tool must forward through TDDY_SANDBOX_TOOL_IPC and return the \
         relay's result; got: {response}"
    );
}
