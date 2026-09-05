//! The host sending tool calls **into** a jail: `in_jail_tool_request` / `in_jail_tool_response`
//! driven from the shared host relay.
//!
//! Feature: `docs/ft/coder/sandboxed-codebase-mode.md` (criterion 6).
//! Changeset: `docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md`.
//!
//! `host_relay_dispatch.rs` covers the other direction — the jail asking the host. This is the
//! mirror: the relay is the one thing holding the `SessionChannel`, because a jail that runs the
//! build needs its CONNECT tunnels on the same channel as its tool calls, so the dispatcher has to
//! live on the relay rather than beside it.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Captured, Mode};
use tddy_sandbox_runner::{
    run_host_relay_with_in_jail_tools, HostRelayConfig, InJailToolDispatcher, NullToolHandler,
};
use tddy_service::proto::connection::ExecuteToolRequest;
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tokio::sync::mpsc;

const SESSION_ID: &str = "in-jail-tool-dispatch-session";

/// What the tests here give a jail to answer in, in place of the production `IN_JAIL_TOOL_TIMEOUT`
/// (600s — a real `cargo build` is allowed to be slow). A silent jail has to be waited out for a
/// timeout to be observable at all, so the budget is narrowed to keep that wait inside a test run.
const A_SHORT_ANSWER_BUDGET: Duration = Duration::from_millis(150);

/// The test's own guard against the dispatcher hanging: comfortably past the budget above, so it
/// only fires where the call failed to settle on its own.
const A_TEST_WAIT_LONGER_THAN_THE_BUDGET: Duration = Duration::from_secs(10);

async fn connect(endpoint: String) -> SandboxServiceClient<tonic::transport::Channel> {
    SandboxServiceClient::connect(endpoint)
        .await
        .expect("connect fake sandbox grpc")
}

/// A relay attached to a fake jail in `mode`, plus the dispatcher for sending calls into it.
async fn a_relay_attached_to(
    mode: Mode,
) -> (
    InJailToolDispatcher,
    Arc<Mutex<Captured>>,
    tokio::task::JoinHandle<()>,
) {
    let (endpoint, captured) = serve_fake_over_tcp(mode).await;
    let client = connect(endpoint).await;
    let (terminal_tx, _terminal_rx) = mpsc::unbounded_channel::<Bytes>();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
    let (relay, dispatcher) = run_host_relay_with_in_jail_tools(
        client,
        NullToolHandler,
        HostRelayConfig::new(SESSION_ID, terminal_tx),
        stdin_rx,
    )
    .await
    .expect("the relay must attach to the jail");
    (dispatcher, captured, relay)
}

fn a_shell_call(command: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        tool_name: "Shell".to_string(),
        args_json: format!(r#"{{"command":"{command}"}}"#),
        ..Default::default()
    }
}

/// The whole point of the dispatcher: a tool call made on the host runs inside the jail, and the
/// jail's answer is what the caller gets.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatches_a_tool_call_into_the_jail_and_returns_the_jails_answer() {
    // Given
    let (dispatcher, captured, _relay) = a_relay_attached_to(Mode::ServeInJailTools {
        result_json: r#"{"stdout":"from inside the jail"}"#.to_string(),
    })
    .await;

    // When
    let response = dispatcher.execute(a_shell_call("ls")).await;

    // Then
    assert!(
        !response.is_error,
        "the call must reach the jail; error was '{}'",
        response.error_message
    );
    assert_eq!(response.result_json, r#"{"stdout":"from inside the jail"}"#);
    let asked = &captured.lock().unwrap().in_jail_tool_requests;
    assert_eq!(
        asked
            .iter()
            .map(|r| r.tool_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Shell"],
        "the jail must have been asked for exactly the tool the host dispatched"
    );
}

/// `in_jail_tool_response` carries no request id, so two calls in flight would have no way to tell
/// which answer is whose. The dispatcher serialises them rather than leaving that to its callers —
/// which, being independent MCP tool calls, could not coordinate anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn keeps_one_in_jail_tool_call_outstanding_at_a_time() {
    // Given
    let (dispatcher, captured, _relay) = a_relay_attached_to(Mode::ServeInJailTools {
        result_json: r#"{"stdout":"ok"}"#.to_string(),
    })
    .await;
    let dispatcher = Arc::new(dispatcher);

    // When — three callers dispatch at once, as three concurrent MCP tool calls would.
    let calls = (0..3).map(|i| {
        let dispatcher = Arc::clone(&dispatcher);
        tokio::spawn(async move { dispatcher.execute(a_shell_call(&format!("echo {i}"))).await })
    });
    let responses = futures_util::future::join_all(calls).await;

    // Then
    for response in &responses {
        let response = response.as_ref().expect("each dispatch task must finish");
        assert!(
            !response.is_error,
            "every serialised call must still be answered; error was '{}'",
            response.error_message
        );
    }
    let captured = captured.lock().unwrap();
    assert!(
        !captured.saw_concurrent_in_jail_calls,
        "a second call reached the jail before the first was answered — its response would be \
         unattributable"
    );
    assert_eq!(
        captured.in_jail_tool_requests.len(),
        3,
        "serialising must not drop a call"
    );
}

