//! Acceptance tests: `tddy-tools --mcp` reaching a *remote* daemon's worktree over LiveKit.
//!
//! In a split session the agent runs on one host and its git worktree lives on another
//! (docs/ft/daemon/remote-managed-worktree.md). `tddy-tools` already speaks `tddy-rpc`; LiveKit is
//! simply a third binding for it beside the sandbox's stdio socket and the relay's HTTP/Connect
//! endpoint. `tddy_livekit::RpcClient` already implements `tddy_rpc::RpcClientTransport`, so the
//! dispatch itself is transport-agnostic — what is new is the transport variant and, crucially, the
//! request envelope.
//!
//! The sandbox path sends an **empty** `session_id`/`session_token`/`daemon_instance_id` because the
//! unix socket implies identity. Talking straight to a remote daemon it does not: the callee
//! resolves the worktree from its *own* sessions base keyed by the `session_id` in the request, and
//! authenticates the token. Getting that envelope wrong is the single easiest mistake in the design,
//! so it is pinned here explicitly.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tddy_tools::session_tool_client::{
    clamp_await_block_ms, detect_session_tool_transport, dispatch_via_rpc_transport,
    SessionToolEnvelope, SessionToolTransport, MAX_REMOTE_AWAIT_BLOCK_MS,
};
use tokio::time::{timeout, Duration};

/// Bounded safety net around channel-driven calls (see fluent-tests "Testing Async Code"). Well
/// under the 1s integration ceiling; the peer here is in-process.
const CALL_TIMEOUT: Duration = Duration::from_millis(900);

const LIVEKIT_URL: &str = "ws://127.0.0.1:7880";
const COMMON_ROOM: &str = "tddy-lobby";
const JOIN_TOKEN: &str = "a-scoped-join-jwt";
const CODEBASE_DAEMON: &str = "workstation-b";
const CODEBASE_SERVER_IDENTITY: &str = "daemon-workstation-b";
/// The **B-side workspace** session id — not the agent's own session id on host A.
const CODEBASE_SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a38";
const SESSION_TOKEN: &str = "caller-session-token";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A fake `ConnectionService/ExecuteTool` peer that echoes the decoded request back, so a test can
/// assert on the envelope the client actually put on the wire.
struct EnvelopeEchoingExecuteTool;

#[async_trait]
impl RpcService for EnvelopeEchoingExecuteTool {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        use prost::Message as _;
        use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};

        assert_eq!(service, "connection.ConnectionService");
        assert_eq!(method, "ExecuteTool");
        let request =
            ExecuteToolRequest::decode(message.payload.as_ref()).expect("decode ExecuteToolRequest");
        let echoed = serde_json::json!({
            "session_id": request.session_id,
            "session_token": request.session_token,
            "daemon_instance_id": request.daemon_instance_id,
            "tool_name": request.tool_name,
            "args_json": request.args_json,
        });
        let response = ExecuteToolResponse {
            result_json: echoed.to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        };
        RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

/// The test process hosts no callback service — an inbound call would be a bug.
struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

