//! LSP client round-trips against the deterministic `fake_lsp` server, driven through the
//! registry (get-or-spawn → bind_target → client). The fake returns fixed values, so the
//! assertions are exact.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_lsp::{
    Diagnostic, DocumentSource, Language, LaunchSpec, Location, LspAllowList, LspError, LspKey,
    LspRegistry, LspService, Position, Range, SymbolInfo,
};
use tddy_task::TaskRegistry;

// Values fixed by the fake server (`tests/bin/fake_lsp.rs`).
const LIB_URI: &str = "file:///workspace/src/lib.rs";
const MAIN_URI: &str = "file:///workspace/src/main.rs";

fn registry() -> LspRegistry {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
}

fn workspace_key() -> LspKey {
    LspKey {
        root: PathBuf::from("/workspace"),
        language: Language::Rust,
    }
}

fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
    Range {
        start: Position::at(sl, sc),
        end: Position::at(el, ec),
    }
}

async fn bound_service(registry: &LspRegistry) -> Arc<LspService> {
    let srcs = vec![DocumentSource {
        uri: LIB_URI.to_string(),
        language_id: "rust".to_string(),
        text: "fn foo() -> u32 { 0 }\n".to_string(),
    }];
    registry
        .bind_target(workspace_key(), &srcs)
        .await
        .expect("bind target")
}

#[tokio::test]
async fn completes_the_initialize_handshake() {
    // Given a registry over the fake server
    let registry = registry();

    // When a server is spawned for a workspace
    let service = registry
        .get_or_spawn(workspace_key())
        .await
        .expect("initialized server");

    // Then a running server task backs it
    assert!(!service.task_id.to_string().is_empty());
}

#[tokio::test]
async fn returns_the_definition_location() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we ask for the definition at a position
    let locations = service
        .client
        .definition(LIB_URI, Position::at(10, 0))
        .await
        .expect("definition");

    // Then the server's single definition location is returned
    assert_eq!(
        locations,
        vec![Location {
            uri: LIB_URI.to_string(),
            range: range(10, 0, 10, 3),
        }]
    );
}

#[tokio::test]
async fn returns_all_reference_locations() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we ask for references at a position
    let locations = service
        .client
        .references(LIB_URI, Position::at(10, 0))
        .await
        .expect("references");

    // Then every reference across the workspace is returned
    assert_eq!(
        locations,
        vec![
            Location {
                uri: LIB_URI.to_string(),
                range: range(10, 0, 10, 3),
            },
            Location {
                uri: MAIN_URI.to_string(),
                range: range(20, 4, 20, 7),
            },
        ]
    );
}

#[tokio::test]
async fn returns_hover_markdown() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we hover at a position
    let hover = service
        .client
        .hover(LIB_URI, Position::at(10, 0))
        .await
        .expect("hover");

    // Then the server's hover markdown is returned
    assert_eq!(hover, Some("fn foo() -> u32".to_string()));
}

#[tokio::test]
async fn returns_document_symbols() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we ask for the document's symbols
    let symbols = service.client.symbols(LIB_URI).await.expect("symbols");

    // Then the server's single symbol is returned, located in the queried document
    assert_eq!(
        symbols,
        vec![SymbolInfo {
            name: "foo".to_string(),
            kind: 12,
            location: Location {
                uri: LIB_URI.to_string(),
                range: range(10, 0, 12, 1),
            },
            container: None,
        }]
    );
}

#[tokio::test]
async fn surfaces_a_published_diagnostic_after_opening_a_document() {
    // Given a Rust workspace whose sources have been opened (bind_target → didOpen)
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we read diagnostics for the opened document
    let diagnostics = service
        .client
        .diagnostics(LIB_URI)
        .await
        .expect("diagnostics");

    // Then the diagnostic the server published is surfaced
    assert_eq!(
        diagnostics,
        vec![Diagnostic {
            range: range(5, 4, 5, 9),
            severity: 1,
            message: "unused variable: `x`".to_string(),
            source: Some("rustc".to_string()),
        }]
    );
}

