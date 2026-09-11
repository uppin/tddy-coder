//! CLI integration tests for `tddy-tools session-hook`.
//!
//! These tests validate the subcommand surface, fail-quiet contract, and argument
//! parsing. No real daemon is required — failure or unreachability must always exit 0.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

/// Build a `tddy-tools` command with TDDY_SOCKET cleared so it never
/// accidentally hits a live session relay in the test environment.
fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

/// `session-hook` (or its kebab alias) must appear in the top-level `--help` output so
/// operators can discover it.
#[test]
fn session_hook_appears_in_help() {
    // When / Then
    tddy_tools_bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("session-hook"));
}

/// `session-hook --help` must describe all required and optional flags.
#[test]
fn session_hook_help_lists_required_flags() {
    // When
    let output = tddy_tools_bin()
        .args(["session-hook", "--help"])
        .output()
        .expect("failed to run tddy-tools session-hook --help");

    // Then
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        combined.contains("--session"),
        "help must mention --session: {combined}"
    );
    assert!(
        combined.contains("--daemon"),
        "help must mention --daemon: {combined}"
    );
    assert!(
        combined.contains("--os-user"),
        "help must mention --os-user: {combined}"
    );
    assert!(
        combined.contains("--hook-token"),
        "help must mention --hook-token: {combined}"
    );
    assert!(
        combined.contains("--event"),
        "help must mention --event: {combined}"
    );
}

/// Running `session-hook` without `--session` must exit with clap error code 2.
#[test]
fn session_hook_requires_session_flag() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--daemon",
            "http://127.0.0.1:8899",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-abc",
            "--event",
            "Stop",
        ])
        .write_stdin(r#"{"hook_event_name":"Stop"}"#)
        .assert()
        .code(2);
}

/// When stdin carries an unknown/no-op event (`PreToolUse`) and the daemon URL is
/// unroutable, the process must still exit 0 — the hook must never block Claude.
///
/// This exercises the short-circuit path: the event maps to `None`, so no network
/// call is attempted at all.
#[test]
fn session_hook_noop_event_exits_zero_without_daemon() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "test-session-noop-1",
            "--daemon",
            "http://127.0.0.1:1", // unroutable port
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-noop",
            "--event",
            "PreToolUse",
        ])
        .write_stdin(r#"{"hook_event_name":"PreToolUse","session_id":"test-session-noop-1"}"#)
        .assert()
        .success(); // exit 0 — fail-quiet contract
}

/// When stdin carries Cursor `sessionStart` (maps to `Started`) but the daemon is unreachable,
/// the process must still exit 0 — fail-quiet contract.
#[test]
fn session_hook_cursor_session_start_unreachable_daemon_exits_zero() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-start-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-start",
        ])
        .write_stdin(r#"{"hook_event_name":"sessionStart","session_id":"cursor-session-start-1"}"#)
        .assert()
        .success();
}

/// Cursor `beforeSubmitPrompt` maps to `Running`; fail-quiet even when daemon is down.
#[test]
fn session_hook_cursor_before_submit_prompt_unreachable_daemon_exits_zero() {
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-running-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-running",
        ])
        .write_stdin(
            r#"{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-session-running-1"}"#,
        )
        .assert()
        .success();
}

/// Cursor `stop` maps to `Done`; stdin `hook_event_name` is used without `--event`.
#[test]
fn session_hook_cursor_stop_unreachable_daemon_exits_zero() {
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-stop-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-stop",
        ])
        .write_stdin(r#"{"hook_event_name":"stop","session_id":"cursor-session-stop-1"}"#)
        .assert()
        .success();
}

/// When stdin carries a `SessionStart` event (maps to `Started`) but the daemon is
/// unreachable, the process must still exit 0.
///
/// This exercises the fail-quiet contract on the network-call path: the RPC fails but
/// the hook must never propagate the error upward (it would block Claude Code).
#[test]
fn session_hook_unreachable_daemon_exits_zero() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "test-session-unreachable-1",
            "--daemon",
            "http://127.0.0.1:1", // port 1 is always closed
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-unreachable",
            "--event",
            "SessionStart",
        ])
        .write_stdin(
            r#"{"hook_event_name":"SessionStart","session_id":"test-session-unreachable-1"}"#,
        )
        .assert()
        .success(); // exit 0 — fail-quiet contract even on connection error
}

// ---------------------------------------------------------------------------
// Which coordinate the two reports are posted to
//
// The fail-quiet contract above makes every assertion in this file blind to the URL: the hook
// exits 0 whether the daemon answered, refused or was never there. So a coordinate that no
// service answers any more — `#unbundle` node 7 moved both methods off
// `connection.ConnectionService` — would leave every test here green while session status, the
// attention alerts read off it and every agent-activity row silently stopped.
//
// These tests close that by serving the **real** `/rpc/{service}/{method}` router with the
// activity service mounted at its own name and nothing else. `MultiRpcService` refuses an
// unmounted service, exactly as the daemon does, so a hook posting to any other coordinate
// records nothing and the assertion fails.
// ---------------------------------------------------------------------------

use std::sync::{Arc, Mutex};

use prost::Message as _;
use tddy_rpc::{MultiRpcService, RpcBridge, RpcMessage, RpcResult, RpcService, ServiceEntry};
use tddy_service::proto::activity::{ReportAgentActivityRequest, ReportSessionStatusRequest};

