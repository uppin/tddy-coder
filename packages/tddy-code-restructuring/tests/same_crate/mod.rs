//! Fixtures and plan builders shared by the same-crate move suites (`move_item`, `reparent_module`).
//!
//! A separate module rather than more of `harness/mod.rs`, which is already past two thousand
//! lines and is compiled into every acceptance binary. This one is compiled only by the suites that
//! move something inside one crate.
//!
//! Every fixture here is **one** package, `app`, so an item path reads `app::pairing::f` and the
//! tests say where something moved without a workspace manifest in the way.
//!
//! Operations are built from JSON rather than from the `RefactorKind` variants, on purpose: the
//! variants are the engine's own spelling, and a suite that named them would stop compiling instead
//! of failing with the refusal an author would actually meet ("unknown operation `move_item`").

#![allow(dead_code)]

use tddy_code_restructuring::runner::RunSummary;
use tddy_code_restructuring::{Anchor, RefactorOp};

use crate::harness::{
    a_sink_that_keeps_what_it_hears, a_workspace_holding_files, applying_a_plan_of,
    applying_the_plan_with, checking_the_plan, the_anchor_command_emits, AFixtureWorkspace,
};

/// The manifest of the one package every fixture here holds.
const THE_APP_MANIFEST: &str =
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A committed `app` package holding exactly `files` (paths relative to the package root), beside
/// its manifest.
pub fn an_app_holding(files: &[(&str, &str)]) -> AFixtureWorkspace {
    let mut all = vec![("Cargo.toml", THE_APP_MANIFEST)];
    all.extend_from_slice(files);
    a_workspace_holding_files(&all)
}

