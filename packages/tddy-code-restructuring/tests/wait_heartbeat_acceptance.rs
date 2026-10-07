//! A run that waits for rust-analyzer says, on a fixed heartbeat, what it is waiting for.
//!
//! The wait for an index has no budget: a run waits until the server is ready or until its caller
//! stops it. That left a server that stops talking — a build script that never finishes — with a
//! run that printed one line and then nothing for as long as it lasted. These suites drive
//! `fake_lsp` in its busy modes (`--never-quiescent`, `--goes-busy-after-hovers`,
//! `--hover-never-answers`) with a heartbeat shortened to a few hundred milliseconds, and read the
//! lines the run sends to its progress sink. The cadence is an injected collaborator, as it is in
//! production, so no test branches on being a test.
//!
//! **None of this adds a deadline.** `a_wait_has_no_deadline_however_many_beats_pass` is the guard.

mod harness;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use harness::{a_workspace_whose_origin_build_script_never_finishes, THE_TEST_BINARY};
use tddy_code_restructuring::backends::rust::{ProgressSink, RustBackend};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Position, RefactorKind, RefactorOp,
    Resolution, RestructureError,
};
use tddy_lsp::client::LspClient;
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// The prefix a reader greps for, and so a contract.
const THE_PREFIX_OF_A_BEAT: &str = "still waiting";

/// How long a test lets a run go on waiting for the beats it needs before it stops the run anyway,
/// so a run that never beats fails by assertion instead of hanging the suite.
const LONG_ENOUGH_FOR_THE_BEATS_THE_TEST_NEEDS: Duration = Duration::from_secs(8);

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

    /// The lines that are a heartbeat, which begin with [`THE_PREFIX_OF_A_BEAT`].
    fn beats(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .filter(|line| line.starts_with(THE_PREFIX_OF_A_BEAT))
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

/// A shared client on a fake started with `args`, launched with the handshake production sends.
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

/// A backend over a fake started with `args`, beating every `heartbeat` into `heard`.
async fn a_backend_over_a_fake_started_with(
    root: &Path,
    args: &[&str],
    heartbeat: Duration,
    heard: &HeardLines,
    cancel: &CancellationToken,
) -> RustBackend {
    let client = a_client_on_a_fake_started_with(root, args).await;
    RustBackend::from_lsp_client(client, None, heard.sink())
        .with_cancellation(cancel.clone())
        .with_wait_heartbeat(heartbeat)
}

/// A rename of `foo`, anchored on where the function is declared: line 1, the name's three columns.
fn a_rename_of_foo_at_its_declaration() -> RefactorOp {
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
    }
}

/// The rename, on a blocking thread — the way the runner drives a backend.
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
        backend.resolve(&a_rename_of_foo_at_its_declaration(), &workspace)
    })
}

