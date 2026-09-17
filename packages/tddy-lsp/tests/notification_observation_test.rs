//! Observing a server's notifications must not consume them: a status reader and a running
//! operation on the same root both need the `$/progress` a loading server reports, and a drain
//! hands whichever got there first everything the other needed.
//!
//! Every test runs against the deterministic `fake_lsp` server in its `--cold-hovers` mode, which
//! reports progress exactly when it answers a hover with nothing — the shape a real server takes
//! while it loads a crate graph and answers no requests.

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

/// Long enough for a local fake to answer, short enough that a subscriber which never receives
/// fails the test instead of hanging the suite.
const A_WAIT_A_LOCAL_FAKE_CANNOT_OUTLAST: Duration = Duration::from_secs(5);

/// A client on a fake server that reports progress instead of answering its first `cold_hovers`
/// hovers, held the way a warm host holds one: behind an `Arc` from the registry.
async fn a_client_on_a_loading_server(cold_hovers: u32) -> Arc<LspClient> {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"));
    spec.args = vec!["--cold-hovers".to_string(), cold_hovers.to_string()];
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
    Arc::clone(&service.client)
}

/// Ask for one hover, which a loading fake precedes with a `$/progress` notification.
///
/// The notification is dispatched before the answer that ends this await, so a test that returns
/// from here knows the progress has arrived without polling for it.
async fn the_server_reports_progress_once(client: &LspClient) {
    client
        .hover(A_SOURCE_FILE, Position::at(10, 0))
        .await
        .expect("the fake answers a cold hover with nothing");
}

/// The next notification on `stream`, or a panic naming what never arrived.
async fn next_notification(stream: &mut NotificationStream) -> Value {
    match tokio::time::timeout(A_WAIT_A_LOCAL_FAKE_CANNOT_OUTLAST, stream.recv()).await {
        Ok(NotificationEvent::Received(notification)) => notification,
        Ok(other) => panic!("expected a notification, got {other:?}"),
        Err(_) => panic!("no notification arrived within {A_WAIT_A_LOCAL_FAKE_CANNOT_OUTLAST:?}"),
    }
}

/// The `$/progress` params the fake sends with the first hover it cannot answer.
fn the_first_progress_report() -> Value {
    json!({
        "token": "fake-lsp-indexing",
        "value": { "kind": "begin", "title": "loading crate graph", "percentage": 0 },
    })
}

#[tokio::test]
async fn two_subscribers_each_receive_the_same_progress_notification() {
    // Given two observers of one loading server — a status reader and a running operation
    let client = a_client_on_a_loading_server(1).await;
    let mut status_reader = client.subscribe_notifications();
    let mut running_operation = client.subscribe_notifications();

    // When the server reports progress once
    the_server_reports_progress_once(&client).await;

    // Then each observer sees it, neither having taken it from the other
    assert_eq!(
        next_notification(&mut status_reader).await["params"],
        the_first_progress_report()
    );
    assert_eq!(
        next_notification(&mut running_operation).await["params"],
        the_first_progress_report()
    );
}

/// The bridged restructuring path folds `$/progress` out of `drain_notifications` into its own
/// account of what the server is doing. A subscriber attached alongside it must not be what stops
/// that account from being written.
#[tokio::test]
async fn a_drain_still_yields_a_notification_a_subscriber_observed() {
    // Given a subscriber attached to a loading server
    let client = a_client_on_a_loading_server(1).await;
    let mut subscriber = client.subscribe_notifications();

    // When the server reports progress and the subscriber observes it
    the_server_reports_progress_once(&client).await;
    let observed = next_notification(&mut subscriber).await;

    // Then the drain yields the same notification, verbatim
    assert_eq!(client.drain_notifications(), vec![observed]);
}

#[tokio::test]
async fn a_subscriber_receives_only_notifications_that_arrive_after_it_attached() {
    // Given a server that has already reported progress once
    let client = a_client_on_a_loading_server(2).await;
    the_server_reports_progress_once(&client).await;

    // When an observer attaches and the server reports again
    let mut latecomer = client.subscribe_notifications();
    the_server_reports_progress_once(&client).await;

    // Then it is handed the live report rather than the backlog it was not there for
    assert_eq!(
        next_notification(&mut latecomer).await["params"],
        json!({
            "token": "fake-lsp-indexing",
            "value": { "kind": "report", "percentage": 1 },
        })
    );
}
