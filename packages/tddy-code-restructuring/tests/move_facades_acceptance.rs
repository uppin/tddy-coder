//! What a cross-crate move leaves behind in the origin and carries into the destination: one named
//! facade per destination for the whole plan, `pub mod` in sorted position, a nested module's
//! parent re-export re-pointed, every `use` of the moved file re-pointed at any depth, and a later
//! test-binary move that sees through the facade an earlier move wrote.
//!
//! Each of these was a move that compiled — or nearly — and left the lint job or a test build red.
//! Against a live rust-analyzer, through the runner; one server at a time.

mod harness;

use harness::{
    a_move_of, a_workspace_holding_files, applying_a_plan_of, assert_compiles,
    assert_compiles_with_its_tests, assert_lints_clean, checking_the_plan, performing,
    DESTINATION_MANIFEST, SHARED_LIB, SHARED_MANIFEST, THREE_CRATES,
};
use tddy_code_restructuring::{Anchor, Reexport, RefactorKind, RefactorOp};

const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";
const DESTINATION_LIB: &str = "crates/destination/src/lib.rs";

/// `origin` depends on `shared` only, so a move into `destination` is a clean one-way edge.
const ORIGIN_OVER_SHARED: &str =
    "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                                  [dependencies]\nshared = { path = \"../shared\" }\n";

fn a_workspace_of(files: &[(&str, &str)]) -> harness::AFixtureWorkspace {
    let mut all = vec![
        ("Cargo.toml", THREE_CRATES),
        ("crates/shared/Cargo.toml", SHARED_MANIFEST),
        ("crates/shared/src/lib.rs", SHARED_LIB),
        ("crates/destination/Cargo.toml", DESTINATION_MANIFEST),
    ];
    all.extend_from_slice(files);
    a_workspace_holding_files(&all)
}

fn a_glob_move_of(file: &str, module: &str) -> RefactorOp {
    a_move_of(file, module, Some(Reexport::Glob))
}

#[tokio::test(flavor = "multi_thread")]
async fn three_modules_moved_to_one_destination_leave_one_grouped_facade() {
    // Given three modules of `origin`, each used from its root
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            "pub mod auth;\npub mod config;\npub mod paths;\n\npub fn total() -> u32 {\n    \
             auth::one() + config::two() + paths::three()\n}\n",
        ),
        (
            "crates/origin/src/auth.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/config.rs",
            "pub fn two() -> u32 {\n    2\n}\n",
        ),
        (
            "crates/origin/src/paths.rs",
            "pub fn three() -> u32 {\n    3\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ]);

    // When one plan moves all three into `destination`
    let summary = applying_a_plan_of(
        &workspace,
        &[
            a_glob_move_of("crates/origin/src/auth.rs", "auth"),
            a_glob_move_of("crates/origin/src/config.rs", "config"),
            a_glob_move_of("crates/origin/src/paths.rs", "paths"),
        ],
    )
    .await;

    // Then `origin`'s root carries one line naming what moved, and the workspace lints clean
    assert_eq!(summary.map(|run| run.applied), Ok(3));
    assert_eq!(
        workspace
            .read(ORIGIN_LIB)
            .matches("pub use destination::")
            .collect::<Vec<_>>(),
        vec!["pub use destination::"]
    );
    assert!(workspace
        .read(ORIGIN_LIB)
        .contains("pub use destination::{auth, config, paths};"));
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_destination_name_the_origin_also_binds_does_not_trip_hidden_glob_reexports() {
    // Given `origin` and `destination` both defining a root `Clock`
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            "pub mod host_registry;\n\npub struct Clock;\n\npub fn boot() -> u32 {\n    \
             host_registry::one()\n}\n",
        ),
        (
            "crates/origin/src/host_registry.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (DESTINATION_LIB, "pub struct Clock;\n"),
    ]);

    // When `host_registry` moves into `destination` with a facade
    performing(
        &workspace,
        a_glob_move_of("crates/origin/src/host_registry.rs", "host_registry"),
    )
    .await;

    // Then the facade names only what moved, and the lint gate passes
    assert!(workspace
        .read(ORIGIN_LIB)
        .contains("pub use destination::host_registry;"));
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_added_to_the_destination_root_is_declared_in_sorted_position() {
    // Given a destination root declaring `alpha` and `zeta`
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (ORIGIN_LIB, "pub mod host_registry;\n"),
        (
            "crates/origin/src/host_registry.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (DESTINATION_LIB, "pub mod alpha;\npub mod zeta;\n"),
        ("crates/destination/src/alpha.rs", ""),
        ("crates/destination/src/zeta.rs", ""),
    ]);

    // When `host_registry` moves in
    performing(
        &workspace,
        a_move_of("crates/origin/src/host_registry.rs", "host_registry", None),
    )
    .await;

    // Then it is declared between them
    assert_eq!(
        workspace.read(DESTINATION_LIB),
        "pub mod alpha;\npub mod host_registry;\npub mod zeta;\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_a_nested_module_rewrites_its_parents_glob_to_the_destination() {
    // Given `origin::outer`, which declares `inner` privately and re-exports all of it
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            "pub mod outer;\n\npub fn boot() -> u32 {\n    outer::one()\n}\n",
        ),
        (
            "crates/origin/src/outer.rs",
            "mod inner;\npub use inner::*;\n",
        ),
        (
            "crates/origin/src/outer/inner.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ]);

    // When `inner` moves into `destination`
    performing(
        &workspace,
        a_move_of("crates/origin/src/outer/inner.rs", "inner", None),
    )
    .await;

    // Then `outer` re-exports it from `destination`, with no dangling `mod` line, and it builds
    assert_eq!(
        workspace.read("crates/origin/src/outer.rs"),
        "pub use destination::inner::*;\n"
    );
    assert_compiles(&workspace);
}

