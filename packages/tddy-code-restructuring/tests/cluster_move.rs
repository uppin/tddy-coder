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
        id: None,
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
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

/// One `move_cluster_to_crate` of `modules`, anchored on the first and naming the rest in `also`.
fn a_cluster_move_of(modules: &[&str]) -> RefactorOp {
    let mut members = modules.iter().map(|module| a_move_of(module).anchor);
    RefactorOp {
        op: RefactorKind::MoveClusterToCrate,
        anchor: members.next().expect("a cluster names at least one module"),
        also: members.collect(),
        ..a_move_of(modules[0])
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

/// AC7 — a mutually-referencing set spread over one `move_module_to_crate` per member is reported
/// at the first operation that `apply` would refuse.
///
/// `apply` runs one operation at a time, so when operation 0 moves `spawner`, the `spawn_worker` it
/// names is still in the origin. Only that operation is reported: each later one names modules that
/// earlier operations have already moved.
#[test]
fn reports_a_whole_set_spread_over_separate_moves_at_its_first_operation() {
    // Given a plan that moves every member that references another, one operation each
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
    assert_eq!(
        stranded,
        [
            "`crates/origin/src/spawner.rs`, which operation 0 moves to `crates/destination`, names \
             `origin::spawn_worker::Worker`, which is still in `crates/origin` at that point \
             (operation 1 moves it only afterwards). So the destination would depend on the crate \
             it left, while the facade it leaves there names the destination: a cycle `apply` \
             refuses. Move them in one `move_cluster_to_crate`, with this module as its anchor and \
             what those paths reach in `also`, or leave `spawner` where it is"
        ]
    );
}

/// AC7 — moving the whole set in one `move_cluster_to_crate` is reported as having nothing wrong
/// with it.
///
/// A preflight that cries wolf is as useless as one that says nothing.
#[test]
fn reports_nothing_when_one_cluster_operation_moves_the_whole_set() {
    // Given a plan that moves every member that references another, in one operation
    let workspace = a_workspace_with_an_entangled_cluster();
    let plan = [a_cluster_move_of(&[
        "spawner",
        "spawn_worker",
        "supervisor_spawn",
        "supervisor_client",
    ])];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(stranded, Vec::<String>::new());
}

/// A module an **earlier** operation moved is already in the destination, so naming it is not an
/// edge back to the origin.
#[test]
fn reports_nothing_for_a_module_naming_one_an_earlier_operation_moved() {
    // Given the pair moved first, then a module naming one of them
    let workspace = a_workspace_with_an_entangled_cluster();
    let plan = [
        a_cluster_move_of(&["spawner", "spawn_worker"]),
        a_move_of("supervisor_spawn"),
    ];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(stranded, Vec::<String>::new());
}

/// The same two operations the other way round: the module now leaves while what it names is still
/// in the origin, and only the order changed.
#[test]
fn reports_a_module_naming_one_a_later_operation_moves() {
    // Given a module naming `spawner` moved first, then the pair
    let workspace = a_workspace_with_an_entangled_cluster();
    let plan = [
        a_move_of("supervisor_spawn"),
        a_cluster_move_of(&["spawner", "spawn_worker"]),
    ];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(
        stranded,
        [
            "`crates/origin/src/supervisor_spawn.rs`, which operation 0 moves to \
             `crates/destination`, names `origin::spawner::Spawner`, which is still in \
             `crates/origin` at that point (operation 1 moves it only afterwards). So the \
             destination would depend on the crate it left, while the facade it leaves there names \
             the destination: a cycle `apply` refuses. Move them in one `move_cluster_to_crate`, \
             with this module as its anchor and what those paths reach in `also`, or leave \
             `supervisor_spawn` where it is"
        ]
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

/// An `origin` crate over `shared`, with each of `files` (relative to `crates/origin/src`, the root
/// included), and an empty `destination`.
fn an_origin_over_shared_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path();
    let shared_files = [
        (
            "crates/shared/Cargo.toml".to_string(),
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".to_string(),
        ),
        (
            "crates/shared/src/lib.rs".to_string(),
            "pub mod clock {\n    pub struct Clock;\n}\n".to_string(),
        ),
        (
            "crates/origin/Cargo.toml".to_string(),
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n"
                .to_string(),
        ),
        (
            "crates/destination/Cargo.toml".to_string(),
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
                .to_string(),
        ),
        (
            "crates/destination/src/lib.rs".to_string(),
            "\n".to_string(),
        ),
    ];
    let origin_files = files
        .iter()
        .map(|(relative, text)| (format!("crates/origin/src/{relative}"), (*text).to_string()));
    for (relative, text) in shared_files.into_iter().chain(origin_files) {
        let absolute = root.join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent")).expect("directories");
        std::fs::write(absolute, text).expect("the file is written");
    }
    directory
}

/// A `move_module_to_crate` of the module file at `file` (relative to `crates/origin/src`), leaving
/// a facade.
fn a_move_of_the_file(file: &str, module: &str) -> RefactorOp {
    RefactorOp {
        anchor: Anchor::Symbol {
            file: format!("crates/origin/src/{file}"),
            path: module.to_string(),
        },
        ..a_move_of(module)
    }
}

/// Test 17 (`#reshape` 8/19) — a grouped `use` whose leaves land on different qualifiers is split by
/// the move, so the static check reads it as the move will write it instead of refusing the plan.
#[test]
fn a_static_check_of_a_cluster_with_a_mixed_grouped_use_reports_nothing_and_does_not_refuse() {
    // Given `spawner` naming its co-moving sibling and a `shared` item through a facade in one group
    let workspace = an_origin_over_shared_holding(&[
        (
            "lib.rs",
            "pub mod spawn_worker;\npub mod spawner;\n\npub use shared::clock;\n",
        ),
        (
            "spawner.rs",
            "use crate::{spawn_worker::Worker, clock::Clock};\n\n\
             pub fn pair() -> (Worker, Clock) {\n    (Worker, Clock)\n}\n",
        ),
        ("spawn_worker.rs", "pub struct Worker;\n"),
    ]);
    let plan = [a_cluster_move_of(&["spawner", "spawn_worker"])];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(stranded, Vec::<String>::new());
}

/// Test 18 (`#reshape` 8/19) — `apply` refuses on every path the moved file writes outside
/// `#[cfg(test)]`, so `check` reads a `use` nested in a function too.
#[test]
fn reports_a_sibling_reached_from_a_use_nested_in_a_function() {
    // Given `spawner` importing a function of `spawn_worker` inside a function body
    let workspace = an_origin_over_shared_holding(&[
        ("lib.rs", "pub mod spawn_worker;\npub mod spawner;\n"),
        (
            "spawner.rs",
            "pub fn spawned() -> u32 {\n    use crate::spawn_worker::count;\n    count()\n}\n",
        ),
        ("spawn_worker.rs", "pub fn count() -> u32 {\n    1\n}\n"),
    ]);
    let plan = [a_move_of("spawner")];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then the sibling the nested `use` reaches is named
    assert!(
        stranded.len() == 1 && stranded[0].contains("origin::spawn_worker::count"),
        "the nested `use` was not read: {stranded:?}"
    );
}

/// Test 19 (`#reshape` 8/19) — a path through the origin's glob facade to a module the same cluster
/// moves travels with it, so the check strands nothing.
#[test]
fn reports_nothing_for_a_cluster_whose_member_reaches_another_through_a_glob_facade() {
    // Given `agent_host_callbacks` naming `SeededAgentClones` through `pub use seed_codebase::*;`
    let workspace = an_origin_over_shared_holding(&[
        (
            "lib.rs",
            "pub mod agent_host_callbacks;\npub mod seed_codebase;\n\npub use seed_codebase::*;\n",
        ),
        (
            "agent_host_callbacks.rs",
            "use crate::SeededAgentClones;\n\npub fn clones(_clones: &dyn SeededAgentClones) {}\n",
        ),
        ("seed_codebase.rs", "pub trait SeededAgentClones {}\n"),
    ]);
    let plan = [a_cluster_move_of(&[
        "agent_host_callbacks",
        "seed_codebase",
    ])];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(stranded, Vec::<String>::new());
}

/// Test 20 (`#reshape` 8/19) — a `pub(in …)` restriction is a visibility, not a path back into the
/// origin, so it strands nothing — however many paths the finding reads.
#[test]
fn does_not_report_a_pub_in_restriction_as_a_stranded_sibling() {
    // Given a member of `connection_service` restricted to it
    let workspace = an_origin_over_shared_holding(&[
        ("lib.rs", "pub mod connection_service;\n"),
        (
            "connection_service.rs",
            "pub mod attached_initial_prompt;\n\npub fn serve() -> u32 {\n    \
             attached_initial_prompt::prompt()\n}\n",
        ),
        (
            "connection_service/attached_initial_prompt.rs",
            "pub(in crate::connection_service) fn prompt() -> u32 {\n    1\n}\n",
        ),
    ]);
    let plan = [a_move_of_the_file(
        "connection_service/attached_initial_prompt.rs",
        "attached_initial_prompt",
    )];

    // When
    let stranded = stranded_by(workspace.path(), &plan);

    // Then
    assert_eq!(stranded, Vec::<String>::new());
}
