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
    a_workspace_holding_files, applying_a_plan_of, checking_the_plan, the_anchor_command_emits,
    AFixtureWorkspace,
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

/// A `move_item` operation over `anchor`, into the module `to`, with the given `reexport`.
pub fn a_move_item_op(anchor: &Anchor, to: &str, reexport: Option<&str>) -> RefactorOp {
    let mut op = serde_json::json!({ "op": "move_item", "anchor": anchor, "to": to });
    if let Some(reexport) = reexport {
        op["reexport"] = serde_json::json!(reexport);
    }
    serde_json::from_value(op).expect("a `move_item` operation parses")
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
pub async fn the_anchor_over(
    workspace: &AFixtureWorkspace,
    file: &str,
    names: &[&str],
) -> Anchor {
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
    applying_a_plan_of(workspace, &[a_move_item_op(&anchor, to, reexport)]).await
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
