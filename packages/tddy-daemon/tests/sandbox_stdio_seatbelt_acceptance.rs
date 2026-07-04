//! Acceptance: `tddy-sandbox-runner --stdio`, spawned inside a real Seatbelt jail via
//! `spawn_sandbox_runner` (not spawned directly, as `sandbox_runner_stdio_acceptance.rs` does) —
//! proves the full production spawn path (jail creation, piped stdio instead of an egress-log
//! redirect, async pipe wrapping, `StdioEndpoint`) carries an RPC round trip end-to-end.
//!
//! See docs/ft/coder/1-WIP/PRD-2026-07-01-stdio-transport-for-grpc-binaries.md (Milestone 3).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_daemon::sandbox_session::{
    bridge_sandbox_stdio, build_sandbox_runner_env, pick_free_loopback_port, spawn_sandbox_runner,
    SandboxRunnerSpawn,
};
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::sandbox::{EchoRequest, EchoResponse};

const CALL_TIMEOUT: Duration = Duration::from_secs(8);

fn sandbox_runner_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// `tddy-sandbox-runner --stdio` never calls back into the daemon for this scenario — any inbound
/// request here would be a bug, so it fails loudly rather than silently no-op'ing.
struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "daemon hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// **round_trips_an_echo_over_stdio_through_a_real_seatbelt_jail**: `spawn_sandbox_runner` spawns
/// `--stdio` inside Seatbelt with piped stdio (not the egress-log redirect used otherwise —
/// see `tddy_sandbox_darwin::spawn_plan`'s `stdio_mode` branch); `bridge_sandbox_stdio` wraps
/// those pipes as async and hosts an `RpcService` endpoint over them, exactly as production code
/// would after this milestone. `SandboxService/Echo` round-tripping proves every link in that
/// chain — jail spawn, piped (not logged) stdio, blocking→async pipe conversion, RPC framing.
#[tokio::test]
async fn round_trips_an_echo_over_stdio_through_a_real_seatbelt_jail() {
    // Given a real Seatbelt-jailed `tddy-sandbox-runner --stdio`
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    // Canonicalize once the dirs exist: TMPDIR is reached via the /tmp -> /private/tmp symlink,
    // and Seatbelt matches the socket bind path against the (canonical) SBPL rules — same
    // reasoning as `sandbox_runner_spawn_smoke.rs`.
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");

    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");
    assert!(tools.exists(), "build tddy-tools first");

    let ready_marker = project.join("sandbox.ready");
    // `--grpc-socket` is a required flag on `SandboxRunnerArgs` but unused once `--stdio` is
    // passed (vestigial, superseded) — a placeholder path satisfies clap without affecting
    // behavior, same as `sandbox_runner_stdio_acceptance.rs`.
    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "stdio-seatbelt".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        project.join("unused.grpc.sock").to_string_lossy().into(),
        "--tool-ipc-socket".into(),
        project.join("tool_ipc.sock").to_string_lossy().to_string(),
        "--tddy-tools-path".into(),
        tools.to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--claude-binary".into(),
        "/bin/sleep".into(),
        "--model".into(),
        "claude-opus-4-8".into(),
        "--permission-mode".into(),
        "auto".into(),
        "--stdio".into(),
    ];

    let mut env = build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "stdio-seatbelt",
        &project.join("tool_ipc.sock"),
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let shim_port = pick_free_loopback_port().expect("egress shim port");
    let profile_path = project.join("profile.sb");

    let mut handle = spawn_sandbox_runner(SandboxRunnerSpawn {
        project_root: project.clone(),
        scratch_dir: scratch,
        egress_dir: egress,
        profile_path,
        runner_argv,
        env,
        loopback_allow_ports: vec![shim_port],
        ipc_socket: None,
        mounts: vec![],
        host_home: None,
    })
    .expect("spawn sandbox-runner");

    // Wait for the ready marker (`--stdio` mode writes "stdio" instead of a port number) — same
    // polling pattern as `sandbox_runner_spawn_smoke.rs`, so a jail that fails to boot (e.g. an
    // SBPL denial) fails fast with a diagnosable exit status rather than hanging until timeout.
    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            panic!("sandbox child died before ready marker: {reason}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready_marker.exists(), "ready marker must appear");

    // When bridging the jailed process's piped stdio into an RPC client and calling Echo
    let (client, _run_handle) =
        bridge_sandbox_stdio(&mut handle, NoCallbackService).expect("bridge sandbox stdio");
    let request = EchoRequest {
        message: "hello-through-seatbelt".to_string(),
    };
    let response_bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary("sandbox.SandboxService", "Echo", request.encode_to_vec()),
    )
    .await
    .expect("Echo call timed out")
    .expect("Echo call failed");

    // Then the exact message is echoed back — proving the real jail-spawn stdio path, not just
    // a directly-spawned (unsandboxed) tddy-sandbox-runner process
    let response = EchoResponse::decode(response_bytes.as_slice()).expect("decode EchoResponse");
    assert_eq!(response.message, "hello-through-seatbelt");

    handle.child_mut().kill().ok();
    handle.child_mut().wait().ok();
}

