//! A warm root's server is told what changed on disk since its previous request.
//!
//! A server this process keeps warm outlives every request, and between two of them the tree
//! changes behind it: an `apply` writes a module, a developer hand-writes one. rust-analyzer's own
//! file watcher did not see either on this repository's workspace (1,068 crates): a warm
//! `check --deep` anchored in a module the previous `apply` created never answered, and a type in a
//! hand-written file stayed `_` in an extracted signature. So the host tells the server itself, with
//! the protocol's `workspace/didChangeWatchedFiles`, before handing a warm server to a request.
//!
//! Against the deterministic `fake_lsp`, which replays what it was told as `tddy/watchedFileChanges`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tddy_index_daemon::index::WorkspaceIndex;
use tddy_lsp::client::LspClient;
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

/// A host whose warm servers are the deterministic fake.
fn an_index_over_fake_language_servers() -> WorkspaceIndex {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    WorkspaceIndex::new(LspRegistry::new(
        allow,
        TaskRegistry::new(),
        Duration::from_secs(60),
    ))
}

/// A cargo package on disk, at a canonical path, since that is what the server is told in.
fn a_crate_on_disk() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary workspace");
    let root = directory.path().canonicalize().expect("the root resolves");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(root.join("src/lib.rs"), "pub mod runtime;\n").expect("lib.rs");
    std::fs::write(root.join("src/runtime.rs"), "pub fn boot() {}\n").expect("runtime.rs");
    (directory, root)
}

/// What the server has been told changed on disk, in arrival order.
async fn changes_the_server_was_told(client: &LspClient) -> Vec<Value> {
    client
        .request_raw("tddy/watchedFileChanges", Value::Null)
        .await
        .expect("the fake server replays the changes it was told")
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn a_change(root: &Path, relative: &str, kind: u64) -> Value {
    json!({ "uri": format!("file://{}", root.join(relative).display()), "type": kind })
}

const CREATED: u64 = 1;
const CHANGED: u64 = 2;
const DELETED: u64 = 3;

#[tokio::test(flavor = "multi_thread")]
async fn tells_a_warm_server_of_a_module_written_since_its_last_request() {
    // Given a warm root, and a module written and declared behind its server
    let index = an_index_over_fake_language_servers();
    let (_workspace, root) = a_crate_on_disk();
    index.client_for(&root).await.expect("a warm server");
    std::fs::write(
        root.join("src/clock_face.rs"),
        "pub fn face() -> u32 { 7 }\n",
    )
    .expect("the new module");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod clock_face;\npub mod runtime;\n",
    )
    .expect("its declaration");

    // When the next request reaches the server
    let client: Arc<LspClient> = index.client_for(&root).await.expect("the same server");

    // Then the server has been told the module was created and its parent changed
    assert_eq!(
        changes_the_server_was_told(&client).await,
        vec![
            a_change(&root, "src/clock_face.rs", CREATED),
            a_change(&root, "src/lib.rs", CHANGED),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tells_a_warm_server_of_a_module_removed_since_its_last_request() {
    // Given a warm root whose module is then removed behind its server
    let index = an_index_over_fake_language_servers();
    let (_workspace, root) = a_crate_on_disk();
    index.client_for(&root).await.expect("a warm server");
    std::fs::remove_file(root.join("src/runtime.rs")).expect("the module is removed");

    // When the next request reaches the server
    let client = index.client_for(&root).await.expect("the same server");

    // Then the server has been told the module is gone
    assert_eq!(
        changes_the_server_was_told(&client).await,
        vec![a_change(&root, "src/runtime.rs", DELETED)]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tells_a_warm_server_nothing_when_its_tree_has_not_changed() {
    // Given a warm root whose tree nobody touches
    let index = an_index_over_fake_language_servers();
    let (_workspace, root) = a_crate_on_disk();
    index.client_for(&root).await.expect("a warm server");

    // When two more requests reach the server
    index.client_for(&root).await.expect("the same server");
    let client = index.client_for(&root).await.expect("the same server");

    // Then it has been told of no change
    assert_eq!(
        changes_the_server_was_told(&client).await,
        Vec::<Value>::new()
    );
}

/// Each change is told once: a request after the one that announced it has nothing new to say.
#[tokio::test(flavor = "multi_thread")]
async fn tells_a_warm_server_of_each_change_once() {
    // Given a warm root, a module written behind its server, and one request that announced it
    let index = an_index_over_fake_language_servers();
    let (_workspace, root) = a_crate_on_disk();
    index.client_for(&root).await.expect("a warm server");
    std::fs::write(
        root.join("src/clock_face.rs"),
        "pub fn face() -> u32 { 7 }\n",
    )
    .expect("the new module");
    index.client_for(&root).await.expect("the same server");

    // When another request reaches the server
    let client = index.client_for(&root).await.expect("the same server");

    // Then the module's creation was told exactly once
    assert_eq!(
        changes_the_server_was_told(&client).await,
        vec![a_change(&root, "src/clock_face.rs", CREATED)]
    );
}
