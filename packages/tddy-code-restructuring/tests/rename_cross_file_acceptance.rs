//! Renaming a symbol another file reaches, against a live rust-analyzer.
//!
//! `workspace_edits_for` is unit-tested over a captured rename response; what no captured response
//! can prove is that rust-analyzer *returns* the caller's document for a real rename and that this
//! client addresses it correctly — the defect this fixes was silent precisely because the anchor's
//! own file always looked right. `cargo check` is what makes the caller's half non-negotiable.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{a_rename_of, a_workspace_a_module_can_move_across, assert_compiles, performing};

const DECLARATION: &str = "crates/origin/src/host_registry.rs";
const CALLER: &str = "crates/origin/src/runtime.rs";

/// The rename reaches the caller's file, not only the declaration's. Keeping one document is what
/// left callers naming a symbol that no longer existed, and it did not surface until a later build —
/// so a later build is exactly what this asserts.
#[tokio::test(flavor = "multi_thread")]
async fn rewrites_a_caller_in_another_file() {
    // Given
    let workspace = a_workspace_a_module_can_move_across();

    // When
    performing(&workspace, a_rename_of("HostRegistry", "HostRegistryStore")).await;

    // Then
    assert_eq!(
        workspace.read(CALLER),
        "use crate::host_registry::HostRegistryStore;\n\npub fn boot() -> u64 {\n    \
         HostRegistryStore::new().stamp()\n}\n",
        "the caller kept naming the old symbol"
    );
    assert!(
        workspace
            .read(DECLARATION)
            .contains("pub struct HostRegistryStore"),
        "the declaration itself was not renamed"
    );
    assert_compiles(&workspace);
}