/// A `HostToolHandler` that returns a fixed marker result — enough to prove a `ToolRequest`
/// frame reached the host and a `ToolResponse` made it back to the jail over the stdio-served
/// `SessionChannel`, without depending on any particular real tool implementation.
struct FakeToolHandler;

#[async_trait]
impl tddy_sandbox_runner::HostToolHandler for FakeToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> tddy_sandbox_runner::ExecuteToolResponse {
        tddy_sandbox_runner::ExecuteToolResponse {
            result_json: serde_json::json!({
                "marker": "host-relay-stdio-round-trip-ok",
                "tool": tool_name
            })
            .to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }
}

/// **dispatches_a_tool_call_through_run_host_relay_over_stdio_through_a_real_seatbelt_jail**:
/// exercises the actual production `run_host_relay` function (not a raw `SessionFrame` sender, as
/// the Echo/PTY tests above do) with the new `StdioSandboxClient`, through a real Seatbelt jail —
/// the same code path `tddy-daemon`'s real session lifecycle would run once it's switched over
/// (not yet done — see the changeset's Technical Debt). A concurrent tool call, dispatched from
/// inside the jail exactly as a real MCP tool call would be (via
/// `tddy_tools::session_tool_client::dispatch_via_sandbox_ipc`, the same function
/// `tddy-tools --mcp` uses), proves `ToolRequest`/`ToolResponse` frames round-trip correctly
/// through `StdioSandboxClient`'s send/receive loop, not just Echo or raw PTY bytes.
#[tokio::test]
async fn dispatches_a_tool_call_through_run_host_relay_over_stdio_through_a_real_seatbelt_jail() {
    // Given a real Seatbelt-jailed `tddy-sandbox-runner --stdio`
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");

    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");
    assert!(tools.exists(), "build tddy-tools first");

    let ready_marker = project.join("sandbox.ready");
    let tool_ipc_socket = project.join("tool_ipc.sock");
    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "stdio-relay-seatbelt".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        project.join("unused.grpc.sock").to_string_lossy().into(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
        "--tddy-tools-path".into(),
        tools.to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--claude-binary".into(),
        "/bin/sleep".into(),
        "--model".into(),
        "claude-opus-4-8".into(),
        "--permission-mode".into(),
        "auto".into(),
        "--stdio".into(),
    ];

    let mut env = build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "stdio-relay-seatbelt",
        &tool_ipc_socket,
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let shim_port = pick_free_loopback_port().expect("egress shim port");
    let profile_path = project.join("profile.sb");

    let mut handle = spawn_sandbox_runner(SandboxRunnerSpawn {
        project_root: project.clone(),
        scratch_dir: scratch,
        egress_dir: egress.clone(),
        profile_path,
        runner_argv,
        env,
        loopback_allow_ports: vec![shim_port],
        ipc_socket: None,
        mounts: vec![],
        host_home: None,
    })
    .expect("spawn sandbox-runner");

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            panic!("sandbox child died before ready marker: {reason}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready_marker.exists(), "ready marker must appear");

    // When bridging the jailed process's piped stdio, driving it via the real run_host_relay
    let (client, _run_handle) =
        bridge_sandbox_stdio(&mut handle, NoCallbackService).expect("bridge sandbox stdio");
    let stdio_client = tddy_sandbox_runner::StdioSandboxClient::new(client);
    let (_stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel();
    let (term_tx, _term_rx) = tokio::sync::mpsc::unbounded_channel();
    tddy_sandbox_runner::run_host_relay(
        stdio_client,
        FakeToolHandler,
        tddy_sandbox_runner::HostRelayConfig::new("stdio-relay-seatbelt", term_tx),
        stdin_rx,
    )
    .await
    .expect("run_host_relay over stdio");

    // ...and, concurrently, a tool call dispatched from inside the jail exactly as a real MCP
    // tool call would be
    let ipc_result = tokio::time::timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_sandbox_ipc(
            &tool_ipc_socket,
            "Read",
            &serde_json::json!({"path": "README.md"}),
        ),
    )
    .await
    .expect("tool dispatch timed out");

    // Then the fake handler's marker result comes back through the whole chain: tool-IPC socket
    // -> SandboxSessionRelay -> stdio-served SessionChannel -> StdioSandboxClient ->
    // run_host_relay -> FakeToolHandler -> back the same way
    let parsed: serde_json::Value = serde_json::from_str(&ipc_result).expect("valid json response");
    assert_eq!(
        parsed.get("marker").and_then(|v| v.as_str()),
        Some("host-relay-stdio-round-trip-ok"),
        "expected the fake handler's marker in the tool result: {parsed}"
    );
    assert_eq!(parsed.get("tool").and_then(|v| v.as_str()), Some("Read"));

    handle.child_mut().kill().ok();
    handle.child_mut().wait().ok();
}
