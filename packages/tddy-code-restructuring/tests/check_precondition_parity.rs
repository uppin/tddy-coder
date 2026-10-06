//! `restructure check` must refuse what `apply` would refuse.
//!
//! Twice now, a plan has passed `check` with `no findings` and then been rejected outright by
//! `apply`: the 13-op `model_registry/` plan, on the nested-module refusal, and the four-op
//! `tddy-spawn` plan, on the cluster one. The backlog calls that the worse half of both defects —
//! *"a plan that passes `check` reads as safe"*.
//!
//! Both decisions are made **before rust-analyzer is spawned**, from files on disk. So `check` can
//! reach the same verdict statically, for free, and there is no reason for it not to.
//!
//! These tests need no server, which is the point.

use std::path::Path;

use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    unrunnable_moves, Anchor, Overlay, Reexport, RefactorKind, RefactorOp,
};

/// A workspace on disk with a nested module and a top-level one, and a destination crate.
fn a_workspace_with_both_module_shapes() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path();

    for (relative, text) in [
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/origin/src/lib.rs",
            "pub mod model_registry;\npub mod host_registry;\n",
        ),
        ("crates/origin/src/model_registry.rs", "pub mod store;\n"),
        (
            "crates/origin/src/model_registry/store.rs",
            "pub struct Store;\n",
        ),
        (
            "crates/origin/src/host_registry.rs",
            "pub struct HostRegistry;\n",
        ),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", "\n"),
    ] {
        let absolute = root.join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }

    directory
}

fn a_move_of(file: &str, path: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: file.to_string(),
            path: path.to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport: Some(Reexport::Glob),
        to_file: false,
        also: Vec::new(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
    }
}

fn unrunnable_in(root: &Path, ops: &[RefactorOp]) -> Vec<String> {
    let overlay = Overlay::new();
    let workspace = Workspace {
        root,
        overlay: &overlay,
    };
    unrunnable_moves(&workspace, ops).expect("the preconditions read the workspace")
}

/// AC7 — a move whose parent module file exists nowhere is reported by `check`, not only by `apply`.
///
/// This is the shape that has twice reached `apply` looking safe.
#[test]
fn reports_a_move_whose_parent_module_file_is_absent() {
    // Given a plan naming a nested module whose parent was never written
    let workspace = a_workspace_with_both_module_shapes();
    std::fs::remove_file(workspace.path().join("crates/origin/src/model_registry.rs"))
        .expect("the parent is removed");
    let plan = [a_move_of(
        "crates/origin/src/model_registry/store.rs",
        "model_registry::store",
    )];

    // When
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert!(
        findings[0].contains("model_registry"),
        "the finding did not name the module it is about: {}",
        findings[0]
    );
}

/// AC7 — a plan `apply` would run is reported as having nothing wrong with it.
///
/// A preflight that cries wolf is as useless as one that says nothing, so the negative case is the
/// other half of the criterion.
#[test]
fn reports_nothing_for_a_plan_that_can_run() {
    // Given a plan whose nested module has a parent, and whose top-level module has a crate root
    let workspace = a_workspace_with_both_module_shapes();
    let plan = [
        a_move_of(
            "crates/origin/src/model_registry/store.rs",
            "model_registry::store",
        ),
        a_move_of("crates/origin/src/host_registry.rs", "host_registry"),
    ];

    // When
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then
    assert!(
        findings.is_empty(),
        "a runnable plan was reported as unrunnable: {findings:?}"
    );
}

/// AC7 — findings come back in plan order, one per operation that cannot run.
///
/// A reader fixes a plan by operation index, so a finding that cannot be tied to one is a finding
/// they have to re-derive.
#[test]
fn reports_one_finding_per_unrunnable_operation_in_plan_order() {
    // Given a plan whose first and third moves name modules that are not there
    let workspace = a_workspace_with_both_module_shapes();
    let plan = [
        a_move_of("crates/origin/src/absent_one.rs", "absent_one"),
        a_move_of("crates/origin/src/host_registry.rs", "host_registry"),
        a_move_of("crates/origin/src/absent_two.rs", "absent_two"),
    ];

    // When
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then
    assert_eq!(findings.len(), 2, "expected two findings, got {findings:?}");
    assert!(
        findings[0].contains("absent_one"),
        "the first finding is not the first unrunnable move: {}",
        findings[0]
    );
    assert!(
        findings[1].contains("absent_two"),
        "the second finding is not the second unrunnable move: {}",
        findings[1]
    );
}

