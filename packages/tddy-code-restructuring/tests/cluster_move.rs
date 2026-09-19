//! Moving a **mutually-referencing** set of modules as one unit, and keying run state by the plan.
//!
//! `move_module_to_crate` models one module. `#unbundle` node 3 moved **0 of 4** entangled modules —
//! `spawner`, `spawn_worker`, `supervisor_spawn`, `supervisor_client` — and all four were hand-moved,
//! because two mechanics defeat one-at-a-time:
//!
//! - the header pass re-points every `crate::` path at the **origin**, so a reference to a sibling
//!   that is also moving becomes a `destination → origin` edge the operation authors itself;
//! - the reference survey runs against a tree where the siblings have **not** moved, so moving one
//!   rewrites the others before they are correct. Between the first operation and the last the tree
//!   does not compile, and there is no intermediate state to verify against.
//!
//! The worse half is that `check` reported `no findings` on that plan. These tests need no server,
//! because neither decision needs one.

use std::path::{Path, PathBuf};

use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    siblings_left_behind, state_directory_for_plan, Anchor, Destination, ModuleHome, MovingCluster,
    Overlay, Reexport, RefactorKind, RefactorOp,
};

/// A crate whose four modules reference each other, plus a destination to move them to.
fn a_workspace_with_an_entangled_cluster() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path();

    for (relative, text) in [
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "crates/origin/src/lib.rs",
            "pub mod spawner;\npub mod spawn_worker;\npub mod supervisor_spawn;\n\
             pub mod supervisor_client;\n",
        ),
        (
            "crates/origin/src/spawner.rs",
            "use crate::spawn_worker::Worker;\n\npub struct Spawner;\n\n\
             impl Spawner {\n    pub fn worker(&self) -> Worker {\n        Worker\n    }\n}\n",
        ),
        (
            "crates/origin/src/spawn_worker.rs",
            "use crate::spawner::Spawner;\n\npub struct Worker;\n\n\
             impl Worker {\n    pub fn spawner(&self) -> Spawner {\n        Spawner\n    }\n}\n",
        ),
        (
            "crates/origin/src/supervisor_spawn.rs",
            "use crate::spawner::Spawner;\n\npub fn supervise() -> Spawner {\n    Spawner\n}\n",
        ),
        (
            "crates/origin/src/supervisor_client.rs",
            "use crate::supervisor_spawn::supervise;\n\npub fn client() {\n    \
             let _ = supervise();\n}\n",
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

fn a_move_of(module: &str) -> RefactorOp {
    RefactorOp {
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: format!("crates/origin/src/{module}.rs"),
            path: module.to_string(),
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

fn stranded_by(root: &Path, ops: &[RefactorOp]) -> Vec<String> {
    let overlay = Overlay::new();
    let workspace = Workspace {
        root,
        overlay: &overlay,
    };
    siblings_left_behind(&workspace, ops).expect("the check reads the workspace")
}

fn a_member(module: &str) -> ModuleHome {
    ModuleHome {
        crate_dir: "crates/origin".to_string(),
        declared_in: "crates/origin/src/lib.rs".to_string(),
        path: vec![module.to_string()],
    }
}

/// AC2/AC3 — the co-moving set is what tells a sibling coming along from one staying behind.
///
/// This is the distinction the single-module model cannot express, and every other criterion rests
/// on it: without it the header pass has no way to know that `crate::spawn_worker` will be in the
/// destination by the time the edit lands.
#[test]
fn names_the_modules_travelling_together() {
    // Given a cluster of two
    let cluster = MovingCluster {
        members: vec![a_member("spawner"), a_member("spawn_worker")],
        destination: Destination {
            dir: "crates/destination".to_string(),
            package: "destination".to_string(),
            extern_name: "destination".to_string(),
        },
        reexport: Reexport::Glob,
    };

    // When the co-moving set is asked for
    let co_moving = cluster.co_moving();

    // Then both members are in it, and nothing else
    assert!(
        co_moving.contains("spawner"),
        "a member is missing: {co_moving:?}"
    );
    assert!(
        co_moving.contains("spawn_worker"),
        "a member is missing: {co_moving:?}"
    );
    assert_eq!(
        co_moving.len(),
        2,
        "the set names something extra: {co_moving:?}"
    );
}

/// AC7 — a plan that moves half a mutually-referencing set is reported, not discovered at apply.
///
/// `spawner` and `spawn_worker` name each other. Moving only `spawner` rewrites `spawn_worker`'s
/// reference to a crate `spawn_worker` is not in.
#[test]
fn reports_the_sibling_a_partial_cluster_move_would_strand() {
    // Given a plan that moves one member of a mutually-referencing pair
    let workspace = a_workspace_with_an_entangled_cluster();
    let plan = [a_move_of("spawner")];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then the sibling left behind is named
    assert!(
        stranded.iter().any(|said| said.contains("spawn_worker")),
        "the sibling that would be stranded was not reported: {stranded:?}"
    );
}

/// AC7 — moving the whole set is reported as having nothing wrong with it.
///
/// A preflight that cries wolf is as useless as one that says nothing.
#[test]
fn reports_nothing_when_the_whole_cluster_moves() {
    // Given a plan that moves every member that references another
    let workspace = a_workspace_with_an_entangled_cluster();
    let plan = [
        a_move_of("spawner"),
        a_move_of("spawn_worker"),
        a_move_of("supervisor_spawn"),
        a_move_of("supervisor_client"),
    ];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert!(
        stranded.is_empty(),
        "a complete cluster move was reported as stranding something: {stranded:?}"
    );
}

/// AC5/AC6 — run state is keyed by the plan, so a completed plan does not block the next one.
///
/// Every `#carve` node is multi-plan by construction: one plan carves a flat module, the next moves
/// it. Today the second is refused because `.restructure/` is one directory per repository, and
/// `--resume` would resume the first plan against the second's coordinates.
#[test]
fn keys_run_state_by_the_plan_rather_than_the_repository() {
    // Given two plans in one repository
    let root = PathBuf::from("/repo");
    let carve = Path::new("tmp/carve-plans/02-parser-phases.jsonl");
    let moves = Path::new("tmp/carve-plans/03-tdd-hooks.jsonl");

    // When each is asked where its state lives
    let first = state_directory_for_plan(&root, carve).expect("a state directory");
    let second = state_directory_for_plan(&root, moves).expect("a state directory");

    // Then they do not share it
    assert_ne!(
        first, second,
        "two plans share one state directory, so a completed plan blocks the next"
    );
    assert!(
        first.starts_with(root.join(".restructure")),
        "state left the directory it belongs in: {}",
        first.display()
    );
}

/// The same plan resolves to the same directory, so `--resume` finds what it left.
#[test]
fn gives_one_plan_a_stable_state_directory() {
    // Given one plan, asked twice
    let root = PathBuf::from("/repo");
    let plan = Path::new("tmp/carve-plans/02-parser-phases.jsonl");

    // When
    let first = state_directory_for_plan(&root, plan).expect("a state directory");
    let again = state_directory_for_plan(&root, plan).expect("a state directory");

    // Then
    assert_eq!(
        first, again,
        "a plan's state directory is not stable, so --resume could not find it"
    );
}
