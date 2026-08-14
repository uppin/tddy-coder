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
    clamp_remote_block_ms, clamp_remote_blocking_args, detect_session_tool_transport,
    dispatch_session_tool, dispatch_via_rpc_transport, SessionToolEnvelope, SessionToolTransport,
    MAX_REMOTE_BLOCK_MS,
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
        let request = ExecuteToolRequest::decode(message.payload.as_ref())
            .expect("decode ExecuteToolRequest");
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
fn a_livekit_environment_missing_its_join_token_is_reported_as_misconfigured() {
    // Given LiveKit routing that cannot actually join a room, plus a relay URL that could be reached
    clear_transport_env();
    set_livekit_transport_env();
    std::env::remove_var("TDDY_REMOTE_LIVEKIT_TOKEN");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", "http://127.0.0.1:9321");

    // When
    let transport = detect_session_tool_transport();

    // Then — the LiveKit variables are only ever set for a split session, whose worktree is on
    // another host, so no relay on this side reaches it. Detection names the missing variable rather
    // than selecting a transport pointed at the wrong filesystem.
    assert_eq!(
        transport,
        Some(SessionToolTransport::IncompleteLiveKit {
            missing: vec!["TDDY_REMOTE_LIVEKIT_TOKEN"],
        })
    );
}

#[test]
#[serial_test::serial]
fn a_blank_daemon_url_is_not_a_configured_http_transport() {
    // Given what a split session actually exports: `RemoteToolEnv::env_pairs` always exports
    // TDDY_REMOTE_DAEMON_URL, and for a split session its value is deliberately blank
    clear_transport_env();
    std::env::set_var("TDDY_REMOTE_SESSION_ID", CODEBASE_SESSION_ID);
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", "");

    // When / Then — an exported blank configures nothing; accepting it made every tool call fail
    // with "relay connection error: relative URL without a base", naming the wrong subsystem
    assert_eq!(detect_session_tool_transport(), None);
}

#[tokio::test]
#[serial_test::serial]
async fn a_tool_call_in_a_half_configured_livekit_environment_names_the_missing_variable() {
    // Given an agent whose environment lost one LiveKit variable
    clear_transport_env();
    set_livekit_transport_env();
    std::env::remove_var("TDDY_REMOTE_SERVER_IDENTITY");

    // When
    let result = timeout(
        CALL_TIMEOUT,
        dispatch_session_tool("Read", serde_json::json!({ "path": "src/main.rs" })),
    )
    .await
    .expect("a misconfigured environment must be answered without reaching any transport");

    // Then — the same error shape every other failure uses, saying which variable is missing
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(parsed["is_error"], serde_json::Value::Bool(true));
    let error = parsed["error"]
        .as_str()
        .expect("an error result must carry an error string");
    assert!(
        error.contains("TDDY_REMOTE_SERVER_IDENTITY"),
        "the error must name the missing variable rather than blame a transport; got {error}"
    );
}

// ---------------------------------------------------------------------------
// The remote block budget
// ---------------------------------------------------------------------------
//
// Nothing bounds a call on this path: `tddy-tools` calls the codebase daemon's participant directly,
// that daemon routes the call locally, and neither the LiveKit RPC client nor the frame loop carries
// a deadline. A peer that dies after the request is published hangs the tool rather than failing it.
//
// So rather than add keepalive frames, the client keeps every blocking call short: a long `Shell` is
// backgrounded (`block_until_ms: 0` → `job_id`) and polled with `Await` in slices, which turns a
// dead peer into one poll that never returns instead of a session that stopped. The PRD states that
// as a constraint; the budget below pins it.
//
// The same budget also has to fit inside `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30s), the deadline a
// daemon→daemon *forwarded* stream imposes. Nothing forwards today, but a call ever routed through a
// third daemon would be killed at that mark and reported as a transport error — the hardest kind to
// attribute — so the headroom is checked here rather than discovered then.

#[test]
fn the_remote_await_block_budget_stays_under_the_forwarded_stream_deadline() {
    // Mirrored from `tddy_daemon::livekit_peer_discovery::PEER_FORWARD_STREAM_IDLE_TIMEOUT`:
    // tddy-tools does not depend on tddy-daemon, so the value is duplicated rather than imported.
    // KEEP IN SYNC — if the daemon lowers that deadline, this constant must follow or this test
    // goes on passing against a number nothing uses.
    const FORWARD_STREAM_IDLE_TIMEOUT_MS: u64 = 30_000;
    // The round trip itself has to fit between the last frame and the reaper, so "under the
    // deadline" is not enough — the budget needs slack, not just inequality.
    const REQUIRED_HEADROOM_MS: u64 = 5_000;

    // When an agent asks to block for exactly as long as a forwarded stream would tolerate
    let clamped = clamp_remote_block_ms(FORWARD_STREAM_IDLE_TIMEOUT_MS);

    // Then what the clamp actually hands the remote engine still leaves room for the round trip.
    // Asserted through the function rather than over the two constants: comparing the constants is
    // arithmetic the compiler already settles, and would leave this test reporting green whether or
    // not `clamp_remote_block_ms` did anything at all.
    assert!(
        clamped + REQUIRED_HEADROOM_MS <= FORWARD_STREAM_IDLE_TIMEOUT_MS,
        "a clamped block of {clamped}ms plus round-trip headroom ({REQUIRED_HEADROOM_MS}ms) must \
         stay under the forwarded-stream idle deadline ({FORWARD_STREAM_IDLE_TIMEOUT_MS}ms)"
    );
    assert_eq!(
        clamped, MAX_REMOTE_BLOCK_MS,
        "a request at the deadline must be cut to the budget, not passed through"
    );
}