/// A move outside `src/` is not a cross-crate move at all, and is reported rather than panicking.
#[test]
fn reports_a_module_that_is_in_no_crate() {
    // Given an anchor that sits outside any crate's `src/`
    let workspace = a_workspace_with_both_module_shapes();
    let plan = [a_move_of("scripts/helper.rs", "helper")];

    // When
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
}

fn a_manifest_named(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
}

/// `origin` holding the given `src/` files, and an empty `destination`.
///
/// A file named here replaces the default of the same path, which is how a test gives `origin` a
/// manifest of its own.
fn an_origin_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let origin_manifest = a_manifest_named("origin");
    let destination_manifest = a_manifest_named("destination");
    let defaults = [
        ("crates/origin/Cargo.toml", origin_manifest.as_str()),
        (
            "crates/destination/Cargo.toml",
            destination_manifest.as_str(),
        ),
        ("crates/destination/src/lib.rs", "\n"),
    ];
    for (relative, text) in defaults.iter().chain(files.iter()) {
        let absolute = directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }
    directory
}

const ORIGIN_DECLARING_HOST_AND_WORKSPACE_SESSION: &str =
    "pub mod host;\npub mod workspace_session;\n";
const THE_HOST_MODULE: &str = "pub(crate) fn project_root() -> u32 {\n    1\n}\n";
const REACHES_HOST_IN_A_BODY: &str =
    "pub fn start() -> u32 {\n    let root = crate::host::project_root();\n    root + 1\n}\n";

/// An origin whose `workspace_session` reaches `host` only through a body path on line 2, and
/// whose `host` stays behind.
fn a_workspace_whose_moving_module_reaches_its_host_in_a_body() -> tempfile::TempDir {
    an_origin_holding(&[
        (
            "crates/origin/src/lib.rs",
            ORIGIN_DECLARING_HOST_AND_WORKSPACE_SESSION,
        ),
        ("crates/origin/src/host.rs", THE_HOST_MODULE),
        (
            "crates/origin/src/workspace_session.rs",
            REACHES_HOST_IN_A_BODY,
        ),
    ])
}

fn a_move_of_workspace_session() -> RefactorOp {
    a_move_of(
        "crates/origin/src/workspace_session.rs",
        "workspace_session",
    )
}

/// The destination's root declares `module`, as it would after an earlier move of its own.
fn a_destination_declaring(workspace: &tempfile::TempDir, module: &str) {
    std::fs::write(
        workspace.path().join("crates/destination/src/lib.rs"),
        format!("pub mod {module};\n"),
    )
    .expect("the destination's root is rewritten");
}

/// The destination holding a file at `relative` under its `src/`, which its root does not declare.
fn a_destination_holding_a_file_at(workspace: &tempfile::TempDir, relative: &str) {
    std::fs::write(
        workspace
            .path()
            .join("crates/destination/src")
            .join(relative),
        "pub struct Other;\n",
    )
    .expect("the destination's file is written");
}

/// A body path to a module that stays behind is named with the line it is written on.
///
/// The header pass cannot see this edge — there is no `use` line — and `check --deep` passed it
/// while the move itself could not build.
#[test]
fn a_body_path_to_a_module_staying_behind_is_a_finding_naming_the_line() {
    // Given a move of `workspace_session`, whose only edge to `host` is a body path on line 2
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then there is one finding, naming the path and where it is written
    assert_eq!(
        findings,
        vec![
            "plan is malformed: `crates/origin/src/workspace_session.rs` reaches \
             `crate::host::project_root` in a body at line 2, and `host` stays behind in `origin` — \
             after the move that path names nothing in `destination`, and naming `origin` from \
             there is a cycle. Cut the body's dependency on `host` before moving the module."
                .to_string()
        ]
    );
}

/// The remedy for a body path never suggests moving the host along.
///
/// A body's reach into the code that hosts it is not a sibling that can come along, so advice to
/// `move_cluster_to_crate` would send the reader to a plan that cannot work either.
#[test]
fn the_body_path_remedy_does_not_suggest_a_cluster_for_the_host_module() {
    // Given a move of `workspace_session`, whose only edge to `host` is a body path
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then the one finding does not suggest a cluster move
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert!(
        !findings[0].contains("move_cluster_to_crate"),
        "the remedy suggested moving the host along: {}",
        findings[0]
    );
}