/// A jail that goes silent mid-call is the case a closed channel does not cover: nothing signals
/// it, so without a budget the call waits forever — and it waits holding the turnstile, so every
/// later call from the agent wedges behind it with no error any of them can report.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answers_with_an_error_when_the_jail_takes_the_call_and_never_answers() {
    // Given
    let (dispatcher, _captured, _relay) =
        a_relay_attached_to(Mode::AcceptInJailToolCallsWithoutAnswering).await;
    let dispatcher = dispatcher.with_answer_timeout(A_SHORT_ANSWER_BUDGET);

    // When
    let response = tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(a_shell_call("cargo build")),
    )
    .await
    .expect("a call the jail never answers must settle on its own budget");

    // Then
    assert!(
        response.is_error,
        "a call that timed out must fail; it returned: {}",
        response.result_json
    );
    assert!(
        response.error_message.contains("did not answer"),
        "the failure must name what happened, was: {}",
        response.error_message
    );
}

/// Once an answer has gone missing the channel is spent: the answer may still arrive, and carrying
/// no request id it would be handed to whichever call is parked next. Refusing the later calls is
/// what keeps a slow answer from being read as somebody else's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refuses_later_calls_once_a_jail_has_let_an_answer_go_missing() {
    // Given — a first call the jail never answered
    let (dispatcher, captured, _relay) =
        a_relay_attached_to(Mode::AcceptInJailToolCallsWithoutAnswering).await;
    let dispatcher = dispatcher.with_answer_timeout(A_SHORT_ANSWER_BUDGET);
    tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(a_shell_call("cargo build")),
    )
    .await
    .expect("the first call must settle on its budget before a second is made");

    // When
    let response = tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(a_shell_call("ls")),
    )
    .await
    .expect("a call made after a lost answer must fail immediately, not wait its own budget out");

    // Then
    assert!(
        response.is_error,
        "the later call must be refused; it returned: {}",
        response.result_json
    );
    assert_eq!(
        captured.lock().unwrap().in_jail_tool_requests.len(),
        1,
        "the refused call must never have been sent into the jail"
    );
}

/// A jail whose channel dies mid-call must end the call, not hold it. The caller is an MCP tool
/// call the agent is blocked on: an error it can report beats a wait it cannot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answers_with_an_error_when_the_jails_channel_closes_before_the_response() {
    // Given
    let (dispatcher, _captured, _relay) = a_relay_attached_to(Mode::CloseOnInJailToolCall).await;

    // When
    let response = tokio::time::timeout(
        Duration::from_secs(10),
        dispatcher.execute(a_shell_call("ls")),
    )
    .await
    .expect("a dispatch to a dead jail must settle, not hang");

    // Then
    assert!(
        response.is_error,
        "a call the jail never answered must fail; it returned: {}",
        response.result_json
    );
    assert!(
        !response.error_message.is_empty(),
        "the failure must say what happened"
    );
}

/// What the refusal has to be *about*. In a `sandboxed` session these calls are the agent's only
/// route to the checkout, so a lost channel takes the whole tool surface with it — an agent told
/// only that answers can no longer be attributed will keep trying tools that will never work
/// again, and the session sits there looking alive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tells_a_later_caller_the_session_has_to_be_restarted() {
    // Given — a first call the jail never answered
    let (dispatcher, _captured, _relay) =
        a_relay_attached_to(Mode::AcceptInJailToolCallsWithoutAnswering).await;
    let dispatcher = dispatcher.with_answer_timeout(A_SHORT_ANSWER_BUDGET);
    tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(a_shell_call("cargo build")),
    )
    .await
    .expect("the first call must settle on its budget before a second is made");

    // When
    let response = tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(a_shell_call("ls")),
    )
    .await
    .expect("a call made after a lost answer must fail immediately");

    // Then
    assert!(
        response.error_message.contains("restart"),
        "the refusal must tell the agent what to do about a session it can no longer use tools \
         in, was: {}",
        response.error_message
    );
}
