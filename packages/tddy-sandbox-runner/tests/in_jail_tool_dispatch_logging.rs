//! What the *host* learns when a jail stops answering its tool calls.
//!
//! Feature: `docs/ft/coder/sandboxed-codebase-mode.md` (criterion 6).
//! Changeset: `docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md`.
//!
//! Its own test binary because `log` has a single process-global logger, and installing a
//! capturing one would otherwise decide what every other test in the crate logs through.
//!
//! The behaviour under test is not decoration. A jail that lets one call pass its budget loses the
//! channel for good — `in_jail_tool_response` carries no request id, so a late answer would be
//! handed to the next caller — and in a `sandboxed` session that channel is the agent's only route
//! to the checkout. The session then looks alive from every angle: the agent is still running, the
//! jail is still up, the tool calls still return. This record is the only thing on the host that
//! says what happened.

mod common;

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Mode};
use tddy_sandbox_runner::{
    run_host_relay_with_in_jail_tools, HostRelayConfig, InJailToolDispatcher, NullToolHandler,
};
use tddy_service::proto::connection::ExecuteToolRequest;
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tokio::sync::mpsc;

const SESSION_ID: &str = "in-jail-tool-dispatch-logging-session";

/// The budget a jail is given to answer in here, in place of the production `IN_JAIL_TOOL_TIMEOUT`
/// (600s): a silent jail has to be waited out for the timeout to happen at all.
const A_SHORT_ANSWER_BUDGET: Duration = Duration::from_millis(150);

/// The test's own guard against the dispatcher hanging, comfortably past the budget above.
const A_TEST_WAIT_LONGER_THAN_THE_BUDGET: Duration = Duration::from_secs(10);

static LOG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn buffer() -> &'static Mutex<Vec<String>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        buffer()
            .lock()
            .unwrap()
            .push(format!("{} {}", record.level(), record.args()));
    }
    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn a_host_recording_what_it_logs() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
}

fn recorded() -> Vec<String> {
    buffer().lock().unwrap().clone()
}

async fn a_relay_attached_to_a_jail_that_never_answers(
) -> (InJailToolDispatcher, tokio::task::JoinHandle<()>) {
    let (endpoint, _captured) =
        serve_fake_over_tcp(Mode::AcceptInJailToolCallsWithoutAnswering).await;
    let client = SandboxServiceClient::connect(endpoint)
        .await
        .expect("connect fake sandbox grpc");
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
    // The relay handle is returned so the caller can hold it: dropping it would abort the reader
    // loop, and a channel with no reader fails for a reason other than the one under test.
    (dispatcher.with_answer_timeout(A_SHORT_ANSWER_BUDGET), relay)
}

/// The transition that ends the session's tool surface has to leave a mark on the host, at a level
/// an operator's default filter shows, naming the call it gave up on and the budget it gave up
/// after — otherwise the only evidence anywhere is inside the agent's own transcript.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn records_a_lost_tool_channel_where_the_hosts_operator_can_find_it() {
    // Given
    a_host_recording_what_it_logs();
    let (dispatcher, _relay) = a_relay_attached_to_a_jail_that_never_answers().await;

    // When — a build that outlasts its budget, which is how this happens in production
    tokio::time::timeout(
        A_TEST_WAIT_LONGER_THAN_THE_BUDGET,
        dispatcher.execute(ExecuteToolRequest {
            tool_name: "Shell".to_string(),
            args_json: r#"{"command":"cargo build"}"#.to_string(),
            ..Default::default()
        }),
    )
    .await
    .expect("a call the jail never answers must settle on its own budget");

    // Then
    let recorded = recorded();
    let reported = recorded
        .iter()
        .find(|line| line.starts_with("ERROR") && line.contains("Shell"))
        .unwrap_or_else(|| {
            panic!("a lost tool channel must be reported as an error naming the call it lost; recorded:\n{}", recorded.join("\n"))
        });
    assert!(
        reported.contains("150ms"),
        "the report must name the budget the call outlasted, was: {reported}"
    );
    assert!(
        reported.contains("restart"),
        "the report must say the session is over rather than reading as one slow call, was: \
         {reported}"
    );
}