/// A destination whose root already declares the module is a merge, and `check` says so.
///
/// `apply` refuses it before rust-analyzer starts, so `check` has no reason to pass it.
#[test]
fn a_destination_that_already_declares_the_module_is_reported_as_a_merge() {
    // Given a destination whose root already declares `host_registry`
    let workspace = a_workspace_with_both_module_shapes();
    a_destination_declaring(&workspace, "host_registry");
    let plan = [a_move_of(
        "crates/origin/src/host_registry.rs",
        "host_registry",
    )];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then the collision is named as a merge
    assert_eq!(
        findings,
        vec![
            "plan is malformed: `destination` already declares `host_registry` in \
             crates/destination/src/lib.rs — moving `host_registry` into it would be a merge, which \
             no operation performs"
                .to_string()
        ]
    );
}

/// A file already at the target path is a merge too, even when no `mod` declaration names it.
///
/// The declaration is not the only way for the destination to bind the name, so the file on disk is
/// read as well.
#[test]
fn a_destination_with_a_file_at_the_target_path_is_reported_as_a_merge() {
    // Given a destination with an undeclared `host_registry.rs` already on disk
    let workspace = a_workspace_with_both_module_shapes();
    a_destination_holding_a_file_at(&workspace, "host_registry.rs");
    let plan = [a_move_of(
        "crates/origin/src/host_registry.rs",
        "host_registry",
    )];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then the file already there is named as a merge
    assert_eq!(
        findings,
        vec![
            "plan is malformed: `destination` already has crates/destination/src/host_registry.rs \
             — moving `host_registry` into it would be a merge, which no operation performs"
                .to_string()
        ]
    );
}

/// A body path into a module an earlier operation moved to the same destination is no finding.
///
/// The plan orders the moves so `host` is already there; reporting it would reject a plan `apply`
/// runs.
#[test]
fn a_body_path_into_a_module_an_earlier_operation_already_moved_is_no_finding() {
    // Given a plan whose first move takes `host` to the destination, and whose second moves
    // `workspace_session`, which reaches `host` only through a body path
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [
        a_move_of("crates/origin/src/host.rs", "host"),
        a_move_of_workspace_session(),
    ];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then `crate::host` is in the destination by the time the second move runs
    assert!(
        findings.is_empty(),
        "a body path into a module moved earlier was reported: {findings:?}"
    );
}

/// The same two moves in the other order are still a finding.
///
/// Without this the earlier-operation allowance could be "any operation of the plan", which would
/// hide the case where `host` has not left yet.
#[test]
fn a_body_path_into_a_module_only_a_later_operation_moves_is_still_a_finding() {
    // Given the same two moves, in the order that leaves `host` behind when `workspace_session` goes
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [
        a_move_of_workspace_session(),
        a_move_of("crates/origin/src/host.rs", "host"),
    ];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then the first move is reported, because `host` has not left yet
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert!(
        findings[0].contains("`host` stays behind"),
        "the finding is not about `host`: {}",
        findings[0]
    );
}

/// An earlier move only helps when it went to the same destination.
///
/// A `host` moved to some third crate is still not in the destination of the second move, so the
/// path still names nothing there.
#[test]
fn a_body_path_into_a_module_an_earlier_operation_moved_elsewhere_is_still_a_finding() {
    // Given a first move that takes `host` to a crate other than the one `workspace_session` goes to
    let elsewhere_manifest = a_manifest_named("elsewhere");
    let workspace = an_origin_holding(&[
        (
            "crates/origin/src/lib.rs",
            ORIGIN_DECLARING_HOST_AND_WORKSPACE_SESSION,
        ),
        ("crates/origin/src/host.rs", THE_HOST_MODULE),
        (
            "crates/origin/src/workspace_session.rs",
            REACHES_HOST_IN_A_BODY,
        ),
        ("crates/elsewhere/Cargo.toml", elsewhere_manifest.as_str()),
        ("crates/elsewhere/src/lib.rs", "\n"),
    ]);
    let mut host_elsewhere = a_move_of("crates/origin/src/host.rs", "host");
    host_elsewhere.to = Some("crates/elsewhere".to_string());
    let plan = [host_elsewhere, a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then `crate::host` still names nothing in the destination of the second move
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert!(
        findings[0].contains("`host` stays behind"),
        "the finding is not about `host`: {}",
        findings[0]
    );
}

/// A cluster member's body path to the module anchoring the cluster is no finding.
///
/// Both modules go in one operation, so the path travels with them; reporting it would make a
/// cluster move impossible to plan.
#[test]
fn a_cluster_member_reaching_the_module_that_anchors_the_cluster_in_a_body_is_no_finding() {
    // Given one cluster whose anchor is `workspace_session` and whose other member, `session_log`,
    // reaches the anchor's module through a body path
    const THE_CLUSTER_ANCHOR: &str = "pub fn project_root() -> u32 {\n    1\n}\n";
    let workspace = an_origin_holding(&[
        (
            "crates/origin/src/lib.rs",
            "pub mod workspace_session;\npub mod session_log;\n",
        ),
        ("crates/origin/src/workspace_session.rs", THE_CLUSTER_ANCHOR),
        (
            "crates/origin/src/session_log.rs",
            "pub fn record() -> u32 {\n    crate::workspace_session::project_root()\n}\n",
        ),
    ]);
    let mut cluster = a_move_of_workspace_session();
    cluster.op = RefactorKind::MoveClusterToCrate;
    cluster.also = vec![Anchor::Symbol {
        file: "crates/origin/src/session_log.rs".to_string(),
        path: "session_log".to_string(),
    }];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &[cluster]);

    // Then the path travels with the cluster
    assert!(
        findings.is_empty(),
        "a path to a module moving in the same cluster was reported: {findings:?}"
    );
}

