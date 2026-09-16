//! Re-binding a target on a host that outlives one request. A bind announces the target's sources
//! to the server, and on a second bind it has to *extend* each open document's version sequence
//! rather than restart it — a server is entitled to ignore an edit whose version went backwards.
//!
//! Every test runs against the deterministic `fake_lsp` server and asserts on
//! `tddy/documentSyncLog`, the document-sync notifications the server actually received. Which of
//! `didOpen` and `didChange` a bind sent is observable nowhere else: a re-open at version 1 and an
//! edit at version 1 look identical from the version alone.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_lsp::{
    DocumentSource, Language, LaunchSpec, LspAllowList, LspClient, LspKey, LspRegistry, LspService,
};
use tddy_task::TaskRegistry;

const A_SOURCE_FILE: &str = "file:///workspace/src/lib.rs";
const ANOTHER_SOURCE_FILE: &str = "file:///workspace/src/main.rs";

fn a_registry() -> LspRegistry {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
}

fn a_workspace() -> LspKey {
    LspKey {
        root: PathBuf::from("/workspace"),
        language: Language::Rust,
    }
}

fn a_source(uri: &str, text: &str) -> DocumentSource {
    DocumentSource {
        uri: uri.to_string(),
        language_id: "rust".to_string(),
        text: text.to_string(),
    }
}

async fn bind(registry: &LspRegistry, srcs: &[DocumentSource]) -> Arc<LspService> {
    registry
        .bind_target(a_workspace(), srcs)
        .await
        .expect("the target binds to a fake language server")
}

/// The document-sync notifications the server received for `uri`, in arrival order.
async fn sync_log_for(client: &LspClient, uri: &str) -> Value {
    let replayed = client
        .request_raw("tddy/documentSyncLog", Value::Null)
        .await
        .expect("the fake replays the sync notifications it received");
    replayed.get(uri).cloned().unwrap_or(Value::Null)
}

/// The latest contents the server was told `uri` holds.
async fn text_the_server_holds(client: &LspClient, uri: &str) -> Value {
    let replayed = client
        .request_raw("tddy/documentTexts", Value::Null)
        .await
        .expect("the fake replays the contents it was announced");
    replayed.get(uri).cloned().unwrap_or(Value::Null)
}

#[tokio::test]
async fn a_second_bind_of_an_open_source_changes_it_rather_than_reopening_it() {
    // Given a target already bound once, so its source is open at version 1
    let registry = a_registry();
    let srcs = [a_source(A_SOURCE_FILE, "fn foo() -> u32 { 0 }\n")];
    bind(&registry, &srcs).await;

    // When the same target is bound again with the same source
    let service = bind(&registry, &srcs).await;

    // Then the server was told of an edit at the next version, not of a second open at 1
    assert_eq!(
        sync_log_for(&service.client, A_SOURCE_FILE).await,
        json!([
            { "method": "textDocument/didOpen", "version": 1 },
            { "method": "textDocument/didChange", "version": 2 },
        ])
    );
}

#[tokio::test]
async fn a_source_edited_between_two_binds_reaches_the_server() {
    // Given a target bound once with its original source
    let registry = a_registry();
    bind(
        &registry,
        &[a_source(A_SOURCE_FILE, "fn foo() -> u32 { 0 }\n")],
    )
    .await;

    // When it is bound again after the file changed on disk
    let service = bind(
        &registry,
        &[a_source(A_SOURCE_FILE, "fn foo() -> u32 { 1 }\n")],
    )
    .await;

    // Then the server holds the new contents
    assert_eq!(
        text_the_server_holds(&service.client, A_SOURCE_FILE).await,
        json!("fn foo() -> u32 { 1 }\n")
    );
}

#[tokio::test]
async fn a_source_first_seen_on_a_later_bind_is_opened_at_version_one() {
    // Given a target bound once with a single source
    let registry = a_registry();
    bind(&registry, &[a_source(A_SOURCE_FILE, "fn foo() {}\n")]).await;

    // When a bind adds a source the server has never been told about
    let service = bind(
        &registry,
        &[
            a_source(A_SOURCE_FILE, "fn foo() {}\n"),
            a_source(ANOTHER_SOURCE_FILE, "fn main() {}\n"),
        ],
    )
    .await;

    // Then that one is opened, rather than changed on a sequence it has no place in
    assert_eq!(
        sync_log_for(&service.client, ANOTHER_SOURCE_FILE).await,
        json!([{ "method": "textDocument/didOpen", "version": 1 }])
    );
}