/// A committed workspace of the `app` package, holding `app_files`, and a `consumer` package that
/// depends on it, holding `consumer_files` (paths relative to each package root).
///
/// What a caller in *another crate* does when an item moves is the question a single package cannot
/// ask, which is what this builder is for.
pub fn an_app_with_a_consumer(
    app_files: &[(&str, &str)],
    consumer_files: &[(&str, &str)],
) -> AFixtureWorkspace {
    let mut all: Vec<(String, String)> = vec![
        (
            "Cargo.toml".into(),
            "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"consumer\"]\n".into(),
        ),
        (
            "app/Cargo.toml".into(),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
        ),
        (
            "consumer/Cargo.toml".into(),
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\napp = { path = \"../app\" }\n"
                .into(),
        ),
    ];
    all.extend(
        app_files
            .iter()
            .map(|(path, text)| (format!("app/{path}"), (*text).to_string())),
    );
    all.extend(
        consumer_files
            .iter()
            .map(|(path, text)| (format!("consumer/{path}"), (*text).to_string())),
    );
    let borrowed: Vec<(&str, &str)> = all
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    a_workspace_holding_files(&borrowed)
}

/// A `move_item` operation over `anchor`, into the module `to`, with the given `reexport`.
pub fn a_move_item_op(anchor: &Anchor, to: &str, reexport: Option<&str>) -> RefactorOp {
    let mut op = serde_json::json!({ "op": "move_item", "anchor": anchor, "to": to });
    if let Some(reexport) = reexport {
        op["reexport"] = serde_json::json!(reexport);
    }
    serde_json::from_value(op).expect("a `move_item` operation parses")
}

/// A `move_item` operation that **creates** its destination: `parent` is the module the new module
/// is declared in, `name` is the new module's name.
pub fn a_move_item_into_a_new_module_op(
    anchor: &Anchor,
    parent: &str,
    name: &str,
    reexport: Option<&str>,
) -> RefactorOp {
    let mut op = serde_json::json!({
        "op": "move_item", "anchor": anchor, "to": parent, "name": name,
    });
    if let Some(reexport) = reexport {
        op["reexport"] = serde_json::json!(reexport);
    }
    serde_json::from_value(op).expect("a `move_item` operation parses")
}

/// Apply a `move_item` of `names` out of `file` into a **new** module `name` under `parent`.
pub async fn moving_items_into_a_new_module(
    workspace: &AFixtureWorkspace,
    file: &str,
    names: &[&str],
    parent: &str,
    name: &str,
    reexport: Option<&str>,
) -> Result<RunSummary, String> {
    let anchor = the_anchor_over(workspace, file, names).await;
    applying_a_plan_of(
        workspace,
        &[a_move_item_into_a_new_module_op(
            &anchor, parent, name, reexport,
        )],
    )
    .await
}

/// A `reparent_module` operation over `anchor` (the module's `mod` declaration in its old parent),
/// into the module `to`, with the given `reexport`.
pub fn a_reparent_module_op(anchor: &Anchor, to: &str, reexport: Option<&str>) -> RefactorOp {
    let mut op = serde_json::json!({ "op": "reparent_module", "anchor": anchor, "to": to });
    if let Some(reexport) = reexport {
        op["reexport"] = serde_json::json!(reexport);
    }
    serde_json::from_value(op).expect("a `reparent_module` operation parses")
}

/// The anchor `restructure anchors <file> --items <names>` emits, as a plan would carry it.
pub async fn the_anchor_over(workspace: &AFixtureWorkspace, file: &str, names: &[&str]) -> Anchor {
    the_anchor_command_emits(workspace, file, names, None)
        .await
        .expect("the anchors command emits an anchor over the named items")
}

/// Apply a `move_item` of `names` out of `file`, into the module `to`.
pub async fn moving_items(
    workspace: &AFixtureWorkspace,
    file: &str,
    names: &[&str],
    to: &str,
    reexport: Option<&str>,
) -> Result<RunSummary, String> {
    let anchor = the_anchor_over(workspace, file, names).await;
    a_run_of_its_own(workspace);
    applying_a_plan_of(workspace, &[a_move_item_op(&anchor, to, reexport)]).await
}

/// Forget the journal of an earlier plan. Every plan the harness writes is `earlier-plan.jsonl`, so
/// a second move in one workspace would otherwise be read as the same run, and refused for it.
fn a_run_of_its_own(workspace: &AFixtureWorkspace) {
    match std::fs::remove_dir_all(workspace.path().join(".restructure")) {
        Ok(()) => {}
        Err(gone) if gone.kind() == std::io::ErrorKind::NotFound => {}
        Err(other) => panic!("the earlier run's journal could not be removed: {other}"),
    }
}

/// Apply a `reparent_module` of the module `name` that `parent_file` declares, under the module `to`.
pub async fn reparenting_module(
    workspace: &AFixtureWorkspace,
    parent_file: &str,
    name: &str,
    to: &str,
    reexport: Option<&str>,
) -> Result<RunSummary, String> {
    let anchor = the_anchor_over(workspace, parent_file, &[name]).await;
    applying_a_plan_of(workspace, &[a_reparent_module_op(&anchor, to, reexport)]).await
}

/// What a **static** `check` (no server) of `op` finds, as the lines a finding reads as.
///
/// A refusal the text of the tree can answer must not cost an index, which is what this proves:
/// `check` without `--deep` never reaches a language server.
pub async fn what_a_static_check_finds_in(
    workspace: &AFixtureWorkspace,
    op: RefactorOp,
) -> Vec<String> {
    let plan = workspace.a_plan_of(&[op]);
    checking_the_plan(workspace, plan, false)
        .await
        .expect("a static check runs")
}

/// A committed workspace of the `app` package, holding `app_files`, over a path crate `kernel`,
/// holding `kernel_files` (paths relative to each package root).
///
/// What `app` re-exports from `kernel` (`pub use kernel::config;`) is a facade path that names a
/// definition in another crate, which a single package cannot have.
pub fn an_app_over_a_kernel(
    app_files: &[(&str, &str)],
    kernel_files: &[(&str, &str)],
) -> AFixtureWorkspace {
    let mut all: Vec<(String, String)> = vec![
        (
            "Cargo.toml".into(),
            "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"kernel\"]\n".into(),
        ),
        (
            "app/Cargo.toml".into(),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nkernel = { path = \"../kernel\" }\n"
                .into(),
        ),
        (
            "kernel/Cargo.toml".into(),
            "[package]\nname = \"kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
        ),
    ];
    all.extend(
        app_files
            .iter()
            .map(|(path, text)| (format!("app/{path}"), (*text).to_string())),
    );
    all.extend(
        kernel_files
            .iter()
            .map(|(path, text)| (format!("kernel/{path}"), (*text).to_string())),
    );
    let borrowed: Vec<(&str, &str)> = all
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    a_workspace_holding_files(&borrowed)
}

/// A `move_item` operation over `anchor`, into the module `to`, carrying `canonical_paths: true`.
pub fn a_move_item_asking_for_canonical_paths_op(anchor: &Anchor, to: &str) -> RefactorOp {
    let op = serde_json::json!({
        "op": "move_item", "anchor": anchor, "to": to, "canonical_paths": true,
    });
    serde_json::from_value(op).expect("a `move_item` operation with `canonical_paths` parses")
}

/// Apply a `move_item` of `names` out of `file`, into the module `to`, asking for canonical paths.
/// Returns the run's outcome and every line the run reported while it went.
pub async fn moving_items_with_canonical_paths(
    workspace: &AFixtureWorkspace,
    file: &str,
    names: &[&str],
    to: &str,
) -> (Result<RunSummary, String>, Vec<String>) {
    let anchor = the_anchor_over(workspace, file, names).await;
    a_run_of_its_own(workspace);
    let plan = workspace.a_plan_of(&[a_move_item_asking_for_canonical_paths_op(&anchor, to)]);
    let (progress, heard) = a_sink_that_keeps_what_it_hears();
    let outcome =
        applying_the_plan_with(workspace, plan, |options| options.progress = progress).await;
    let lines = heard.lock().expect("the lines are readable").clone();
    (outcome, lines)
}

/// Assert every intra-doc link in the workspace resolves: `cargo doc` with broken links denied.
pub fn assert_docs_resolve(fixture: &AFixtureWorkspace) {
    let output = std::process::Command::new("cargo")
        .args(["doc", "--workspace", "--no-deps", "--quiet"])
        .current_dir(fixture.path())
        .env("CARGO_TARGET_DIR", fixture.path().join("target"))
        .env("RUSTDOCFLAGS", "-D rustdoc::broken_intra_doc_links")
        .output()
        .expect("cargo doc runs");
    assert!(
        output.status.success(),
        "an intra-doc link in the workspace does not resolve:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
