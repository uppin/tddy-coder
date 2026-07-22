//! LSP client round-trips against the deterministic `fake_lsp` server, driven through the
//! registry (get-or-spawn → bind_target → client). The fake returns fixed values, so the
//! assertions are exact.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tddy_lsp::{
    Diagnostic, DocumentSource, Language, LaunchSpec, Location, LspAllowList, LspKey, LspRegistry,
    LspService, Position, Range, SymbolInfo,
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