/// Let the run go on until `heard` holds `count` beats, or for as long as a test will wait for them.
async fn until_the_run_has_beaten(heard: &HeardLines, count: usize) {
    let deadline = Instant::now() + LONG_ENOUGH_FOR_THE_BEATS_THE_TEST_NEEDS;
    while heard.beats().len() < count && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Stop the run and take what it ended with, unwinding within [`CANCELLATION_UNWIND`].
async fn stopping(
    cancel: &CancellationToken,
    operation: tokio::task::JoinHandle<Result<Resolution, RestructureError>>,
) -> RestructureError {
    cancel.cancel();
    tokio::time::timeout(CANCELLATION_UNWIND, operation)
        .await
        .expect("a cancelled wait unwinds rather than running to its own end")
        .expect("the blocking half joins")
        .expect_err("a run that was stopped is a refusal")
}

/// The whole seconds a beat says the wait has lasted, from `still waiting (<n>s)`.
fn the_elapsed_seconds_in(beat: &str) -> u64 {
    let after = beat
        .strip_prefix("still waiting (")
        .expect("a beat opens with its elapsed time");
    after
        .split('s')
        .next()
        .expect("a figure")
        .parse()
        .expect("whole seconds below a minute")
}

/// The whole seconds a beat says the server's words have been unchanged, from `unchanged for <n>s`.
fn the_unchanged_seconds_in(beat: &str) -> u64 {
    beat.split("unchanged for ")
        .nth(1)
        .expect("a beat says how long the server has said nothing new")
        .split('s')
        .next()
        .expect("a figure")
        .parse()
        .expect("whole seconds below a minute")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_on_a_server_that_stays_busy_says_what_it_is_waiting_for_on_every_beat() {
    // Given a rename waiting on a server that never stops being busy, beating every 300 ms
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--never-quiescent"],
        Duration::from_millis(300),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When it has waited long enough for several beats
    until_the_run_has_beaten(&heard, 4).await;
    stopping(&cancel, operation).await;

    // Then at least four lines are beats
    let beats = heard.beats();
    assert!(
        beats.len() >= 4,
        "{} beats in {:?}",
        beats.len(),
        heard.all()
    );
    // And each names the stage, says the server is still loading and quotes what it last said
    for beat in &beats {
        assert!(beat.contains("warming the crate index"), "{beat}");
        assert!(beat.contains("still loading"), "{beat}");
        assert!(beat.contains("build script num-bigint run"), "{beat}");
    }
    // And the time the wait has lasted never goes backwards
    let elapsed: Vec<u64> = beats
        .iter()
        .map(|beat| the_elapsed_seconds_in(beat))
        .collect();
    let mut in_order = elapsed.clone();
    in_order.sort_unstable();
    assert_eq!(elapsed, in_order);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_names_the_type_inference_stage_once_the_index_was_ready() {
    // Given a server that loads its graph, answers one hover, and is then busy again — a tree
    // that changed after a ready index
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--goes-busy-after-hovers", "1"],
        Duration::from_millis(300),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When the rename waits for it
    until_the_run_has_beaten(&heard, 2).await;
    stopping(&cancel, operation).await;

    // Then the beats name type inference at the anchor, and not the warm-up that was already over
    let beats = heard.beats();
    assert!(!beats.is_empty(), "no beat in {:?}", heard.all());
    for beat in &beats {
        assert!(beat.contains("type inference at src/lib.rs:1"), "{beat}");
        assert!(!beat.contains("warming the crate index"), "{beat}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_beat_says_how_long_the_server_has_said_nothing_new() {
    // Given a rename waiting on a busy server, beating once a second
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--never-quiescent"],
        Duration::from_secs(1),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When it has waited for four beats
    until_the_run_has_beaten(&heard, 4).await;
    stopping(&cancel, operation).await;

    // Then the figure for how long the server has been quiet grows by about a second per beat
    let beats = heard.beats();
    assert!(
        beats.len() >= 4,
        "{} beats in {:?}",
        beats.len(),
        heard.all()
    );
    let unchanged: Vec<u64> = beats
        .iter()
        .map(|beat| the_unchanged_seconds_in(beat))
        .collect();
    let growth: Vec<u64> = unchanged.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(
        growth.iter().all(|step| (1..=2).contains(step)),
        "the quiet grew by {growth:?} between beats {unchanged:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_wait_names_its_stage_and_where_the_server_got_to() {
    // Given a rename waiting on a busy server that has beaten twice
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--never-quiescent"],
        Duration::from_millis(300),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());
    until_the_run_has_beaten(&heard, 2).await;

    // When its caller stops it
    let refusal = stopping(&cancel, operation).await;

    // Then the refusal names the stage and quotes where the server got to
    let message = refusal.to_string();
    let RestructureError::IndexingIncomplete { stage, last, .. } = refusal else {
        panic!("expected IndexingIncomplete, got {refusal:?}");
    };
    assert_eq!(stage, "warming the crate index");
    assert!(last.contains("build script num-bigint run"), "{last}");
    assert!(
        message.contains("while warming the crate index"),
        "{message}"
    );
}

/// A guard, green before and after: the developer decided a heartbeat and **no deadline**, and the
/// cheapest way to break that is a "give up after N beats".
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_has_no_deadline_however_many_beats_pass() {
    // Given a rename waiting on a server that never becomes ready, beating every 100 ms
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--never-quiescent"],
        Duration::from_millis(100),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When it is left to wait for far longer than a beat
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // Then it is still waiting, and only its caller ends it
    assert!(
        !operation.is_finished(),
        "the wait gave up on a busy server while its caller was still waiting"
    );
    let refusal = stopping(&cancel, operation).await;
    assert!(
        matches!(refusal, RestructureError::IndexingIncomplete { .. }),
        "{refusal:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_in_flight_to_a_server_that_never_answers_is_narrated_while_it_waits() {
    // Given a rename whose hover the server receives and never answers, beating every 300 ms
    let workspace = a_workspace_holding_foo();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let backend = a_backend_over_a_fake_started_with(
        workspace.path(),
        &["--hover-never-answers"],
        Duration::from_millis(300),
        &heard,
        &cancel,
    )
    .await;
    let operation = the_rename_of_foo(backend, workspace.path());

    // When the request has been in flight for several beats
    until_the_run_has_beaten(&heard, 2).await;
    let cancelled_at = Instant::now();
    stopping(&cancel, operation).await;

    // Then the beats came while the request was unanswered, naming it
    let beats = heard.beats();
    assert!(
        beats.len() >= 2,
        "{} beats in {:?}",
        beats.len(),
        heard.all()
    );
    for beat in &beats {
        assert!(
            beat.contains("a textDocument/hover request in flight"),
            "{beat}"
        );
    }
    // And cancelling still ended the request within the existing unwind
    assert!(cancelled_at.elapsed() < CANCELLATION_UNWIND);
}

/// A move of the test binary through `runner::apply` over a server no operation asks anything of,
/// with the run's heartbeat set and every line its progress sink heard kept.
async fn applying_the_move_of_the_test_binary_beating_every(
    root: &Path,
    heartbeat: Duration,
    heard: &HeardLines,
    cancel: CancellationToken,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let digest = tddy_code_restructuring::apply::hash_file(&root.join(THE_TEST_BINARY))
        .expect("the test binary hashes");
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        format!(
            "{{\"v\":1,\"snapshot\":{{\"{THE_TEST_BINARY}\":\"{digest}\"}}}}\n\
             {{\"op\":\"move_test_binary_to_crate\",\"anchor\":{{\"kind\":\"symbol\",\
             \"file\":\"{THE_TEST_BINARY}\",\"path\":\"golden\"}},\"to\":\"crates/destination\"}}\n"
        ),
    )
    .expect("the plan is written");
    let client = a_client_on_a_fake_started_with(root, &[]).await;
    let options = tddy_code_restructuring::runner::Options {
        command: tddy_code_restructuring::runner::Command::Apply,
        target: Some(plan),
        progress: heard.sink(),
        wait_heartbeat: heartbeat,
        ..tddy_code_restructuring::runner::Options::default()
    };
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        tddy_code_restructuring::runner::apply(&root, options, Some(client), cancel)
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the apply joins")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cargo_check_that_does_not_finish_is_narrated_with_its_pid() {
    // Given an apply whose baseline `cargo check` blocks on a build script that never finishes,
    // beating every 200 ms
    let workspace = a_workspace_whose_origin_build_script_never_finishes();
    let (heard, cancel) = (HeardLines::default(), CancellationToken::new());
    let stopper = {
        let (heard, cancel) = (heard.clone(), cancel.clone());
        tokio::spawn(async move {
            until_the_run_has_beaten(&heard, 2).await;
            cancel.cancel();
        })
    };

    // When it has beaten twice and its caller stops it
    let outcome = applying_the_move_of_the_test_binary_beating_every(
        workspace.path(),
        Duration::from_millis(200),
        &heard,
        cancel,
    )
    .await;
    stopper.await.expect("the stopper finishes");

    // Then the beats name the check and its pid
    let beats = heard.beats();
    assert!(
        beats.len() >= 2,
        "{} beats in {:?}",
        beats.len(),
        heard.all()
    );
    for beat in &beats {
        assert!(
            beat.contains("cargo check --all-targets -p origin"),
            "{beat}"
        );
        assert!(beat.contains("pid"), "{beat}");
    }
    // And the run ended because its caller stopped it
    assert_eq!(
        outcome.expect_err("a stopped run is not a success"),
        RestructureError::CallerStopped.to_string()
    );
}