/// A body path to an item of the crate root is no finding.
///
/// `crate::helper()` is inside no module, so there is no module that "stays behind" for the
/// finding to name; the header pass owns root items.
#[test]
fn a_body_path_to_an_item_at_the_crate_root_is_no_finding() {
    // Given a move of `workspace_session`, which calls `crate::helper()`, defined in `origin`'s
    // `lib.rs` and in no module
    let workspace = an_origin_holding(&[
        (
            "crates/origin/src/lib.rs",
            "pub mod workspace_session;\npub fn helper() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/workspace_session.rs",
            "pub fn start() -> u32 {\n    crate::helper() + 1\n}\n",
        ),
    ]);
    let plan = [a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then there is nothing to report
    assert!(
        findings.is_empty(),
        "a body path to a crate-root item was reported: {findings:?}"
    );
}

/// A body path to something `origin` only re-exports from another crate is no finding.
///
/// The item is defined elsewhere and does not stay behind in `origin`; the path keeps working after
/// the move.
#[test]
fn a_body_path_to_something_origin_only_reexports_from_another_crate_is_no_finding() {
    // Given a move of `workspace_session`, which calls `crate::facade::fetch()`, where `facade` is
    // nothing but a glob re-export of the path dependency `shared`
    let origin_manifest = format!(
        "{}\n[dependencies]\nshared = {{ path = \"../shared\" }}\n",
        a_manifest_named("origin")
    );
    let shared_manifest = a_manifest_named("shared");
    let workspace = an_origin_holding(&[
        ("crates/origin/Cargo.toml", origin_manifest.as_str()),
        (
            "crates/origin/src/lib.rs",
            "pub mod facade;\npub mod workspace_session;\n",
        ),
        ("crates/origin/src/facade.rs", "pub use shared::*;\n"),
        (
            "crates/origin/src/workspace_session.rs",
            "pub fn start() -> u32 {\n    crate::facade::fetch() + 1\n}\n",
        ),
        ("crates/shared/Cargo.toml", shared_manifest.as_str()),
        (
            "crates/shared/src/lib.rs",
            "pub fn fetch() -> u32 {\n    1\n}\n",
        ),
    ]);
    let plan = [a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then there is nothing to report
    assert!(
        findings.is_empty(),
        "a body path to a re-export of another crate was reported: {findings:?}"
    );
}

/// A body path under `#[cfg(test)]` is no finding.
///
/// Test code does not ship with the module, so it is no edge; the header pass reads it the same way.
#[test]
fn a_body_path_under_cfg_test_is_no_finding() {
    // Given a move of `workspace_session`, whose only body path to `host` sits in its
    // `#[cfg(test)] mod tests`
    let workspace = an_origin_holding(&[
        (
            "crates/origin/src/lib.rs",
            ORIGIN_DECLARING_HOST_AND_WORKSPACE_SESSION,
        ),
        ("crates/origin/src/host.rs", THE_HOST_MODULE),
        (
            "crates/origin/src/workspace_session.rs",
            "pub fn start() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn \
             reads_the_root() {\n        assert_eq!(crate::host::project_root(), 1);\n    }\n}\n",
        ),
    ]);
    let plan = [a_move_of_workspace_session()];

    // When the plan is checked
    let findings = unrunnable_in(workspace.path(), &plan);

    // Then there is nothing to report
    assert!(
        findings.is_empty(),
        "a body path under cfg(test) was reported: {findings:?}"
    );
}
