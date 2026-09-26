//! A cross-crate move reads every path the moved file names — headers and bodies, resolved and
//! followed to the crate that defines each item — and rewrites them from that one survey.
//!
//! One fixture per recorded defect, each a three-crate workspace (`shared`, `origin`,
//! `destination`) where `origin` already depends on `destination`. Each asserts the text the move
//! wrote **and** that the tree compiles, because every one of these defects was a move that
//! looked right and left `E0433`, `E0425` or a cyclic manifest behind.
//!
//! Against a live rust-analyzer; one server at a time, enforced by the harness.

mod harness;

use harness::{
    a_move_of, a_workspace_holding_files, assert_compiles, assert_compiles_with_its_tests,
    performing, DESTINATION_MANIFEST, ORIGIN_OVER_BOTH, SHARED_LIB, SHARED_MANIFEST, THREE_CRATES,
};
use tddy_code_restructuring::Reexport;

const MOVED: &str = "crates/origin/src/host_registry.rs";
const ARRIVED: &str = "crates/destination/src/host_registry.rs";
const DESTINATION_CARGO: &str = "crates/destination/Cargo.toml";

/// A workspace where `origin/src/host_registry.rs` holds `moved`, `origin/src/lib.rs` holds
/// `origin_lib`, and `destination/src/lib.rs` holds `destination_lib`.
fn a_workspace_moving(
    moved: &str,
    origin_lib: &str,
    destination_lib: &str,
) -> harness::AFixtureWorkspace {
    a_workspace_holding_files(&[
        ("Cargo.toml", THREE_CRATES),
        ("crates/shared/Cargo.toml", SHARED_MANIFEST),
        ("crates/shared/src/lib.rs", SHARED_LIB),
        ("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH),
        ("crates/origin/src/lib.rs", origin_lib),
        (MOVED, moved),
        ("crates/destination/Cargo.toml", DESTINATION_MANIFEST),
        ("crates/destination/src/lib.rs", destination_lib),
    ])
}

fn moving_the_host_registry() -> tddy_code_restructuring::RefactorOp {
    a_move_of(MOVED, "host_registry", Some(Reexport::Glob))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crate_path_through_an_origin_facade_to_the_destination_becomes_crate_relative() {
    // Given a moved file naming `crate::config`, which `origin` re-exports from `destination`
    let workspace = a_workspace_moving(
        "use crate::config::DaemonConfig;\n\npub fn configured() -> DaemonConfig {\n    DaemonConfig\n}\n",
        "pub use destination::config;\npub mod host_registry;\n",
        "pub mod config {\n    pub struct DaemonConfig;\n}\n",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then it names `crate::config` there, the destination does not depend on itself, and it builds
    assert!(workspace
        .read(ARRIVED)
        .starts_with("use crate::config::DaemonConfig;\n"));
    assert!(!workspace.read(DESTINATION_CARGO).contains("destination ="));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_naming_the_destination_by_extern_name_becomes_crate_relative_in_headers_and_bodies()
{
    // Given a moved file naming `destination::clock_face` in its header and in a body
    let workspace = a_workspace_moving(
        "use destination::clock_face::tick;\n\npub fn stamp() -> u64 {\n    \
         tick() + destination::clock_face::tick()\n}\n",
        "pub mod host_registry;\n",
        "pub mod clock_face {\n    pub fn tick() -> u64 {\n        0\n    }\n}\n",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then both paths are `crate::` and it builds
    assert_eq!(
        workspace.read(ARRIVED),
        "use crate::clock_face::tick;\n\npub fn stamp() -> u64 {\n    \
         tick() + crate::clock_face::tick()\n}\n"
    );
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_destination_is_never_added_to_its_own_manifest() {
    // Given a moved file whose header names `destination` by its extern name — the path the
    // manifest pass read as a dependency to carry
    let workspace = a_workspace_moving(
        "use destination::clock_face;\n\npub fn stamp() -> u64 {\n    clock_face::tick()\n}\n",
        "pub mod host_registry;\n",
        "pub mod clock_face {\n    pub fn tick() -> u64 {\n        0\n    }\n}\n",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then the destination's manifest is exactly as it was
    assert_eq!(workspace.read(DESTINATION_CARGO), DESTINATION_MANIFEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extern_crate_named_only_in_a_body_is_carried_to_dependencies() {
    // Given a moved file naming `shared` only inside a function body
    let workspace = a_workspace_moving(
        "pub fn clock() -> shared::Clock {\n    shared::Clock\n}\n",
        "pub mod host_registry;\n",
        "",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then `destination` depends on `shared`, re-anchored on its own directory, and it builds
    assert!(workspace
        .read(DESTINATION_CARGO)
        .contains("[dependencies]\nshared = { path = \"../shared\" }\n"));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extern_crate_named_only_under_cfg_test_is_carried_to_dev_dependencies() {
    // Given a moved file naming `shared` only in its test module
    let workspace = a_workspace_moving(
        "pub fn one() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    \
         fn a_clock_exists() {\n        let _clock = shared::Clock;\n    }\n}\n",
        "pub mod host_registry;\n",
        "",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then `shared` is a dev-dependency of `destination`, not a dependency, and its tests build
    let manifest = workspace.read(DESTINATION_CARGO);
    assert!(manifest.contains("[dev-dependencies]\nshared = { path = \"../shared\" }\n"));
    assert!(!manifest.contains("[dependencies]\nshared"));
    assert_compiles_with_its_tests(&workspace);
}

const NESTED: &str = "crates/origin/src/outer/inner.rs";
const NESTED_ARRIVED: &str = "crates/destination/src/inner.rs";

#[tokio::test(flavor = "multi_thread")]
async fn a_super_import_resolved_through_a_glob_reexport_of_the_destination_is_not_an_edge() {
    // Given `origin::outer::inner` importing `super::helper`, where `outer` globs
    // `destination::helper_mod`
    let workspace = a_workspace_holding_files(&[
        ("Cargo.toml", THREE_CRATES),
        ("crates/shared/Cargo.toml", SHARED_MANIFEST),
        ("crates/shared/src/lib.rs", SHARED_LIB),
        ("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH),
        ("crates/origin/src/lib.rs", "pub mod outer;\n"),
        (
            "crates/origin/src/outer.rs",
            "pub use destination::helper_mod::*;\npub mod inner;\n",
        ),
        (
            NESTED,
            "use super::helper;\n\npub fn call() -> u32 {\n    helper()\n}\n",
        ),
        ("crates/destination/Cargo.toml", DESTINATION_MANIFEST),
        (
            "crates/destination/src/lib.rs",
            "pub mod helper_mod {\n    pub fn helper() -> u32 {\n        1\n    }\n}\n",
        ),
    ]);

    // When `inner` moves into `destination`
    performing(&workspace, a_move_of(NESTED, "inner", Some(Reexport::Glob))).await;

    // Then it imports the helper from where `destination` defines it, and the tree builds
    assert!(workspace
        .read(NESTED_ARRIVED)
        .starts_with("use crate::helper_mod::helper;\n"));
    assert_compiles(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_module_import_whose_items_the_destination_defines_is_rewritten_to_the_destination() {
    // Given a moved file importing `crate::roster`, a module `origin` fills only by re-exporting
    // `destination::records`
    let workspace = a_workspace_moving(
        "use crate::roster;\n\npub fn qualified() -> u32 {\n    roster::qualified_id()\n}\n",
        "pub mod roster {\n    pub use destination::records::*;\n}\npub mod host_registry;\n",
        "pub mod records {\n    pub fn qualified_id() -> u32 {\n        7\n    }\n}\n",
    );

    // When the file moves into `destination`
    performing(&workspace, moving_the_host_registry()).await;

    // Then it no longer reaches back through `origin`, and the tree builds
    assert!(!workspace.read(ARRIVED).contains("crate::roster"));
    assert!(!workspace.read(ARRIVED).contains("origin::"));
    assert_compiles(&workspace);
}