#[test]
fn an_await_longer_than_the_budget_is_clamped_to_it() {
    // Given an agent asking to block for five minutes
    let requested = 300_000;

    // When
    let clamped = clamp_remote_block_ms(requested);

    // Then — the agent polls again rather than having the transport kill its stream mid-tool
    assert_eq!(clamped, MAX_REMOTE_BLOCK_MS);
}

#[test]
fn an_await_within_the_budget_is_left_alone() {
    // Given a short poll
    let requested = 1_500;

    // When / Then — clamping must not become a floor; a fast tool should return as soon as it is done
    assert_eq!(clamp_remote_block_ms(requested), 1_500);
}

#[test]
fn a_non_blocking_await_stays_non_blocking() {
    // When / Then — `0` means "return immediately with the job's status", and clamping it upward
    // would turn every status poll into a blocking call
    assert_eq!(clamp_remote_block_ms(0), 0);
}

// ---------------------------------------------------------------------------
// Clamping the arguments an agent actually sends
// ---------------------------------------------------------------------------
//
// The clamp above bounds a number the caller already holds. What crosses the transport is a JSON
// arguments object, and the two shapes an agent actually emits — `Await {job_id}` and
// `Shell {command}` — name no block time at all. The remote tool engine then applies its own 30s
// default to both, which is over the budget: a ceiling that only rewrites a value that is present
// defends nothing in the common case. So the cases below are about the *arguments*, not the number.

#[test]
fn an_await_that_names_no_timeout_is_given_the_ceiling() {
    // Given the request an agent writes when it just wants to wait for its job
    let args = serde_json::json!({ "job_id": "job-7" });

    // When
    let clamped = clamp_remote_blocking_args("Await", &args);

    // Then — passed through untouched, the remote engine would block on its own 30s default
    assert_eq!(clamped["timeout_ms"].as_u64(), Some(MAX_REMOTE_BLOCK_MS));
}

#[test]
fn a_shell_that_names_no_block_time_is_given_the_ceiling() {
    // Given the request an agent writes for an ordinary command
    let args = serde_json::json!({ "command": "cargo test -p tddy-core" });

    // When
    let clamped = clamp_remote_blocking_args("Shell", &args);

    // Then — `Shell` blocks on the same 30s default as `Await`, so it needs the same ceiling
    assert_eq!(
        clamped["block_until_ms"].as_u64(),
        Some(MAX_REMOTE_BLOCK_MS)
    );
}

#[test]
fn a_backgrounded_shell_stays_backgrounded() {
    // Given the background-job protocol the PRD prescribes for a long command
    let args = serde_json::json!({ "command": "cargo build --release", "block_until_ms": 0 });

    // When
    let clamped = clamp_remote_blocking_args("Shell", &args);

    // Then — `0` returns a `job_id` immediately; raising it to the ceiling would block the very
    // calls the protocol exists to keep non-blocking
    assert_eq!(clamped["block_until_ms"].as_u64(), Some(0));
}

#[test]
fn a_status_poll_await_is_not_turned_into_a_blocking_call() {
    // Given a poll spelled with `tool_await`'s second key
    let args = serde_json::json!({ "job_id": "job-7", "block_until_ms": 0 });

    // When
    let clamped = clamp_remote_blocking_args("Await", &args);

    // Then — the engine reads `timeout_ms` first and never looks at the rest, so inserting a
    // ceiling under that key would silently override the `0` the agent asked for
    assert_eq!(clamped["block_until_ms"].as_u64(), Some(0));
    assert_eq!(clamped.get("timeout_ms"), None);
}

#[test]
fn a_timeout_that_arrived_as_a_float_is_still_clamped() {
    // Given a serializer that emitted the number with a fraction — JSON has one number type, so
    // this is the same request as `30000`
    let args = serde_json::json!({ "job_id": "job-7", "timeout_ms": 30000.0 });

    // When
    let clamped = clamp_remote_blocking_args("Await", &args);

    // Then — the engine reads it as an integer or not at all, so an unclamped float lands on the
    // 30s default: exactly the block the ceiling exists to prevent
    assert_eq!(clamped["timeout_ms"].as_u64(), Some(MAX_REMOTE_BLOCK_MS));
}

#[test]
fn a_negative_timeout_lands_on_the_ceiling_like_any_over_long_request() {
    // Given a value the engine would cast to an effectively unbounded block
    let args = serde_json::json!({ "job_id": "job-7", "timeout_ms": -1 });

    // When
    let clamped = clamp_remote_blocking_args("Await", &args);

    // Then
    assert_eq!(clamped["timeout_ms"].as_u64(), Some(MAX_REMOTE_BLOCK_MS));
}

#[test]
fn an_await_asking_for_less_than_the_ceiling_keeps_its_own_timeout() {
    // Given a short poll
    let args = serde_json::json!({ "job_id": "job-7", "timeout_ms": 1_500 });

    // When / Then — the ceiling must not become a floor
    assert_eq!(
        clamp_remote_blocking_args("Await", &args)["timeout_ms"].as_u64(),
        Some(1_500)
    );
}

#[test]
fn a_tool_that_does_not_block_is_passed_through_unchanged() {
    // Given a `Read`, which returns as soon as it has the file
    let args = serde_json::json!({ "path": "src/main.rs" });

    // When / Then — only the tools that block carry a block time; inventing one elsewhere would
    // send the remote engine an argument it never asked for
    assert_eq!(clamp_remote_blocking_args("Read", &args), args);
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
