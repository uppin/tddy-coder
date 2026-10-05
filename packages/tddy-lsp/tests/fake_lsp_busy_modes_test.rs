//! The three scripted modes of `fake_lsp` that let a test wait on a server that never gets ready:
//! `--never-quiescent`, `--goes-busy-after-hovers N` and `--hover-never-answers`.
//!
//! The double is the contract every wait-heartbeat test in `tddy-code-restructuring` and
//! `tddy-index-daemon` stands on, so a mode that stops working fails here, by name, rather than as a
//! mysterious timeout in a consumer.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_lsp::{
    Language, LaunchSpec, LspAllowList, LspClient, LspKey, LspRegistry, NotificationEvent,
    NotificationStream, Position,
};
use tddy_task::TaskRegistry;

const A_SOURCE_FILE: &str = "file:///workspace/src/lib.rs";

/// The text a real rust-analyzer reports while a build script runs, which the double repeats.
const THE_BUILD_SCRIPT_LINE: &str = "build script num-bigint run";

/// Long enough for the double's narrated load (about 300 ms) to finish and for a status to arrive.
const LONG_ENOUGH_TO_HEAR_THE_SERVER: Duration = Duration::from_secs(1);

/// Long enough for a local fake to answer anything it is going to answer.
const LONG_ENOUGH_FOR_A_LOCAL_FAKE_TO_ANSWER: Duration = Duration::from_millis(700);

/// A client on a fake server started with `args`, plus a subscriber attached before it says a word.
async fn a_client_on_a_fake_started_with(args: &[&str]) -> (Arc<LspClient>, NotificationStream) {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"));
    spec.args = args.iter().map(|arg| arg.to_string()).collect();
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language: Language::Rust,
        })
        .await
        .expect("a fake language server");
    let client = Arc::clone(&service.client);
    let stream = client.subscribe_notifications();
    (client, stream)
}

/// Every notification that arrives on `stream` within `window`, in arrival order.
async fn everything_heard_within(stream: &mut NotificationStream, window: Duration) -> Vec<Value> {
    let mut heard = Vec::new();
    let listening = async {
        while let NotificationEvent::Received(notification) = stream.recv().await {
            heard.push(notification);
        }
    };
    let _ = tokio::time::timeout(window, listening).await;
    heard
}

/// The `quiescent` flag of each `experimental/serverStatus` among `heard`, in order.
fn the_quiescence_reported(heard: &[Value]) -> Vec<Value> {
    heard
        .iter()
        .filter(|notification| notification["method"] == "experimental/serverStatus")
        .map(|notification| notification["params"]["quiescent"].clone())
        .collect()
}

/// The `message` of each `$/progress` among `heard`, in order, for the reports that carry one.
fn the_progress_messages_among(heard: &[Value]) -> Vec<Value> {
    heard
        .iter()
        .filter(|notification| notification["method"] == "$/progress")
        .map(|notification| notification["params"]["value"]["message"].clone())
        .filter(|message| !message.is_null())
        .collect()
}

#[tokio::test]
async fn a_server_started_never_quiescent_reports_loading_and_never_finishes() {
    // Given a fake started never-quiescent
    let (client, _stream) = a_client_on_a_fake_started_with(&["--never-quiescent"]).await;

    // When a client has given it a while, and takes what it said. It says it all with the
    // handshake, before any subscriber could attach, so the client's own backlog is what holds it.
    tokio::time::sleep(LONG_ENOUGH_TO_HEAR_THE_SERVER).await;
    let heard = client.drain_notifications();

    // Then it said it is busy, named the build script it is on, and never said it is ready
    assert_eq!(the_quiescence_reported(&heard), vec![json!(false)]);
    assert_eq!(
        the_progress_messages_among(&heard),
        vec![json!(THE_BUILD_SCRIPT_LINE)]
    );

    // And it still answers a hover, the way a real server does while it runs build scripts
    let hover = client.hover(A_SOURCE_FILE, Position::at(10, 0)).await;
    assert_eq!(
        hover.expect("the fake answers a hover"),
        Some("fn foo() -> u32".to_string())
    );
}

#[tokio::test]
async fn a_server_that_goes_busy_after_its_first_hover_says_so_after_it() {
    // Given a fake that loads its crate graph and goes busy once it has answered one hover
    let (client, mut stream) =
        a_client_on_a_fake_started_with(&["--goes-busy-after-hovers", "1"]).await;
    let before_any_hover =
        everything_heard_within(&mut stream, LONG_ENOUGH_TO_HEAR_THE_SERVER).await;

    // When it has answered one hover, and then meets a second
    client
        .hover(A_SOURCE_FILE, Position::at(10, 0))
        .await
        .expect("the fake answers a hover");
    let between_the_hovers =
        everything_heard_within(&mut stream, LONG_ENOUGH_FOR_A_LOCAL_FAKE_TO_ANSWER).await;
    client
        .hover(A_SOURCE_FILE, Position::at(10, 0))
        .await
        .expect("the fake still answers a hover while busy");
    let after_the_hover =
        everything_heard_within(&mut stream, LONG_ENOUGH_FOR_A_LOCAL_FAKE_TO_ANSWER).await;

    // Then it was ready before the hover, said nothing between the two, and was busy again on a
    // build script by the time the second was answered
    assert_eq!(
        the_quiescence_reported(&before_any_hover),
        vec![json!(true)]
    );
    assert_eq!(
        the_quiescence_reported(&between_the_hovers),
        Vec::<Value>::new()
    );
    assert_eq!(
        the_quiescence_reported(&after_the_hover),
        vec![json!(false)]
    );
    assert_eq!(
        the_progress_messages_among(&after_the_hover),
        vec![json!(THE_BUILD_SCRIPT_LINE)]
    );
}

#[tokio::test]
async fn a_server_that_never_answers_a_hover_still_answers_everything_else() {
    // Given a fake that swallows hovers
    let (client, _stream) = a_client_on_a_fake_started_with(&["--hover-never-answers"]).await;

    // When a hover and a document-symbol request are asked of it
    let symbols = client.symbols(A_SOURCE_FILE).await;
    let hover = tokio::time::timeout(
        LONG_ENOUGH_FOR_A_LOCAL_FAKE_TO_ANSWER,
        client.hover(A_SOURCE_FILE, Position::at(10, 0)),
    )
    .await;

    // Then the symbols come back, and the hover is still unanswered when the wait ends
    assert_eq!(symbols.expect("the fake answers symbols").len(), 1);
    assert!(
        hover.is_err(),
        "the hover was answered, but --hover-never-answers means it never is: {hover:?}"
    );
}
