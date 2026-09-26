//! What a cross-crate move leaves behind in the origin and carries into the destination: one named
//! facade per destination for the whole plan, `pub mod` in sorted position, a nested module's
//! parent re-export re-pointed, every `use` of the moved file re-pointed at any depth, and a later
//! test-binary move that sees through the facade an earlier move wrote.
//!
//! Each of these was a move that compiled — or nearly — and left the lint job or a test build red.
//! Against a live rust-analyzer, through the runner; one server at a time.

mod harness;

use harness::{
    a_move_of, a_workspace_holding_files, applying_a_plan_of, assert_compiles, assert_lints_clean,
    performing, resolving, DESTINATION_MANIFEST, SHARED_LIB, SHARED_MANIFEST, THREE_CRATES,
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
/// one does — once it is read. Today it is not, so the move applies and the destination's test
/// build breaks. Read, it is the same edge back into `origin` the move refuses for a root-level
/// `use`, and it is refused before anything is written.
#[tokio::test(flavor = "multi_thread")]
async fn a_crate_use_inside_the_moved_files_mod_tests_is_read_like_a_root_level_one() {
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
    let refusal = resolving(
        &workspace,
        a_move_of("crates/origin/src/host_registry.rs", "host_registry", None),
    )
    .await;

    // Then it is refused naming the inner path — a prefix, because the rest of the message is the
    // remedy text the refusal already carries for a root-level edge
    assert!(
        refusal
            .expect_err("a move whose test module reaches back into origin is refused")
            .starts_with(
                "plan is malformed: `crates/origin/src/host_registry.rs` still names `origin` \
                 (origin::runtime::boot)"
            ),
        "the refusal did not name the test module's path"
    );
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