/// One call the served coordinate answered: the method name the router dispatched and the body it
/// carried.
type RecordedCall = (String, Vec<u8>);

/// An `activity.ActivityService` that answers every method with an empty response and keeps what it
/// was asked. The bodies are kept, not just the method names: a report that reached the right
/// coordinate carrying nobody's session is not a report that landed.
struct RecordingActivityService {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
}

#[async_trait::async_trait]
impl RpcService for RecordingActivityService {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        self.calls
            .lock()
            .expect("recorded calls")
            .push((method.to_string(), message.payload.clone()));
        // Both methods answer with an empty message, so an empty body is the real response shape.
        RpcResult::Unary(Ok(Vec::new()))
    }
}

/// A daemon-shaped `/rpc` surface on an ephemeral loopback port, serving **only**
/// `activity.ActivityService`. Dropping it stops accepting connections.
struct AnActivityEndpoint {
    base_url: String,
    calls: Arc<Mutex<Vec<RecordedCall>>>,
    serving: tokio::task::JoinHandle<()>,
}

impl AnActivityEndpoint {
    async fn start() -> Self {
        let calls: Arc<Mutex<Vec<RecordedCall>>> = Arc::new(Mutex::new(Vec::new()));
        let router = tddy_connectrpc::connect_router(RpcBridge::new(MultiRpcService::new(vec![
            ServiceEntry {
                name: tddy_service::session_activity::ACTIVITY_SERVICE,
                service: Arc::new(RecordingActivityService {
                    calls: Arc::clone(&calls),
                }) as Arc<dyn RpcService>,
            },
        ])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the activity endpoint");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("activity endpoint local addr")
        );
        let serving = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self {
            base_url,
            calls,
            serving,
        }
    }

    /// The body the endpoint was asked `method` with, or `None` when it was never asked — which is
    /// what a hook posting to another coordinate leaves behind.
    fn body_of(&self, method: &str) -> Option<Vec<u8>> {
        self.calls
            .lock()
            .expect("recorded calls")
            .iter()
            .find(|(name, _)| name == method)
            .map(|(_, body)| body.clone())
    }

    fn methods(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("recorded calls")
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }
}

impl Drop for AnActivityEndpoint {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// Run `session-hook` against `daemon`, feeding it `stdin_json`. Blocking, so it runs off the
/// reactor: the endpoint under test is served by this same runtime.
async fn run_session_hook_against(daemon: &str, session: &str, stdin_json: &str) {
    let daemon = daemon.to_string();
    let session = session.to_string();
    let stdin_json = stdin_json.to_string();
    tokio::task::spawn_blocking(move || {
        tddy_tools_bin()
            .args([
                "session-hook",
                "--session",
                &session,
                "--daemon",
                &daemon,
                "--os-user",
                "testuser",
                "--hook-token",
                "tok-coordinate",
            ])
            .write_stdin(stdin_json)
            .assert()
            .success();
    })
    .await
    .expect("the session-hook subprocess ran to completion");
}

/// A `PostToolUse` hook raises **both** reports — it maps to the `executing_tool` status and
/// carries a tool payload — so one run proves both call sites address the coordinate the daemon
/// serves. Each body is decoded, because a request that arrived empty would be a row about no
/// session at all.
#[tokio::test]
async fn posts_both_hook_reports_to_the_activity_service_the_daemon_serves() {
    // Given a daemon-shaped /rpc surface serving activity.ActivityService and nothing else
    let endpoint = AnActivityEndpoint::start().await;

    // When a PostToolUse hook fires against it
    run_session_hook_against(
        &endpoint.base_url,
        "coordinate-session-1",
        r#"{"hook_event_name":"PostToolUse","session_id":"coordinate-session-1","tool_name":"Bash","tool_input":{"command":"cargo build"},"tool_response":{"stdout":"done"}}"#,
    )
    .await;

    // Then the status report landed there, naming the session it was run for
    let status_body = endpoint.body_of("ReportSessionStatus").unwrap_or_else(|| {
        panic!(
            "ReportSessionStatus never reached activity.ActivityService; the endpoint was asked \
             for {:?}. A hook posting to a coordinate nothing serves fails silently — it exits 0 \
             either way",
            endpoint.methods()
        )
    });
    let status = ReportSessionStatusRequest::decode(&status_body[..])
        .expect("decode the ReportSessionStatusRequest the hook posted");
    assert_eq!(status.session_id, "coordinate-session-1");
    assert_eq!(status.status, "ExecutingTool");

    // …and so did the agent-activity report, carrying the tool the hook fired for
    let activity_body = endpoint.body_of("ReportAgentActivity").unwrap_or_else(|| {
        panic!(
            "ReportAgentActivity never reached activity.ActivityService; the endpoint was asked \
             for {:?}",
            endpoint.methods()
        )
    });
    let activity = ReportAgentActivityRequest::decode(&activity_body[..])
        .expect("decode the ReportAgentActivityRequest the hook posted");
    assert_eq!(activity.session_id, "coordinate-session-1");
    assert_eq!(activity.event, "PostToolUse");
    assert_eq!(activity.tool_name, "Bash");
}
