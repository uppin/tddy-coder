//! Moving a **directory-shaped** module to another crate, against a live rust-analyzer.
//!
//! `source_crate_of` requires the anchor to be `<crate>/src/<module>.rs`, so every nested path is
//! refused before the server is ever spawned. That is documented as a known limitation, but nested
//! is the *normal* shape of a subsystem worth extracting: `model_registry/` was chosen as
//! `#unbundle` node 2's opening move precisely because it was the cleanest extraction available —
//! already directory-shaped, zero outbound `crate::` edges, zero inline tests — and the operation
//! moved 0 of its 13 modules.
//!
//! `cargo check` is the assertion that cannot be satisfied by an edit which merely looks right: a
//! parent left declaring a module that is no longer there fails it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_move_of, a_workspace_whose_module_is_nested, assert_compiles, performing, refusal_from,
};

const NESTED: &str = "crates/origin/src/model_registry/store.rs";
const MOVED_TO: &str = "crates/destination/src/store.rs";

/// AC1 — a module its parent declares moves, and the parent stops declaring it.
///
/// The destination module path comes from the anchor's own `path`, which already carries the
/// nesting; the parent's `mod` line is located rather than guessed.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_its_parent_module_file_declares() {
    // Given
    let workspace = a_workspace_whose_module_is_nested(false);

    // When
    performing(&workspace, a_move_of(NESTED, "model_registry::store", None)).await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert!(!workspace.holds(NESTED), "the module file did not leave");
    assert!(
        !workspace
            .read("crates/origin/src/model_registry.rs")
            .contains("pub mod store;"),
        "the parent still declares a module that is no longer beside it"
    );
    assert_compiles(&workspace);
}

/// AC2 — the same, for the other form Rust 2018 allows the parent to take.
///
/// `<parent>.rs` and `<parent>/mod.rs` are the same declaration in two places, and an operation
/// that handled only one would refuse half the subsystems in this workspace.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_module_declared_by_a_mod_rs_parent() {
    // Given
    let workspace = a_workspace_whose_module_is_nested(true);

    // When
    performing(&workspace, a_move_of(NESTED, "model_registry::store", None)).await;

    // Then
    assert!(workspace.holds(MOVED_TO), "the module file did not arrive");
    assert!(
        !workspace
            .read("crates/origin/src/model_registry/mod.rs")
            .contains("pub mod store;"),
        "the mod.rs parent still declares a module that is no longer beside it"
    );
    assert_compiles(&workspace);
}

/// AC3 — the refusal survives for the case it was written for, and says where it looked.
///
/// A parent that exists as neither file is a plan defect. Naming both candidates is what makes it
/// one a reader can fix, rather than one they have to reverse-engineer.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_module_whose_parent_exists_nowhere() {
    // Given
    let workspace = a_workspace_whose_module_is_nested(false);
    workspace.removing("crates/origin/src/model_registry.rs");

    // When
    let refusal = refusal_from(&workspace, a_move_of(NESTED, "model_registry::store", None)).await;

    // Then
    assert!(
        refusal.contains("crates/origin/src/model_registry.rs"),
        "the refusal did not name the parent file it looked for: {refusal}"
    );
    assert!(
        refusal.contains("crates/origin/src/model_registry/mod.rs"),
        "the refusal did not name the mod.rs form it also looked for: {refusal}"
    );
}

/// A facade keeps every caller resolving through the path it already writes — for a nested module
/// exactly as for a top-level one, since the caller's path is what the re-export preserves.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_nested_module_s_callers_untouched_behind_a_facade() {
    // Given
    let workspace = a_workspace_whose_module_is_nested(false);
    let before = workspace.read("crates/origin/src/runtime.rs");

    // When
    performing(
        &workspace,
        a_move_of(
            NESTED,
            "model_registry::store",
            Some(tddy_code_restructuring::Reexport::Glob),
        ),
    )
    .await;

    // Then
    assert_eq!(
        workspace.read("crates/origin/src/runtime.rs"),
        before,
        "a caller was rewritten even though a facade was left behind"
    );
    assert_compiles(&workspace);
}