/// An in-process transport pair whose far end serves `EnvelopeEchoingExecuteTool`.
async fn a_transport_to_an_echoing_peer() -> Arc<dyn RpcClientTransport> {
    let (client_side, server_side) = tokio::io::duplex(64 * 1024);
    let (server_read, server_write) = tokio::io::split(server_side);
    let (_server_client, server_endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        server_read,
        server_write,
        EnvelopeEchoingExecuteTool,
    );
    tokio::spawn(server_endpoint.run());

    let (client_read, client_write) = tokio::io::split(client_side);
    let (client, client_endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(client_read, client_write, NoCallbackService);
    tokio::spawn(client_endpoint.run());
    client
}

fn a_split_session_envelope() -> SessionToolEnvelope {
    SessionToolEnvelope {
        session_id: CODEBASE_SESSION_ID.to_string(),
        session_token: SESSION_TOKEN.to_string(),
        daemon_instance_id: CODEBASE_DAEMON.to_string(),
    }
}

/// Clear every variable the transport detector reads, so a test states its whole environment.
fn clear_transport_env() {
    for key in [
        "TDDY_SANDBOX_TOOL_IPC",
        "TDDY_REMOTE_SESSION_ID",
        "TDDY_REMOTE_SESSION_TOKEN",
        "TDDY_REMOTE_DAEMON_URL",
        "TDDY_REMOTE_DAEMON_INSTANCE_ID",
        "TDDY_REMOTE_LIVEKIT_URL",
        "TDDY_REMOTE_LIVEKIT_ROOM",
        "TDDY_REMOTE_LIVEKIT_TOKEN",
        "TDDY_REMOTE_SERVER_IDENTITY",
    ] {
        std::env::remove_var(key);
    }
}

/// Set the full LiveKit environment a split session's agent is spawned with.
fn set_livekit_transport_env() {
    std::env::set_var("TDDY_REMOTE_LIVEKIT_URL", LIVEKIT_URL);
    std::env::set_var("TDDY_REMOTE_LIVEKIT_ROOM", COMMON_ROOM);
    std::env::set_var("TDDY_REMOTE_LIVEKIT_TOKEN", JOIN_TOKEN);
    std::env::set_var("TDDY_REMOTE_SERVER_IDENTITY", CODEBASE_SERVER_IDENTITY);
    std::env::set_var("TDDY_REMOTE_SESSION_ID", CODEBASE_SESSION_ID);
    std::env::set_var("TDDY_REMOTE_SESSION_TOKEN", SESSION_TOKEN);
    std::env::set_var("TDDY_REMOTE_DAEMON_INSTANCE_ID", CODEBASE_DAEMON);
}

// ---------------------------------------------------------------------------
// Transport detection
// ---------------------------------------------------------------------------

#[test]
#[serial_test::serial]
fn detects_the_livekit_transport_when_the_livekit_environment_is_configured() {
    // Given
    clear_transport_env();
    set_livekit_transport_env();

    // When
    let transport = detect_session_tool_transport();

    // Then
    assert_eq!(
        transport,
        Some(SessionToolTransport::LiveKit {
            url: LIVEKIT_URL.to_string(),
            room: COMMON_ROOM.to_string(),
            token: JOIN_TOKEN.to_string(),
            server_identity: CODEBASE_SERVER_IDENTITY.to_string(),
            session_id: CODEBASE_SESSION_ID.to_string(),
            session_token: SESSION_TOKEN.to_string(),
            daemon_instance_id: CODEBASE_DAEMON.to_string(),
        })
    );
}

#[test]
#[serial_test::serial]
fn prefers_sandbox_ipc_over_livekit_when_both_are_configured() {
    // Given an in-jail agent that also inherited a split session's LiveKit env
    clear_transport_env();
    set_livekit_transport_env();
    std::env::set_var("TDDY_SANDBOX_TOOL_IPC", "/run/tddy/tool.sock");

    // When
    let transport = detect_session_tool_transport();

    // Then — the jail's own socket wins, so an in-jail session never leaves its host by accident
    assert_eq!(
        transport,
        Some(SessionToolTransport::SandboxIpc {
            socket_path: std::path::PathBuf::from("/run/tddy/tool.sock"),
        })
    );
}

#[test]
#[serial_test::serial]
fn no_configured_transport_is_detected_as_none() {
    // Given a bare environment — an agent that is not in a managed session at all
    clear_transport_env();

    // When / Then — the caller reports "not configured" rather than guessing at a transport
    assert_eq!(detect_session_tool_transport(), None);
}

#[test]
#[serial_test::serial]
fn a_livekit_environment_missing_its_join_token_falls_through_to_the_http_transport() {
    // Given LiveKit routing that cannot actually join a room, plus a usable relay URL
    clear_transport_env();
    set_livekit_transport_env();
    std::env::remove_var("TDDY_REMOTE_LIVEKIT_TOKEN");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:9321");

    // When
    let transport = detect_session_tool_transport();

    // Then — a half-configured LiveKit environment must not be selected and then fail at connect
    assert_eq!(
        transport,
        Some(SessionToolTransport::DaemonHttp {
            session_id: CODEBASE_SESSION_ID.to_string(),
            daemon_url: "http://127.0.0.1:9321".to_string(),
            session_token: SESSION_TOKEN.to_string(),
            daemon_instance_id: CODEBASE_DAEMON.to_string(),
        })
    );
}

// ---------------------------------------------------------------------------
// Staying inside the forwarded-stream deadline
// ---------------------------------------------------------------------------
//
// A forwarded stream is killed after `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30s) without a frame, and
// a stalled stream is reported as an *error* rather than a clean end — deliberately, so a truncated
// result can never look complete. A tool that blocks longer than that would therefore surface as a
// transport failure rather than as a slow tool.
//
// Rather than add keepalive frames, the client keeps every blocking call under the deadline: a long
// `Shell` is backgrounded (`block_until_ms: 0` → `job_id`) and polled with `Await` in slices. The
// PRD states that as a constraint; this pins it, so nobody later raises a block time past what the
// transport will carry.

#[test]
fn the_remote_await_block_budget_stays_under_the_forwarded_stream_deadline() {
    // The forwarded-stream idle deadline, mirrored from
    // `tddy_daemon::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`. tddy-tools does not
    // depend on tddy-daemon, so the constant is duplicated here rather than imported.
    const FORWARD_STREAM_IDLE_TIMEOUT_MS: u64 = 30_000;

    // Then — with real headroom, not merely "less than": a block that lands exactly on the deadline
    // races the reaper
    assert!(
        MAX_REMOTE_AWAIT_BLOCK_MS < FORWARD_STREAM_IDLE_TIMEOUT_MS,
        "the Await block budget ({MAX_REMOTE_AWAIT_BLOCK_MS}ms) must stay under the forwarded-stream \
         idle deadline ({FORWARD_STREAM_IDLE_TIMEOUT_MS}ms)"
    );
    let headroom = FORWARD_STREAM_IDLE_TIMEOUT_MS - MAX_REMOTE_AWAIT_BLOCK_MS;
    assert!(
        headroom >= 5_000,
        "at least 5s must remain for the round trip itself; only {headroom}ms spare"
    );
}

#[test]
fn an_await_longer_than_the_budget_is_clamped_to_it() {
    // Given an agent asking to block for five minutes
    let requested = 300_000;

    // When
    let clamped = clamp_await_block_ms(requested);

    // Then — the agent polls again rather than having the transport kill its stream mid-tool
    assert_eq!(clamped, MAX_REMOTE_AWAIT_BLOCK_MS);
}

#[test]
fn an_await_within_the_budget_is_left_alone() {
    // Given a short poll
    let requested = 1_500;

    // When / Then — clamping must not become a floor; a fast tool should return as soon as it is done
    assert_eq!(clamp_await_block_ms(requested), 1_500);
}

#[test]
fn a_non_blocking_await_stays_non_blocking() {
    // When / Then — `0` means "return immediately with the job's status", and clamping it upward
    // would turn every status poll into a blocking call
    assert_eq!(clamp_await_block_ms(0), 0);
}

// ---------------------------------------------------------------------------
// The request envelope
// ---------------------------------------------------------------------------

#[tokio::test]
async fn carries_the_codebase_session_id_and_token_in_the_request_envelope() {
    // Given a transport to a peer that echoes back what it received
    let client = a_transport_to_an_echoing_peer().await;

    // When
    let result = timeout(
        CALL_TIMEOUT,
        dispatch_via_rpc_transport(
            &client,
            &a_split_session_envelope(),
            "Read",
            &serde_json::json!({ "path": "src/main.rs" }),
        ),
    )
    .await
    .expect("the echoing peer must answer within the call timeout");

    // Then — unlike the sandbox path, every identity field must be populated: the remote daemon
    // resolves the worktree by `session_id` against its own sessions base and authenticates the
    // token, so an empty envelope would find no worktree and no user
    let echoed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(echoed["session_id"], CODEBASE_SESSION_ID);
    assert_eq!(echoed["session_token"], SESSION_TOKEN);
    assert_eq!(echoed["daemon_instance_id"], CODEBASE_DAEMON);
}

#[tokio::test]
async fn dispatches_a_tool_call_over_the_rpc_transport_and_returns_the_result_json() {
    // Given
    let client = a_transport_to_an_echoing_peer().await;

    // When
    let result = timeout(
        CALL_TIMEOUT,
        dispatch_via_rpc_transport(
            &client,
            &a_split_session_envelope(),
            "Grep",
            &serde_json::json!({ "pattern": "fn main" }),
        ),
    )
    .await
    .expect("the echoing peer must answer within the call timeout");

    // Then — the tool name and arguments round-trip unaltered
    let echoed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(echoed["tool_name"], "Grep");
    assert_eq!(
        echoed["args_json"],
        serde_json::json!({ "pattern": "fn main" }).to_string()
    );
}
