//! `move_module_to_crate` against a live rust-analyzer and a real toolchain.
//!
//! The unit suites decide what to write from a reference set a test hands them. These two prove the
//! other half: that the reference set the *server* returns drives the same decisions, and that the
//! result compiles. `cargo check` is the assertion that cannot be satisfied by an edit which merely
//! looks right — a caller left naming a crate nobody depends on fails it, and did.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_move_of_the_host_registry, a_workspace_a_module_can_move_across, assert_compiles, performing,
};
use tddy_code_restructuring::Reexport;

const MODULE: &str = "crates/origin/src/host_registry.rs";
const MOVED_TO: &str = "crates/destination/src/host_registry.rs";
const CALLER: &str = "crates/origin/src/runtime.rs";

/// The whole operation, end to end: the file moves, the caller the server found is re-pointed, both
/// manifests gain what the new shape needs, and the workspace still compiles.
#[tokio::test(flavor = "multi_thread")]
async fn relocates_a_module_and_leaves_every_crate_compiling() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();

    // When
    performing(&workspace, a_move_of_the_host_registry(None)).await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert!(!workspace.holds(MODULE), "the module file did not leave");
    assert_eq!(
        workspace.read(CALLER),
        "use destination::host_registry::HostRegistry;\n\npub fn boot() -> u64 {\n    \
         HostRegistry::new().stamp()\n}\n",
        "the caller rust-analyzer reported was not re-pointed"
    );
    assert_compiles(&workspace);
}

/// The move a reviewer can read: the crate the module left re-exports it, so every caller keeps
/// resolving through the path it already writes and not one of them is rewritten.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_every_caller_untouched_when_a_facade_is_left_behind() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();
    let before = workspace.read(CALLER);

    // When
    performing(
        &workspace,
        a_move_of_the_host_registry(Some(Reexport::Glob)),
    )
    .await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert_eq!(
        workspace.read(CALLER),
        before,
        "a faceded move rewrote a caller"
    );
    assert_compiles(&workspace);
}
