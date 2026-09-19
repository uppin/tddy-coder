//! `move_cluster_to_crate` against a live rust-analyzer and a real toolchain.
//!
//! `cluster_move.rs` decides what a set should produce from a reference set a test hands it. This
//! suite proves the half no unit test can: that a plan can *say* "move these two together", that
//! the backend performs it as one operation, and that what lands compiles. The pair references
//! itself in both directions, which is the shape that moved 0 of 4 modules on `#unbundle` node 3
//! and 0 of 2 on this package's own live repro.
//!
//! `assert_compiles` is the assertion that cannot be satisfied by an edit which merely looks right.
//! The single-module suite's note records it catching one, and it caught another here: a co-moving
//! path re-pointed at the destination's *package name* reads as `E0433` from inside the destination
//! itself, and only a compiler says so.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_cluster_move_of, a_workspace_whose_modules_reference_each_other, assert_compiles, performing,
};

const SPAWNER: &str = "crates/origin/src/spawner.rs";
const WORKER: &str = "crates/origin/src/spawn_worker.rs";
const SPAWNER_MOVED_TO: &str = "crates/destination/src/spawner.rs";
const WORKER_MOVED_TO: &str = "crates/destination/src/spawn_worker.rs";

/// AC1 — a mutually-referencing pair moves in **one** operation, and the workspace still compiles.
///
/// One at a time this is impossible by construction: the first move re-points the other's path at a
/// crate it is itself about to leave, and there is no order in which the intermediate tree builds.
#[tokio::test(flavor = "multi_thread")]
async fn relocates_a_mutually_referencing_pair_in_one_operation() {
    // Given
    let workspace = a_workspace_whose_modules_reference_each_other();

    // When
    performing(
        &workspace,
        a_cluster_move_of(&["spawner", "spawn_worker"], None),
    )
    .await;

    // Then
    assert!(
        workspace.holds(SPAWNER_MOVED_TO) && workspace.holds(WORKER_MOVED_TO),
        "a member of the set did not arrive"
    );
    assert!(
        !workspace.holds(SPAWNER) && !workspace.holds(WORKER),
        "a member of the set did not leave"
    );
    assert_eq!(
        workspace.read("crates/destination/src/lib.rs"),
        "//! The crate the set moves into.\n\npub mod spawner;\npub mod spawn_worker;\n",
        "the destination does not declare both members"
    );
    assert_compiles(&workspace);
}

/// AC2/AC3/AC4 — the moved header tells a sibling that is coming along from a module staying
/// behind, and the destination is never made to depend on itself.
///
/// The sibling keeps saying `crate::`, because the destination *is* `crate` for a file that has
/// arrived in it. The module left behind reads as `origin`, which is the dependency the destination
/// gains — `destination = { path = … }` in its own manifest is what cargo would refuse, and what a
/// header re-pointed at the destination's package name would have made inevitable.
#[tokio::test(flavor = "multi_thread")]
async fn reads_a_co_moving_sibling_in_the_destination_and_one_left_behind_in_the_origin() {
    // Given
    let workspace = a_workspace_whose_modules_reference_each_other();

    // When
    performing(
        &workspace,
        a_cluster_move_of(&["spawner", "spawn_worker"], None),
    )
    .await;

    // Then
    let moved = workspace.read(SPAWNER_MOVED_TO);
    assert!(
        moved.contains("use crate::spawn_worker::Worker;"),
        "the path reaching a co-moving sibling does not resolve in the destination:\n{moved}"
    );
    assert!(
        moved.contains("use origin::limits::Limit;"),
        "the path reaching a module staying behind no longer reads as the origin:\n{moved}"
    );

    let manifest = workspace.read("crates/destination/Cargo.toml");
    assert!(
        manifest.contains("origin = { path = \"../origin\" }"),
        "the destination did not gain the crate the moved code names:\n{manifest}"
    );
    assert!(
        !manifest.contains("destination = {"),
        "the destination was made to depend on itself:\n{manifest}"
    );
    assert_compiles(&workspace);
}
