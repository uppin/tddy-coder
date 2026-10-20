//! Languages are supports behind one interface, and Rust is the built-in one.
//!
//! Every test here is library level: the launch the registry builds is compared with the one the CLI
//! and the index daemon used to build by hand, and the one test that needs a server talks to the
//! deterministic fake, never to rust-analyzer.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tddy_code_restructuring::backends::rust::{discard, RustSupport};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    client_capabilities, server_settings, LiveServer, Overlay, RefactorKind, RestructureError,
    ServerHandle, SupportRegistry,
};
use tddy_lsp::client::LspClient;
use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// The rust-analyzer launch `restructure_cli.rs` built by hand before the registry existed.
fn the_launch_the_cli_used_to_build() -> LaunchSpec {
    LaunchSpec::new("rust-analyzer")
        .with_capabilities(client_capabilities())
        .with_initialization_options(server_settings())
        .with_language_id("rust")
}

/// The Rust launch `registry`'s allow-list carries.
fn the_rust_launch_of(registry: &SupportRegistry) -> Option<LaunchSpec> {
    registry
        .allow_list()
        .launch_spec(&LanguageId::RUST)
        .cloned()
}

#[test]
fn the_rust_launch_the_registry_builds_equals_the_one_the_cli_used_to_build() {
    // Given the built-in supports
    let registry = SupportRegistry::with_builtins();

    // When their allow-list is built
    let launch = the_rust_launch_of(&registry);

    // Then Rust launches exactly as the CLI's own allow-list launched it
    assert_eq!(launch, Some(the_launch_the_cli_used_to_build()));
}

#[test]
fn the_daemon_and_the_cli_build_the_same_allow_list() {
    // Given the built-ins as the CLI builds them, and as the daemon does with its configured server
    let configured = "/opt/rust-analyzer/bin/rust-analyzer";
    let cli = SupportRegistry::with_builtins();
    let daemon = SupportRegistry::with_builtin_rust(RustSupport::launching(configured));

    // When each builds its allow-list
    let from_the_cli = the_rust_launch_of(&cli);
    let from_the_daemon = the_rust_launch_of(&daemon);

    // Then the two differ in the program alone
    let mut the_cli_at_the_configured_program = from_the_cli.expect("the CLI allows Rust");
    the_cli_at_the_configured_program.program = configured.to_string();
    assert_eq!(from_the_daemon, Some(the_cli_at_the_configured_program));
}

/// A workspace holding one Rust source file.
fn a_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), source).expect("a source file");
    workspace
}

/// The deterministic fake server, started the way the built-in Rust launch starts rust-analyzer.
async fn the_fake_server_for(root: &Path) -> Arc<LspClient> {
    let mut allow = LspAllowList::new();
    allow.allow(
        LanguageId::RUST,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(client_capabilities())
            .with_initialization_options(server_settings()),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: LanguageId::RUST,
        })
        .await
        .expect("the fake language server starts");
    Arc::clone(&service.client)
}

/// The `languageId` the server was told each document is, by URI.
async fn the_languages_the_server_was_told(client: &LspClient) -> Value {
    client
        .request_raw("tddy/documentLanguages", Value::Null)
        .await
        .expect("the fake server replays the languages it was told")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_document_opens_with_its_supports_language_id() {
    // Given the built-in backends over a server, and a Rust file
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let root = workspace.path().to_path_buf();
    let client = the_fake_server_for(&root).await;
    let for_the_backend = Arc::clone(&client);

    // When the file's backend opens it — asking for an anchor is enough to open the document
    tokio::task::spawn_blocking(move || {
        let cancel = CancellationToken::new();
        let progress = discard();
        let supports = SupportRegistry::with_builtins();
        let mut backends = supports.backends(
            ServerHandle::live(LiveServer {
                client: &for_the_backend,
                cancel: &cancel,
                progress: &progress,
                trace: None,
                wait_heartbeat: Duration::from_secs(60),
            }),
            &root,
        );
        let overlay = Overlay::new();
        let _ = backends
            .backend_of(Path::new("src/lib.rs"))
            .expect("a backend claims the Rust file")
            .anchor_for(
                "src/lib.rs",
                &["foo".to_string()],
                &Workspace {
                    root: &root,
                    overlay: &overlay,
                },
            );
    })
    .await
    .expect("the blocking half of the open joins");

    // Then the server was told the document is the Rust support's languageId
    let uri = format!("file://{}", workspace.path().join("src/lib.rs").display());
    let languages = the_languages_the_server_was_told(&client).await;
    let expected = SupportRegistry::with_builtins()
        .support_of(Path::new("src/lib.rs"))
        .and_then(|support| support.launch())
        .map(|launch| launch.language_id)
        .expect("the Rust support launches a server");
    assert_eq!(
        languages.get(&uri).and_then(Value::as_str),
        Some(expected.as_str()),
        "the languages the server was told: {languages}"
    );
}

#[test]
fn an_unsupported_operation_names_the_language_by_its_id() {
    // Given the built-in backends with no server, for a workspace
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let mut backends = SupportRegistry::with_builtins().backends_static(workspace.path());

    // When a Rust file is asked for an operation Rust does not perform
    let refused = backends
        .backend_for(Path::new("src/lib.rs"), RefactorKind::MoveSymbol)
        .err();

    // Then the refusal names the language by its id
    match refused {
        Some(RestructureError::UnsupportedOp { backend, op }) => {
            assert_eq!((backend.as_str(), op.as_str()), ("rust", "MoveSymbol"));
        }
        other => panic!("expected UnsupportedOp, got {other:?}"),
    }
}
