//! A wait for rust-analyzer ends in a named refusal when the server has gone silent.
//!
//! Two silences, two bounds (`SilenceBounds`), neither of them a deadline on total time:
//!
//! - **Ready but untypable.** The index is ready and the hover at the position stays `null`, with
//!   no diagnostic excusing it. Extractions refuse that after `READY_HOVER_BOUND` since #542; every
//!   other wait for type inference — a rename, the reference survey — waited for ever, which is the
//!   shape of the `#carve` 17 stage B2 apply that sat at 0% CPU for ten minutes.
//! - **Loading and silent.** The server says it is loading and then says nothing new. Past
//!   `LOADING_SILENCE_BOUND` the wait is refused as `ServerStalled`, naming the stage and the
//!   server's last words. A server that keeps narrating is never refused for how long it takes.
//!
//! Driven over `fake_lsp` — `--cold-hovers N --loads-crate-graph` (ready, never types),
//! `--goes-busy-after-hovers 1` (busy and silent after a ready warm-up) and
//! `--narrates-while-loading` (busy and talking) — with the bounds and the heartbeat injected, so
//! no test waits production's minutes and none branches on being a test.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_code_restructuring::backends::rust::{ProgressSink, RustBackend, SilenceBounds};
use tddy_code_restructuring::crate_move::{ItemReferences, ModuleReferences};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Position, RefactorKind, RefactorOp,
    Resolution, RestructureError,
};
use tddy_lsp::client::LspClient;
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// More hovers than any test will ask: a ready index that never types the position.
const NEVER_TYPES: &str = "1000000";

/// A ready bound short enough that a test sees it pass many times over.
const A_SHORT_READY_BOUND: Duration = Duration::from_millis(300);

/// A loading bound short enough that a test sees it pass many times over.
const A_SHORT_LOADING_BOUND: Duration = Duration::from_millis(500);

/// A loading bound above the two seconds between a wait's polls, so a server that narrates on
/// every poll is never silent for that long.
const A_LOADING_BOUND_ABOVE_THE_POLL: Duration = Duration::from_secs(3);

/// A bound no test reaches.
const NEVER_REACHED: Duration = Duration::from_secs(3600);

/// How long a test lets a run go before it stops it and reads what the run ended with.
const LONG_ENOUGH_FOR_A_SHORT_BOUND: Duration = Duration::from_secs(8);

/// How long a cancelled wait may take to unwind before the test calls it stuck.
const CANCELLATION_UNWIND: Duration = Duration::from_secs(5);

/// The lines a run sent to its progress sink, in order.
#[derive(Clone, Default)]
struct HeardLines(Arc<Mutex<Vec<String>>>);

impl HeardLines {
    fn sink(&self) -> ProgressSink {
        let lines = Arc::clone(&self.0);
        Arc::new(move |line: &str| lines.lock().expect("lines").push(line.to_string()))
    }

    fn all(&self) -> Vec<String> {
        self.0.lock().expect("lines").clone()
    }

    fn beats(&self) -> Vec<String> {
        self.0
            .lock()
            .expect("lines")
            .iter()
            .filter(|line| line.starts_with("still waiting"))
            .cloned()
            .collect()
    }
}

/// A source file on disk holding the one function the operations name.
fn a_workspace_holding_foo() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    1\n}\n",
    )
    .expect("a source file");
    workspace
}

/// A source file whose one function sits where `fake_lsp`'s outline puts it — lines 11–13 — so a
/// survey of the file has a declaration to ask about.
fn a_workspace_whose_function_sits_where_the_fake_outlines_it() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    let preamble = "// a line the outline does not name\n".repeat(10);
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        format!("{preamble}pub fn foo() -> u32 {{\n    1\n}}\n"),
    )
    .expect("a source file");
    workspace
}

async fn a_client_on_a_fake_started_with(root: &Path, args: &[&str]) -> Arc<LspClient> {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(client_capabilities())
        .with_initialization_options(server_settings());
    spec.args = args.iter().map(|arg| arg.to_string()).collect();
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
    Arc::clone(&service.client)
}

/// A backend over a fake started with `args`, ending silent waits at `bounds` and beating every
/// 300 ms into `heard`.
async fn a_backend_over_a_fake_started_with(
    root: &Path,
    args: &[&str],
    bounds: SilenceBounds,
    heard: &HeardLines,
    cancel: &CancellationToken,
) -> RustBackend {
    let client = a_client_on_a_fake_started_with(root, args).await;
    RustBackend::from_lsp_client(client, None, heard.sink())
        .with_cancellation(cancel.clone())
        .with_wait_heartbeat(Duration::from_millis(300))
        .with_silence_bounds(bounds)
}

fn bounds(ready: Duration, loading: Duration) -> SilenceBounds {
    SilenceBounds { ready, loading }
}

/// A rename of `foo`, anchored on its name at line 1.
fn a_rename_of_foo() -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::RenameSymbol,
        anchor: Anchor::Range {
            file: "src/lib.rs".to_string(),
            start: Position { line: 1, col: 8 },
            end: Position { line: 1, col: 11 },
        },
        name: Some("bar".to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

fn the_rename_of_foo(
    mut backend: RustBackend,
    root: &Path,
) -> tokio::task::JoinHandle<Result<Resolution, RestructureError>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };
        backend.resolve(&a_rename_of_foo(), &workspace)
    })
}

fn the_survey_of_lib(
    mut backend: RustBackend,
    root: &Path,
) -> tokio::task::JoinHandle<Result<Vec<ItemReferences>, RestructureError>> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };
        backend.outside_references(&workspace, "src/lib.rs")
    })
}

