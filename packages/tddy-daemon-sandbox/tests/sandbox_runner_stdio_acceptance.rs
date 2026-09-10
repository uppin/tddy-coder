//! Acceptance test for `--stdio` on `tddy-sandbox-runner`: `SandboxService` served over the
//! process's own stdin/stdout via `tddy-stdio`, as an alternative to `--grpc-uds`/
//! `--grpc-listen-port`/`--grpc-socket`.
//!
//! Scoped to proving the stdio transport carries `SandboxService` calls end-to-end via the
//! plain unary `Echo` method — mirroring how `tddy-stdio`'s own `tests/rpc_over_stdio.rs` proves
//! transport plumbing via `test.EchoService` rather than full PTY/session-control business logic
//! (already covered by `tddy-sandbox-darwin`'s Seatbelt-confined acceptance tests).
//!
//! See docs/ft/coder/1-WIP/PRD-2026-07-01-stdio-transport-for-grpc-binaries.md (Milestone 3).

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::sandbox::{
    session_frame, EchoRequest, EchoResponse, HostPoll, SessionFrame, SubscribeTerminal,
};
use tddy_stdio::spawn_child_endpoint;
use tokio::process::Command;
use tokio::time::timeout;

/// Bounded safety net around calls otherwise driven entirely by async channels (see fluent-tests
/// "Testing Async Code"). Generous enough to absorb `tddy-sandbox-runner` process startup under
/// CI load, but still well under the 10s E2E ceiling.
const CALL_TIMEOUT: Duration = Duration::from_secs(8);

/// `tddy-sandbox-runner --stdio` never calls back into the test process for this scenario — any
/// inbound request here would be a bug, so it fails loudly rather than silently no-op'ing.
struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// `CARGO_BIN_EXE_tddy-sandbox-runner` is only set by Cargo for binaries of the *current*
/// package; `tddy-daemon` depends on `tddy-sandbox-runner` as a library, not as a bin target of
/// this package, so this falls back to the workspace `target/debug` layout — the same fallback
/// already used by `sandbox_runner_spawn_smoke.rs`'s `sandbox_runner_binary()`.
fn sandbox_runner_exe_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

/// Minimum required args for `tddy-sandbox-runner` to reach the point of serving `SandboxService`,
/// with `--stdio` requested instead of the existing `--grpc-uds`/`--grpc-listen-port`/
/// `--grpc-socket` transport. `--claude-binary /bin/sleep` stands in for a real Claude CLI: this
/// test only exercises the RPC transport, not the PTY/session-control business logic.
fn sandbox_runner_stdio_command(work_dir: &std::path::Path) -> Command {
    let context_dir = work_dir.join("context");
    let tool_ipc_socket = work_dir.join("tool_ipc.sock");
    let ready_marker = work_dir.join("sandbox.ready");
    std::fs::create_dir_all(&context_dir).expect("create context dir");

    // `--grpc-socket` is a required flag on `SandboxRunnerArgs` but unused by any code path
    // (vestigial, superseded by `--grpc-uds`/`--grpc-listen-port`) — a placeholder path satisfies
    // clap without affecting behavior. `--tddy-tools-path` is required by `spawn_claude_pty` to
    // build the claude CLI's `--mcp-config`; `/bin/sleep` stands in for both `--claude-binary`
    // and this path since neither is actually invoked as claude here.
    let grpc_socket = work_dir.join("unused.grpc.sock");
    let mut command = Command::new(sandbox_runner_exe_path());
    command.env_clear().args([
        "--session-id".as_ref(),
        "stdio-acceptance".as_ref(),
        "--context-dir".as_ref(),
        context_dir.as_os_str(),
        "--grpc-socket".as_ref(),
        grpc_socket.as_os_str(),
        "--tool-ipc-socket".as_ref(),
        tool_ipc_socket.as_os_str(),
        "--ready-marker".as_ref(),
        ready_marker.as_os_str(),
        "--claude-binary".as_ref(),
        "/bin/sleep".as_ref(),
        "--tddy-tools-path".as_ref(),
        "/bin/sleep".as_ref(),
        "--model".as_ref(),
        "claude-opus-4-8".as_ref(),
        "--stdio".as_ref(),
    ]);
    command
}