#[tokio::test]
async fn returns_workspace_diagnostics_grouped_by_document() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When we pull workspace-wide diagnostics
    let groups = service
        .client
        .workspace_diagnostics()
        .await
        .expect("workspace diagnostics");

    // Then the server's report is returned, grouped by document uri
    assert_eq!(
        groups,
        vec![(
            LIB_URI.to_string(),
            vec![Diagnostic {
                range: range(5, 4, 5, 9),
                severity: 1,
                message: "unused variable: `x`".to_string(),
                source: Some("rustc".to_string()),
            }],
        )]
    );
}

#[tokio::test]
async fn correlates_concurrent_requests_by_id() {
    // Given a bound Rust workspace
    let registry = registry();
    let service = bound_service(&registry).await;

    // When definition and references are requested concurrently
    let (definition, references) = tokio::join!(
        service.client.definition(LIB_URI, Position::at(10, 0)),
        service.client.references(LIB_URI, Position::at(10, 0)),
    );

    // Then each request receives its own correct response (no id cross-talk)
    assert_eq!(definition.expect("definition").len(), 1);
    assert_eq!(references.expect("references").len(), 2);
}

/// The fake answers `textDocument/codeAction` with a JSON-RPC `ContentModified` error, which is
/// what rust-analyzer sends when a request lands against a document it has since seen change.
/// It has to reach the caller as an error: a caller that receives an empty success instead
/// concludes the server had nothing to offer and stops retrying.
#[tokio::test]
async fn surfaces_a_server_error_response_as_an_error() {
    // Given a bound service over the fake server
    let registry = registry();
    let service = bound_service(&registry).await;

    // When a request the fake answers with a JSON-RPC error is issued
    let outcome = service
        .client
        .request_raw(
            "textDocument/codeAction",
            serde_json::json!({ "textDocument": { "uri": LIB_URI } }),
        )
        .await;

    // Then the caller is handed the server's error, code and message intact
    match outcome {
        Err(LspError::Server { code, message }) => {
            assert_eq!((code, message.as_str()), (-32801, "content modified"));
        }
        other => panic!("expected a server error, got {other:?}"),
    }
}

/// `--indexing-budget` is documented as the remedy for a slow machine, so the per-request wait
/// has to be governed by it rather than by a constant the flag cannot reach.
#[tokio::test]
async fn honours_a_request_timeout_raised_after_the_client_was_built() {
    // Given a bound service whose per-request wait has been shortened
    let registry = registry();
    let service = bound_service(&registry).await;
    service
        .client
        .set_request_timeout(Duration::from_millis(150));

    // When a request the fake never answers is issued
    let started = std::time::Instant::now();
    let outcome = service
        .client
        .request_raw("tddy/neverAnswers", serde_json::json!({}))
        .await;

    // Then it gives up at the configured wait rather than at the built-in default
    assert!(matches!(outcome, Err(LspError::Timeout)), "got {outcome:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}, so the configured timeout was ignored",
        started.elapsed()
    );
}

/// rust-analyzer decides what to answer a `textDocument/codeAction` with from what the client
/// advertised: with no `codeAction` capability it returns an empty list for every range, which
/// reads exactly like a range that supports no refactoring. So what a caller advertises has to
/// reach the server, or the capability it built is dead code.
#[tokio::test]
async fn advertises_the_capabilities_the_launch_spec_carries() {
    // Given an allow-list whose Rust server advertises code-action support
    let capabilities = serde_json::json!({
        "textDocument": {
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["refactor.extract"] }
                }
            }
        }
    });
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")).with_capabilities(capabilities.clone()),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));

    // When a server is spawned and asked what it was initialized with
    let service = registry
        .get_or_spawn(workspace_key())
        .await
        .expect("spawn server");
    let params = service
        .client
        .request_raw("tddy/initializeParams", serde_json::json!({}))
        .await
        .expect("initialize params replayed");

    // Then the server received them, rather than an empty capability set
    assert_eq!(
        params.get("capabilities"),
        Some(&capabilities),
        "the server was initialized with {:?}",
        params.get("capabilities")
    );
}