/// What the run ended with: its own refusal if it ended within `within`, or else what stopping
/// it produced — so a run that waits for ever fails the assertion instead of hanging the suite.
async fn what_it_ended_with<T: std::fmt::Debug>(
    cancel: &CancellationToken,
    mut operation: tokio::task::JoinHandle<Result<T, RestructureError>>,
    within: Duration,
) -> RestructureError {
    if let Ok(ended) = tokio::time::timeout(within, &mut operation).await {
        return ended
            .expect("the blocking half joins")
            .expect_err("a silent server is never an answer");
    }
    cancel.cancel();
    tokio::time::timeout(CANCELLATION_UNWIND, operation)
        .await
        .expect("a cancelled wait unwinds")
        .expect("the blocking half joins")
        .expect_err("a run that was stopped is a refusal")
}

/// Whether the run was still going after `elapsed`.
async fn still_going_after<T>(
    operation: &tokio::task::JoinHandle<Result<T, RestructureError>>,
    elapsed: Duration,
) -> bool {
    tokio::time::sleep(elapsed).await;
    !operation.is_finished()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_at_a_position_a_ready_index_never_types_is_refused_after_the_ready_bound_naming_the_position(
) {
    // Given a rename on a ready index that never types the position, with a 300 ms ready bound
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--cold-hovers", NEVER_TYPES, "--loads-crate-graph"],
        bounds(A_SHORT_READY_BOUND, NEVER_REACHED),
        &heard,
        &cancel,
    )
    .await;

    // When the rename is left to wait
    let refusal = what_it_ended_with(
        &cancel,
        the_rename_of_foo(backend, workspace.path()),
        LONG_ENOUGH_FOR_A_SHORT_BOUND,
    )
    .await;

    // Then it was refused as an unusable answer, naming the position, not stopped by its caller
    assert!(
        matches!(&refusal, RestructureError::ServerDefect(said)
            if said.contains("cannot type this position") && said.contains("src/lib.rs:1")),
        "{refusal:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_beat_for_a_ready_server_that_gives_no_answer_says_it_is_ready_and_gives_no_answer() {
    // Given a rename on a ready index that never types the position, under the default bounds
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--cold-hovers", NEVER_TYPES, "--loads-crate-graph"],
        SilenceBounds::default(),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When it has waited past the warm-up for a few beats
    let _stopped = what_it_ended_with(&cancel, operation, Duration::from_secs(3)).await;

    // Then every beat of the wait for type inference says the server is ready and gives no answer,
    // never that it has not said it is ready
    let inference_beats: Vec<String> = heard
        .beats()
        .into_iter()
        .filter(|beat| beat.contains("type inference at src/lib.rs:1"))
        .collect();
    assert!(!inference_beats.is_empty(), "{:#?}", heard.all());
    assert!(
        inference_beats
            .iter()
            .all(|beat| beat.contains("is ready and gives no answer here")
                && !beat.contains("has not said it is ready")),
        "{inference_beats:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_loading_server_silent_past_the_stall_bound_is_refused_as_stalled_naming_the_stage_and_its_last_words(
) {
    // Given a rename whose server goes busy on a build script after the warm-up and then says
    // nothing new, with a 500 ms loading bound
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--goes-busy-after-hovers", "1"],
        bounds(NEVER_REACHED, A_SHORT_LOADING_BOUND),
        &heard,
        &cancel,
    )
    .await;

    // When the rename is left to wait
    let refusal = what_it_ended_with(
        &cancel,
        the_rename_of_foo(backend, workspace.path()),
        LONG_ENOUGH_FOR_A_SHORT_BOUND,
    )
    .await;

    // Then it was refused as stalled, naming what it waited for and the server's last words
    assert!(
        matches!(&refusal, RestructureError::ServerStalled { stage, last, .. }
            if stage.contains("type inference at src/lib.rs:1")
                && last.contains("build script num-bigint run")),
        "{refusal:?}\n{:#?}",
        heard.all()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_loading_server_that_keeps_narrating_is_never_refused_as_stalled() {
    // Given a rename on a server that stays loading and reports a new build script on every poll,
    // with a loading bound above the poll interval
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--narrates-while-loading"],
        bounds(NEVER_REACHED, A_LOADING_BOUND_ABOVE_THE_POLL),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When it has waited for more than two loading bounds
    let going = still_going_after(&operation, Duration::from_secs(7)).await;

    // Then it is still waiting, and only its caller ends it
    assert!(going, "a server that kept talking was refused as stalled");
    let refusal = what_it_ended_with(&cancel, operation, Duration::ZERO).await;
    assert!(
        matches!(refusal, RestructureError::IndexingIncomplete { .. }),
        "{refusal:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reference_survey_at_an_untypable_position_is_refused_after_the_ready_bound() {
    // Given the reference survey of a file on a ready index that never types its positions
    let workspace = a_workspace_whose_function_sits_where_the_fake_outlines_it();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--cold-hovers", NEVER_TYPES, "--loads-crate-graph"],
        bounds(A_SHORT_READY_BOUND, NEVER_REACHED),
        &heard,
        &cancel,
    )
    .await;

    // When the survey is left to wait
    let refusal = what_it_ended_with(
        &cancel,
        the_survey_of_lib(backend, workspace.path()),
        LONG_ENOUGH_FOR_A_SHORT_BOUND,
    )
    .await;

    // Then it was refused as an unusable answer rather than stopped by its caller
    assert!(
        matches!(&refusal, RestructureError::ServerDefect(said)
            if said.contains("cannot type this position")),
        "{refusal:?}"
    );
}
