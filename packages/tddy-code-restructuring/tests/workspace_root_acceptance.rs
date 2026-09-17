//! Which tree an operation acts on, and where its progress goes.
//!
//! Both are the caller's to state. A library that reads the process directory can serve one tree per
//! process, and a library whose progress is a bare `fn` pointer has nowhere to put the channel a
//! per-caller stream would need. These suites pin the parameters instead: a workspace root on the
//! entry points, and a sink the caller owns.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_code_restructuring::apply::hash_file;
use tddy_code_restructuring::backends::rust::{ProgressSink, RustBackend};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::{client_capabilities, server_settings, Overlay};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// More cold hovers than any test consumes, so a wait ends only when its caller says so.
const NEVER_FINISHES_INDEXING: &str = "100000";

/// A git worktree holding one source file, at a path that is not the process directory.
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

/// A plan whose snapshot names `src/lib.rs` under `root`, and which asks for nothing.
///
/// No operations on purpose: the snapshot header is the only part of a plan whose meaning depends on
/// which tree it is read against, so a plan with nothing else in it isolates exactly that.
fn a_plan_snapshotting_the_source_under(root: &Path) -> std::path::PathBuf {
    let digest = hash_file(&root.join("src/lib.rs")).expect("hash the source file");
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        format!("{{\"v\":1,\"snapshot\":{{\"src/lib.rs\":\"{digest}\"}}}}\n"),
    )
    .expect("write the plan");
    plan
}

fn a_check_of(plan: &Path) -> Options {
    Options {
        command: Command::Check,
        target: Some(plan.to_path_buf()),
        ..Options::default()
    }
}

#[test]
fn a_check_resolves_its_snapshot_against_the_root_it_was_given() {
    // Given a workspace that is not the process directory, and a plan snapshotting its source
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_snapshotting_the_source_under(workspace.path());

    // When the plan is checked against that workspace
    let outcome = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    );

    // Then the snapshot verifies, rather than being resolved against the process directory
    outcome.expect("the snapshot resolves against the root the caller named");
}

#[test]
fn two_roots_are_checked_against_their_own_sources() {
    // Given two workspaces whose source files differ, each with a plan snapshotting its own
    let one = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let other = a_workspace_holding("pub fn bar() -> u32 {\n    2\n}\n");
    let one_plan = a_plan_snapshotting_the_source_under(one.path());
    let other_plan = a_plan_snapshotting_the_source_under(other.path());

    // When each plan is checked against the other's workspace
    let crossed = runner::check(
        other.path(),
        a_check_of(&one_plan),
        None,
        CancellationToken::new(),
    );
    let matched = runner::check(
        other.path(),
        a_check_of(&other_plan),
        None,
        CancellationToken::new(),
    );

    // Then the mismatch is caught and the match is accepted — so the root, not the process
    // directory, is what a snapshot is measured against
    crossed.expect_err("a plan snapshotting another tree's source must not verify here");
    matched.expect("a plan snapshotting this tree's source verifies");
}

#[test]
fn a_status_reads_the_journal_under_the_root_it_was_given() {
    // Given a workspace whose journal on disk cannot be parsed
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_snapshotting_the_source_under(workspace.path());
    std::fs::create_dir_all(workspace.path().join(".restructure")).expect(".restructure");
    std::fs::write(
        workspace.path().join(".restructure/journal.jsonl"),
        "this is not a journal record\n",
    )
    .expect("write a corrupt journal");

    // When its status is asked for
    let outcome = runner::status(
        workspace.path(),
        Options {
            command: Command::Status,
            target: Some(plan),
            ..Options::default()
        },
    );

    // Then that journal is what was read — a status resolved against the process directory would
    // have found no journal at all and reported success
    outcome.expect_err("the corrupt journal under the named root must be the one read");
}

/// A sink that keeps every line it is handed, so a test can ask what a caller was told.
fn a_recording_sink() -> (ProgressSink, Arc<Mutex<Vec<String>>>) {
    let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = Arc::clone(&recorded);
    let sink: ProgressSink = Arc::new(move |line: &str| {
        sink_lines.lock().unwrap().push(line.to_string());
    });
    (sink, recorded)
}

/// A backend over a fake server that never finishes indexing, reporting to `sink`.
async fn a_backend_reporting_to(
    root: &Path,
    sink: ProgressSink,
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

    RustBackend::from_lsp_client(Arc::clone(&service.client), Some(cancel), sink)
}

/// Drive a wait that reports progress, then stop it.
///
/// `spawn_blocking` rather than a plain thread: the backend is synchronous and reaches the shared
/// LSP client through `Handle::current().block_on`, which needs a runtime context to be in.
async fn a_cancelled_wait(
    mut backend: RustBackend,
    root: std::path::PathBuf,
    cancel: CancellationToken,
) {
    let waiting = tokio::task::spawn_blocking(move || {
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };
        let _ = backend.anchor_for("src/lib.rs", &["foo".to_string()], &workspace);
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    cancel.cancel();
    waiting.await.expect("the wait unwinds");
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_reaches_the_sink_the_caller_installed() {
    // Given a backend reporting to a sink the caller owns
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let cancel = CancellationToken::new();
    let (sink, recorded) = a_recording_sink();
    let backend = a_backend_reporting_to(workspace.path(), sink, cancel.clone()).await;

    // When it waits on a loading index and is then stopped
    a_cancelled_wait(backend, workspace.path().to_path_buf(), cancel).await;

    // Then the caller's own sink is what was told about it
    let lines = recorded.lock().unwrap().clone();
    assert!(
        !lines.is_empty(),
        "the caller's sink was told nothing about a wait it was waiting on"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn one_callers_progress_does_not_reach_another_callers_sink() {
    // Given two callers, each with its own sink and its own workspace
    let one = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let other = a_workspace_holding("pub fn foo() -> u32 {\n    2\n}\n");
    let (one_sink, one_recorded) = a_recording_sink();
    let (other_sink, other_recorded) = a_recording_sink();
    let one_cancel = CancellationToken::new();
    let other_cancel = CancellationToken::new();
    let one_backend = a_backend_reporting_to(one.path(), one_sink, one_cancel.clone()).await;
    let other_backend =
        a_backend_reporting_to(other.path(), other_sink, other_cancel.clone()).await;

    // When only the first caller's wait runs
    a_cancelled_wait(one_backend, one.path().to_path_buf(), one_cancel).await;
    drop(other_backend);
    other_cancel.cancel();

    // Then the second caller's sink heard nothing about work it did not ask for
    assert!(
        !one_recorded.lock().unwrap().is_empty(),
        "the waiting caller's sink was told nothing"
    );
    assert_eq!(
        other_recorded.lock().unwrap().clone(),
        Vec::<String>::new(),
        "a caller's sink was told about another caller's wait"
    );
}