#[tokio::test]
async fn echoes_a_message_over_sandbox_service_served_over_stdio() {
    // Given `tddy-sandbox-runner --stdio` spawned as a child, driven entirely over its
    // stdin/stdout instead of a `--grpc-uds`/`--grpc-listen-port` socket
    let work_dir = std::env::temp_dir().join(format!(
        "tddy-sandbox-runner-stdio-e2e-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&work_dir).expect("create sandbox-runner work dir");
    let runner = sandbox_runner_exe_path();
    assert!(
        runner.exists(),
        "build tddy-sandbox-runner first (./dev cargo build -p tddy-sandbox-runner)"
    );
    let endpoint = spawn_child_endpoint(sandbox_runner_stdio_command(&work_dir), NoCallbackService)
        .await
        .expect("spawn tddy-sandbox-runner --stdio");

    // When calling SandboxService's unary Echo method over that stdio channel
    let request = EchoRequest {
        message: "hello-sandbox-stdio".to_string(),
    };
    let response_bytes = timeout(
        CALL_TIMEOUT,
        endpoint
            .client
            .call_unary("sandbox.SandboxService", "Echo", request.encode_to_vec()),
    )
    .await
    .expect("Echo call timed out")
    .expect("Echo call failed");

    // Then the exact message is echoed back, decodable as a clean RPC frame — proving
    // `SandboxService` is reachable over stdio, not just the removed grpc-uds/grpc-listen-port
    // transport
    let response = EchoResponse::decode(response_bytes.as_slice()).expect("decode EchoResponse");
    assert_eq!(response.message, "hello-sandbox-stdio");
}

#[tokio::test]
async fn streams_pty_output_over_session_channel_served_over_stdio() {
    // Given `tddy-sandbox-runner --stdio` spawned as a child, with `/bin/sleep` standing in for
    // the claude binary — invoked with `--model`/`--session-id`/`--permission-mode` flags (not a
    // numeric duration), `/bin/sleep` writes a usage error to its controlling terminal and exits
    // immediately. Since a PTY connects the child's stdout/stderr to the same fd, that error text
    // is real PTY output the relay's reader thread observes — a deterministic signal for this
    // test, which only needs to prove PTY output flows over the stdio-served SessionChannel (the
    // relay logic itself is already covered end-to-end by tddy-sandbox-darwin's Seatbelt-confined
    // acceptance tests, unchanged by this milestone).
    //
    // Short prefix: the tool-IPC socket path derived from this dir must fit within AF_UNIX's
    // SUN_LEN (104 bytes on macOS) — see docs/dev/1-WIP/CS-2026-07-01-stdio-transport-for-grpc-binaries.md.
    let work_dir = std::env::temp_dir().join(format!("tddy-sc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&work_dir).expect("create sandbox-runner work dir");
    let endpoint = spawn_child_endpoint(sandbox_runner_stdio_command(&work_dir), NoCallbackService)
        .await
        .expect("spawn tddy-sandbox-runner --stdio");
    let (mut sender, mut responses) = endpoint
        .client
        .start_bidi_stream("sandbox.SandboxService", "SessionChannel")
        .expect("start SessionChannel bidi call");

    // When subscribing to the terminal, then polling — SubscribeTerminal establishes interest,
    // HostPoll asks the relay to flush any backlogged terminal output
    let subscribe = SessionFrame {
        payload: Some(session_frame::Payload::SubscribeTerminal(
            SubscribeTerminal {
                session_id: "stdio-acceptance".to_string(),
                terminal_id: "t1".to_string(),
                initial_cols: 80,
                initial_rows: 24,
            },
        )),
    };
    sender
        .send(subscribe.encode_to_vec(), false)
        .await
        .expect("send SubscribeTerminal frame");

    let poll = SessionFrame {
        payload: Some(session_frame::Payload::HostPoll(HostPoll {})),
    };

    // Then a non-empty TerminalOutput frame eventually arrives, decodable as a clean RPC frame —
    // proving frames the relay produces (not just ones the client sends) flow over the stdio
    // transport, not just the removed grpc-uds/grpc-listen-port transport
    let mut received_output = Vec::new();
    'polling: for _ in 0..20 {
        sender
            .send(poll.clone().encode_to_vec(), false)
            .await
            .expect("send HostPoll frame");
        loop {
            let next = timeout(Duration::from_millis(500), responses.recv()).await;
            let Ok(Some(frame)) = next else { break };
            let bytes = frame.expect("stream item error");
            let message = SessionFrame::decode(bytes.as_slice()).expect("decode SessionFrame");
            if let Some(session_frame::Payload::TerminalOutput(output)) = message.payload {
                received_output.extend(output.data);
                break 'polling;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(
        !received_output.is_empty(),
        "expected non-empty PTY output over the stdio-served SessionChannel"
    );
}
