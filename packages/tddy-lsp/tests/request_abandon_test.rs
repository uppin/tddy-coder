//! Abandoning an in-flight request. A caller that has decided to stop — a `^C`d restructure run,
//! a dropped RPC — must not be held until one request's liveness bound expires, and the client it
//! abandoned the request on has to stay usable.
//!
//! Every test runs against the deterministic `fake_lsp` server, whose `tddy/neverAnswers` sends no
//! response at all, so nothing but the caller's own decision can end the wait.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_lsp::{
    Language, LaunchSpec, LspAllowList, LspClient, LspError, LspKey, LspRegistry, Position,
};
use tddy_task::TaskRegistry;
use tokio::sync::oneshot;

const A_SOURCE_FILE: &str = "file:///workspace/src/lib.rs";

/// The hover markdown the fake answers with.
const THE_FAKES_HOVER: &str = "fn foo() -> u32";

/// Longer than this suite could ever run for, so a test that finishes proves the abandon ended
/// the request rather than the per-request timeout doing it.
const A_WAIT_NO_TEST_CAN_OUTLAST: Duration = Duration::from_secs(600);

/// Short enough that an abandon which does not return fails the test instead of hanging the suite.
const A_WAIT_A_LOCAL_FAKE_CANNOT_OUTLAST: Duration = Duration::from_secs(5);

/// A client on a running fake server, held the way a warm host holds one.
async fn a_warm_client() -> Arc<LspClient> {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language: Language::Rust,
        })
        .await
        .expect("a fake language server");
    Arc::clone(&service.client)
}

/// A caller that has already decided to stop, so no test waits on a clock to express it.
fn a_caller_that_has_already_stopped() -> impl Future<Output = ()> {
    let (stop, stopped) = oneshot::channel();
    stop.send(()).expect("the caller decided to stop");
    async move {
        let _ = stopped.await;
    }
}

/// Issue a request the fake never answers and abandon it, bounded so a wedge fails the test.
async fn abandon_an_unanswerable_request(client: &LspClient) -> Result<Value, LspError> {
    tokio::time::timeout(
        A_WAIT_A_LOCAL_FAKE_CANNOT_OUTLAST,
        client.request_abandonable(
            "tddy/neverAnswers",
            json!({}),
            a_caller_that_has_already_stopped(),
        ),
    )
    .await
    .expect("the abandoned request never returned")
}

#[tokio::test]
async fn an_abandoned_request_gives_up_without_waiting_out_the_request_timeout() {
    // Given a client whose per-request wait no test could outlast
    let client = a_warm_client().await;
    client.set_request_timeout(A_WAIT_NO_TEST_CAN_OUTLAST);

    // When a request the server never answers is abandoned
    let outcome = abandon_an_unanswerable_request(&client).await;

    // Then the caller is told it abandoned the request, rather than being held by the bound
    assert!(
        matches!(outcome, Err(LspError::Abandoned)),
        "got {outcome:?}"
    );
}

#[tokio::test]
async fn the_client_answers_the_next_request_after_one_was_abandoned() {
    // Given a client that abandoned a request the server never answered
    let client = a_warm_client().await;
    abandon_an_unanswerable_request(&client)
        .await
        .expect_err("a request nobody waited for cannot have been answered");

    // When the next request is issued on the same client
    let hover = client
        .hover(A_SOURCE_FILE, Position::at(10, 0))
        .await
        .expect("the server answers the request after the abandoned one");

    // Then it is answered, so the abandoned request left nothing wedged behind it
    assert_eq!(hover, Some(THE_FAKES_HOVER.to_string()));
}

/// A server told nothing keeps computing an answer nobody will read, which on a wedged run is the
/// whole problem: the caller stopping has to stop the work, not just the waiting.
#[tokio::test]
async fn abandoning_a_request_tells_the_server_to_stop_working_on_it() {
    // Given a client that abandoned a request the server never answered
    let client = a_warm_client().await;
    abandon_an_unanswerable_request(&client)
        .await
        .expect_err("a request nobody waited for cannot have been answered");

    // When the server is asked which requests it was told to cancel
    let cancelled = client
        .request_raw("tddy/cancelledRequests", Value::Null)
        .await
        .expect("the fake replays the cancellations it received");

    // Then it was told to stop working on exactly the abandoned one
    assert_eq!(cancelled, json!(["tddy/neverAnswers"]));
}

#[tokio::test]
async fn an_abandonable_request_nobody_abandons_returns_the_servers_answer() {
    // Given a warm client and a caller that never decides to stop
    let client = a_warm_client().await;

    // When it issues an abandonable request the server does answer
    let result = client
        .request_abandonable(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": A_SOURCE_FILE },
                "position": { "line": 10, "character": 0 },
            }),
            std::future::pending(),
        )
        .await
        .expect("the server's hover answer");

    // Then it is handed the server's answer, exactly as an ordinary request would be
    assert_eq!(
        result.pointer("/contents/value").and_then(Value::as_str),
        Some(THE_FAKES_HOVER)
    );
}
