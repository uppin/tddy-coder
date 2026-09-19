//! What ends a wait, now that no budget does.
//!
//! The backend waits for a server that is still loading rather than refusing it at a number this
//! library guessed, and stops when its caller stops — which it learns from a cancellation token
//! checked *inside* the poll loops. That the check is inside matters: the backend is synchronous
//! and runs under `spawn_blocking`, where dropping the calling future stops nothing. These suites
//! drive `fake_lsp` in its `--cold-hovers` mode, which answers hover with `null` the way a server
//! that has not finished loading the crate graph does, so a slow index costs no real one.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_code_restructuring::backends::rust::{discard, RustBackend};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Position, RefactorKind, RefactorOp,
    RestructureError,
};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// More cold hovers than any test consumes, so the index never finishes on its own and the only
/// thing that can end a wait is the caller.
const NEVER_FINISHES_INDEXING: &str = "100000";

/// How long a cancelled wait may take to unwind before the test calls it stuck. Generous next to
/// the 200ms poll the loops use, tight next to the ten-minute wait this replaces.
const CANCELLATION_UNWIND: Duration = Duration::from_secs(5);

/// A backend over a fake server that never finishes indexing, launched with the same handshake
/// `tddy-tools restructure` sends — a handshake that drifted from production's would be testing a
/// different client.
async fn a_backend_over_a_server_that_never_finishes_indexing(
    root: &Path,
    cancel: CancellationToken,
) -> RustBackend {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(client_capabilities())
        .with_initialization_options(server_settings());
    spec.args = vec![
        "--cold-hovers".to_string(),
        NEVER_FINISHES_INDEXING.to_string(),
    ];
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        })
        .await
        .expect("the fake language server starts");

    RustBackend::from_lsp_client(Arc::clone(&service.client), None, discard())
        .with_cancellation(cancel)
}

/// A source file on disk for the backend to anchor against.
fn a_workspace_holding_one_source_file() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    1\n}\n",
    )
    .expect("a source file");
    workspace
}

/// Ask the backend for something that has to wait for the index, on a blocking thread — the way
/// the runner drives it.
fn an_operation_that_waits_for_the_index(
    mut backend: RustBackend,
    root: PathBuf,
) -> tokio::task::JoinHandle<Result<tddy_code_restructuring::Range, RestructureError>> {
    tokio::task::spawn_blocking(move || {
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };
        backend.anchor_for("src/lib.rs", &["foo".to_string()], &workspace)
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_for_a_loading_index_is_not_refused_while_the_caller_is_still_waiting() {
    // Given a backend over a server that never finishes indexing, and a caller still waiting
    let workspace = a_workspace_holding_one_source_file();
    let cancel = CancellationToken::new();
    let backend =
        a_backend_over_a_server_that_never_finishes_indexing(workspace.path(), cancel.clone())
            .await;

    // When the operation is left to wait for longer than the budget a raised run used to imply
    let operation = an_operation_that_waits_for_the_index(backend, workspace.path().to_path_buf());
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Then it is still waiting rather than having refused a server that was merely slow
    assert!(
        !operation.is_finished(),
        "the operation gave up on a loading index while its caller was still waiting"
    );
    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_wait_stops_and_reports_how_far_the_index_got() {
    // Given an operation waiting on a server that never finishes indexing
    let workspace = a_workspace_holding_one_source_file();
    let cancel = CancellationToken::new();
    let backend =
        a_backend_over_a_server_that_never_finishes_indexing(workspace.path(), cancel.clone())
            .await;
    let operation = an_operation_that_waits_for_the_index(backend, workspace.path().to_path_buf());
    tokio::time::sleep(Duration::from_millis(500)).await;

    // When its caller stops waiting
    cancel.cancel();
    let outcome = tokio::time::timeout(CANCELLATION_UNWIND, operation)
        .await
        .expect("a cancelled wait unwinds rather than running to its own end")
        .expect("the blocking half joins");

    // Then it reports an incomplete index, naming where the server got to
    let RestructureError::IndexingIncomplete { last, .. } = outcome.expect_err("a refusal") else {
        panic!("expected IndexingIncomplete, got a different refusal");
    };
    assert!(
        !last.is_empty(),
        "a cancelled wait must say how far the index got, not just that it stopped"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_stops_the_blocking_wait_rather_than_only_dropping_its_future() {
    // Given an operation waiting on a server that never finishes indexing
    let workspace = a_workspace_holding_one_source_file();
    let cancel = CancellationToken::new();
    let backend =
        a_backend_over_a_server_that_never_finishes_indexing(workspace.path(), cancel.clone())
            .await;
    let operation = an_operation_that_waits_for_the_index(backend, workspace.path().to_path_buf());
    tokio::time::sleep(Duration::from_millis(500)).await;

    // When its caller stops waiting
    let cancelled_at = Instant::now();
    cancel.cancel();
    tokio::time::timeout(CANCELLATION_UNWIND, operation)
        .await
        .expect("the blocking thread returns")
        .expect("the blocking half joins")
        .expect_err("a refusal");

    // Then the blocking thread itself returned, rather than the work continuing behind a dropped
    // future
    assert!(
        cancelled_at.elapsed() < CANCELLATION_UNWIND,
        "the blocking wait ran on after cancellation for {:?}",
        cancelled_at.elapsed()
    );
}

/// An extraction of the one function the fixture file holds. The range covers its body's second
/// line only: rust-analyzer declines to wrap a whole body in a function that adds nothing, so a
/// proper subset is what an extraction anchor has to be.
fn an_extraction_of_the_function_body() -> RefactorOp {
    RefactorOp {
        op: RefactorKind::ExtractMethod,
        anchor: Anchor::Range {
            file: "src/lib.rs".to_string(),
            start: Position { line: 2, col: 5 },
            end: Position { line: 2, col: 6 },
        },
        name: Some("extracted".to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_will_not_settle_is_refused_as_unsettled_not_as_a_malformed_plan() {
    // Given a backend over a server that answers every code action with `ContentModified`, the way
    // a server perpetually behind its client's edits does
    let workspace = a_workspace_holding_one_source_file();
    let cancel = CancellationToken::new();
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(client_capabilities())
            .with_initialization_options(server_settings()),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: workspace.path().to_path_buf(),
            language: Language::Rust,
        })
        .await
        .expect("the fake language server starts");
    let mut backend = RustBackend::from_lsp_client(Arc::clone(&service.client), None, discard())
        .with_cancellation(cancel);

    // When an operation asks it to resolve an extraction
    let root = workspace.path().to_path_buf();
    let outcome = tokio::task::spawn_blocking(move || {
        let overlay = Overlay::new();
        let space = Workspace {
            root: &root,
            overlay: &overlay,
        };
        backend.resolve(&an_extraction_of_the_function_body(), &space)
    })
    .await
    .expect("the blocking half joins");

    // Then the refusal names the server rather than the plan
    let error = outcome.expect_err("a refusal");
    assert!(
        matches!(error, RestructureError::ServerNotSettled { .. }),
        "a server that would not settle was reported as {error:?}"
    );
}
