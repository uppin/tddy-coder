//! Document-sync bookkeeping for a host that outlives one request: the version sequence a server
//! is told about must be the client's to keep, not each caller's. Every test runs against the
//! deterministic `fake_lsp` server and asserts on `tddy/documentVersions` — what the server
//! actually received — because a client's version bookkeeping is observable nowhere else.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspClient, LspKey, LspRegistry};
use tddy_task::TaskRegistry;

const A_SOURCE_FILE: &str = "file:///workspace/src/lib.rs";
const ANOTHER_SOURCE_FILE: &str = "file:///workspace/src/main.rs";
const RUST: &str = "rust";

fn fake_allow_list() -> LspAllowList {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    allow
}

/// A client on a running fake server, the way a warm host holds one: behind an `Arc` from the
/// registry, shared by every caller rather than built per operation.
async fn a_warm_shared_client() -> Arc<LspClient> {
    let registry = LspRegistry::new(
        fake_allow_list(),
        TaskRegistry::new(),
        Duration::from_secs(60),
    );
    let service = registry
        .get_or_spawn(LspKey {
            root: PathBuf::from("/workspace"),
            language: Language::Rust,
        })
        .await
        .expect("a fake language server");
    Arc::clone(&service.client)
}

/// The versions the server was told about for `uri`, in arrival order.
async fn versions_the_server_saw(client: &LspClient, uri: &str) -> Vec<i64> {
    let replayed = client
        .request_raw("tddy/documentVersions", Value::Null)
        .await
        .expect("the fake server replays the versions it received");
    replayed
        .get(uri)
        .and_then(Value::as_array)
        .map(|versions| versions.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_second_edit_to_one_document_carries_a_greater_version() {
    // Given an open document on a warm shared client
    let client = a_warm_shared_client().await;
    client
        .did_open(A_SOURCE_FILE, RUST, "fn foo() {}")
        .await
        .expect("open the document");

    // When it is edited twice
    client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 1 }")
        .await
        .expect("first edit");
    client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 2 }")
        .await
        .expect("second edit");

    // Then the server was told about one ascending sequence
    assert_eq!(
        versions_the_server_saw(&client, A_SOURCE_FILE).await,
        vec![1, 2, 3]
    );
}

#[tokio::test]
async fn a_document_edited_through_two_borrows_keeps_one_version_sequence() {
    // Given an open document, edited through one borrow of the shared client
    let client = a_warm_shared_client().await;
    client
        .did_open(A_SOURCE_FILE, RUST, "fn foo() {}")
        .await
        .expect("open the document");
    let first_caller = Arc::clone(&client);
    first_caller
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 1 }")
        .await
        .expect("the first caller's edit");
    drop(first_caller);

    // When an independent borrow of the same client edits it again
    let second_caller = Arc::clone(&client);
    second_caller
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 2 }")
        .await
        .expect("the second caller's edit");

    // Then the sequence continued rather than restarting for the new caller
    assert_eq!(
        versions_the_server_saw(&client, A_SOURCE_FILE).await,
        vec![1, 2, 3]
    );
}

#[tokio::test]
async fn each_document_carries_its_own_version_sequence() {
    // Given two open documents on one client
    let client = a_warm_shared_client().await;
    client
        .did_open(A_SOURCE_FILE, RUST, "fn foo() {}")
        .await
        .expect("open the first document");
    client
        .did_open(ANOTHER_SOURCE_FILE, RUST, "fn main() {}")
        .await
        .expect("open the second document");

    // When only the first is edited, twice
    client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 1 }")
        .await
        .expect("first edit");
    client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 2 }")
        .await
        .expect("second edit");

    // Then the second document's sequence is untouched by the first's
    assert_eq!(
        versions_the_server_saw(&client, A_SOURCE_FILE).await,
        vec![1, 2, 3]
    );
    assert_eq!(
        versions_the_server_saw(&client, ANOTHER_SOURCE_FILE).await,
        vec![1]
    );
}

#[tokio::test]
async fn closing_a_document_and_reopening_it_restarts_its_version_sequence() {
    // Given an open document that has been edited once
    let client = a_warm_shared_client().await;
    client
        .did_open(A_SOURCE_FILE, RUST, "fn foo() {}")
        .await
        .expect("open the document");
    client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 1 }")
        .await
        .expect("an edit");

    // When it is closed and opened again
    client
        .did_close(A_SOURCE_FILE)
        .await
        .expect("close the document");
    client
        .did_open(A_SOURCE_FILE, RUST, "fn foo() {}")
        .await
        .expect("reopen the document");

    // Then the reopened document starts again at 1 rather than continuing from 2
    assert_eq!(
        versions_the_server_saw(&client, A_SOURCE_FILE).await,
        vec![1, 2, 1]
    );
}

#[tokio::test]
async fn changing_a_document_that_was_never_opened_is_refused() {
    // Given a warm shared client with nothing open
    let client = a_warm_shared_client().await;

    // When a document the client never opened is changed
    let result = client
        .did_change(A_SOURCE_FILE, "fn foo() -> u32 { 1 }")
        .await;

    // Then it is refused, and the server is told nothing about that document
    assert!(
        result.is_err(),
        "expected a refusal for a document that was never opened"
    );
    assert_eq!(
        versions_the_server_saw(&client, A_SOURCE_FILE).await,
        Vec::<i64>::new()
    );
}

#[tokio::test]
async fn closing_a_document_that_was_never_opened_is_refused() {
    // Given a warm shared client with nothing open
    let client = a_warm_shared_client().await;

    // When a document the client never opened is closed
    let result = client.did_close(A_SOURCE_FILE).await;

    // Then it is refused rather than silently ignored
    assert!(
        result.is_err(),
        "expected a refusal for a document that was never opened"
    );
}
