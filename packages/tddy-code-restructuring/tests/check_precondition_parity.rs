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

/// A two-crate workspace where `origin`'s `workspace_session` reaches `host` only through a body
/// path, and `host` stays behind.
fn a_workspace_whose_moving_module_reaches_its_host_in_a_body() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    for (relative, text) in [
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/origin/src/lib.rs",
            "pub mod host;\npub mod workspace_session;\n",
        ),
        (
            "crates/origin/src/host.rs",
            "pub(crate) fn project_root() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/workspace_session.rs",
            "pub fn start() -> u32 {\n    let root = crate::host::project_root();\n    root + 1\n}\n",
        ),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", "\n"),
    ] {
        let absolute = directory.path().join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }
    directory
}

#[test]
fn a_body_path_to_a_module_staying_behind_is_a_finding_naming_the_line() {
    // Given a move of `workspace_session`, whose only edge to `host` is a body path on line 2
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [a_move_of(
        "crates/origin/src/workspace_session.rs",
        "workspace_session",
    )];

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

#[test]
fn the_body_path_remedy_does_not_suggest_a_cluster_for_the_host_module() {
    let workspace = a_workspace_whose_moving_module_reaches_its_host_in_a_body();
    let plan = [a_move_of(
        "crates/origin/src/workspace_session.rs",
        "workspace_session",
    )];

    let findings = unrunnable_in(workspace.path(), &plan);

    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert!(
        !findings[0].contains("move_cluster_to_crate"),
        "the remedy suggested moving the host along: {}",
        findings[0]
    );
}

#[test]
fn a_destination_that_already_declares_the_module_is_reported_as_a_merge() {
    // Given a destination whose root already declares `host_registry`
    let workspace = a_workspace_with_both_module_shapes();
    std::fs::write(
        workspace.path().join("crates/destination/src/lib.rs"),
        "pub mod host_registry;\n",
    )
    .unwrap();
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

#[test]
fn a_destination_with_a_file_at_the_target_path_is_reported_as_a_merge() {
    // Given a destination with an undeclared `host_registry.rs` already on disk
    let workspace = a_workspace_with_both_module_shapes();
    std::fs::write(
        workspace
            .path()
            .join("crates/destination/src/host_registry.rs"),
        "pub struct Other;\n",
    )
    .unwrap();
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