/// A `crate::` path inside the moved file's own `mod tests` changed meaning exactly as a root-level
/// one does, so it is read and re-pointed. Because it sits under `#[cfg(test)]` it makes `origin`
/// a dev-dependency of `destination` — never an edge back — so the move goes through and the
/// destination's test build still resolves it.
#[tokio::test(flavor = "multi_thread")]
async fn a_crate_use_inside_the_moved_files_mod_tests_is_re_pointed_at_origin() {
    // Given a moved file whose `mod tests` imports a module that stays behind in `origin`
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            "pub mod host_registry;
pub mod runtime;
",
        ),
        (
            "crates/origin/src/runtime.rs",
            "pub fn boot() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/host_registry.rs",
            "pub fn one() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    \
             use crate::runtime::boot;\n\n    #[test]\n    fn boots() {\n        \
             assert_eq!(boot(), 1);\n    }\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ]);

    // When the file is moved into `destination`
    performing(
        &workspace,
        a_move_of("crates/origin/src/host_registry.rs", "host_registry", None),
    )
    .await;

    // Then the test module names `origin` by crate, `origin` is a dev-dependency only, and the
    // tests still build
    let moved = workspace.read("crates/destination/src/host_registry.rs");
    assert!(
        moved.contains("use origin::runtime::boot;"),
        "the test module's path was not re-pointed at origin:\n{moved}"
    );
    let manifest = workspace.read("crates/destination/Cargo.toml");
    assert!(manifest.contains("[dev-dependencies]\norigin = { path = \"../origin\" }\n"));
    assert!(!manifest.contains("[dependencies]\norigin"));
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_super_glob_inside_the_moved_files_mod_tests_is_left() {
    // Given a moved file whose `mod tests` globs its own parent
    let moved =
        "pub fn one() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    \
                 #[test]\n    fn is_one() {\n        assert_eq!(one(), 1);\n    }\n}\n";
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (ORIGIN_LIB, "pub mod host_registry;\n"),
        ("crates/origin/src/host_registry.rs", moved),
        (DESTINATION_LIB, ""),
    ]);

    // When the file moves into `destination`
    performing(
        &workspace,
        a_move_of("crates/origin/src/host_registry.rs", "host_registry", None),
    )
    .await;

    // Then `super::*` still means the moved file itself, so it is written exactly as it was
    assert_eq!(
        workspace.read("crates/destination/src/host_registry.rs"),
        moved
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_binary_move_after_a_module_move_names_the_defining_crate() {
    // Given a test binary naming `origin::host_registry`, and a plan that first moves that module
    // into `destination` with a facade, then the test binary after it
    let workspace = a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (ORIGIN_LIB, "pub mod host_registry;\n"),
        ("crates/origin/src/host_registry.rs", "pub fn one() -> u32 {\n    1\n}\n"),
        (
            "crates/origin/tests/golden.rs",
            "use origin::host_registry::one;\n\n#[test]\nfn golden() {\n    assert_eq!(one(), 1);\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ]);
    let moving_the_test = RefactorOp {
        id: None,
        op: RefactorKind::MoveTestBinaryToCrate,
        anchor: Anchor::Symbol {
            file: "crates/origin/tests/golden.rs".to_string(),
            path: "golden".to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
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
    };

    // When the plan is applied
    let summary = applying_a_plan_of(
        &workspace,
        &[
            a_glob_move_of("crates/origin/src/host_registry.rs", "host_registry"),
            moving_the_test,
        ],
    )
    .await;

    // Then the moved test names the crate that now defines the module
    assert_eq!(summary.map(|run| run.applied), Ok(2));
    assert!(workspace
        .read("crates/destination/tests/golden.rs")
        .starts_with("use destination::host_registry::one;\n"));
}

/// An origin whose root declares `observer` with `above` directly over its `mod` line.
fn an_origin_declaring_the_observer_under(above: &str) -> harness::AFixtureWorkspace {
    a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            &format!(
                "{above}pub mod observer;\n\npub fn boot() -> u32 {{\n    observer::one()\n}}\n"
            ),
        ),
        (
            "crates/origin/src/observer.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_modules_doc_comment_leaves_the_origin_and_lands_on_the_destinations_declaration() {
    // Given a module whose declaration carries a doc comment
    let workspace =
        an_origin_declaring_the_observer_under("/// The observer, spawned per session.\n");

    // When it moves into `destination` with a facade
    performing(
        &workspace,
        a_glob_move_of("crates/origin/src/observer.rs", "observer"),
    )
    .await;

    // Then the doc comment is on the destination's declaration and nowhere in the origin
    assert_eq!(
        (
            workspace.read(ORIGIN_LIB).contains("The observer"),
            workspace
                .read(DESTINATION_LIB)
                .contains("/// The observer, spawned per session.\npub mod observer;\n"),
        ),
        (false, true),
        "origin:\n{}\ndestination:\n{}",
        workspace.read(ORIGIN_LIB),
        workspace.read(DESTINATION_LIB)
    );
    assert_compiles(&workspace);
}

/// An origin whose root holds two `use` runs with `pub mod beta;` the only line between them, and
/// `alpha` declared above both. Every run is rustfmt-sorted; joined, they would sort differently.
fn an_origin_whose_use_runs_beta_separates() -> harness::AFixtureWorkspace {
    a_workspace_of(&[
        ("crates/origin/Cargo.toml", ORIGIN_OVER_SHARED),
        (
            ORIGIN_LIB,
            "pub mod alpha;\n\npub use std::cell::Cell;\npub use std::rc::Rc;\npub mod beta;\n\
             pub use std::collections::HashMap;\npub use std::sync::Arc;\n\n\
             pub fn boot() -> u32 {\n    alpha::one() + beta::two()\n}\n",
        ),
        (
            "crates/origin/src/alpha.rs",
            "pub fn one() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/beta.rs",
            "pub fn two() -> u32 {\n    2\n}\n",
        ),
        (DESTINATION_LIB, ""),
    ])
}

/// The two `use` runs of [`an_origin_whose_use_runs_beta_separates`], as they stand when kept apart.
const THE_USE_RUNS_KEPT_APART: &str =
    "pub use std::cell::Cell;\npub use std::rc::Rc;\n\npub use std::collections::HashMap;\n\
     pub use std::sync::Arc;\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_move_that_removes_the_only_line_between_two_use_runs_leaves_the_origins_other_lines_where_they_were(
) {
    // Given two `use` runs that `beta`'s declaration alone separates
    let workspace = an_origin_whose_use_runs_beta_separates();

    // When `alpha` and then `beta` move with one facade, so `beta`'s line goes without a replacement
    let summary = applying_a_plan_of(
        &workspace,
        &[
            a_glob_move_of("crates/origin/src/alpha.rs", "alpha"),
            a_glob_move_of("crates/origin/src/beta.rs", "beta"),
        ],
    )
    .await;

    // Then the facade names both, and the runs keep their lines in their order
    assert_eq!(summary.map(|run| run.applied), Ok(2));
    let origin = workspace.read(ORIGIN_LIB);
    assert!(
        origin.contains("pub use destination::{alpha, beta};\n")
            && origin.contains(THE_USE_RUNS_KEPT_APART),
        "the origin's root was re-sorted:\n{origin}"
    );
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_facade_that_replaces_a_declaration_between_two_use_runs_leaves_the_run_below_in_place() {
    // Given two `use` runs that `beta`'s declaration alone separates
    let workspace = an_origin_whose_use_runs_beta_separates();

    // When `beta` alone moves with a facade, which takes the place of its declaration, through an
    // apply that formats what it wrote
    let summary = applying_a_plan_of(
        &workspace,
        &[a_glob_move_of("crates/origin/src/beta.rs", "beta")],
    )
    .await;

    // Then the run below the facade is still a run of its own
    assert_eq!(summary.map(|run| run.applied), Ok(1));
    let origin = workspace.read(ORIGIN_LIB);
    assert!(
        origin.contains(
            "pub use std::rc::Rc;\n\npub use std::collections::HashMap;\npub use std::sync::Arc;\n"
        ),
        "the run below the facade was merged into the one above:\n{origin}"
    );
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_moved_declaration_carrying_a_cfg_attribute_is_refused_by_check_and_by_apply_and_nothing_is_written(
) {
    // Given a module declared under an attribute a move can neither keep nor drop
    let workspace =
        an_origin_declaring_the_observer_under("#[cfg(not(feature = \"no-observer\"))]\n");
    let the_origin_before = workspace.read(ORIGIN_LIB);
    let the_plan = [a_glob_move_of("crates/origin/src/observer.rs", "observer")];

    // When the plan is checked, then applied
    let found = checking_the_plan(&workspace, workspace.a_plan_of(&the_plan), false).await;
    let applied = applying_a_plan_of(&workspace, &the_plan).await;

    // Then both refuse it naming the attribute, and the tree is as it was
    let names_the_attribute = |text: &str| {
        text.contains(
            "`mod observer;` in crates/origin/src/lib.rs carries `#[cfg(not(feature = \"no-observer\"))]`",
        )
    };
    assert!(
        found
            .as_ref()
            .is_ok_and(|findings| findings.iter().any(|finding| names_the_attribute(finding))),
        "the static check did not refuse it: {found:?}"
    );
    assert!(
        applied
            .as_ref()
            .is_err_and(|refusal| names_the_attribute(refusal)),
        "the apply did not refuse it: {applied:?}"
    );
    assert_eq!(
        (
            workspace.read(ORIGIN_LIB),
            workspace.holds("crates/destination/src/observer.rs")
        ),
        (the_origin_before, false)
    );
}
