//! `extract_variable` against a live rust-analyzer, which names the binding it introduces itself.
//!
//! The engine once looked for `let var_name`, the placeholder older rust-analyzers wrote. The one
//! the dev shell ships names the binding from the expression (a field read suggests the field's
//! name), so every `extract_variable` was refused with "did not produce a `let var_name` to name".
//! The port-move pilot met it over `self.session_agent_clones`.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_method_reads_a_field, an_extract_variable_of, assert_compiles, performing,
    ORIGIN_LIB,
};

/// `self.clones.iter()…`: the line whose field read is hoisted.
const THE_LINE_READING_THE_FIELD: u32 = 9;

#[tokio::test(flavor = "multi_thread")]
async fn names_the_binding_rust_analyzer_introduced_with_the_plans_name() {
    // Given
    let workspace = a_crate_whose_method_reads_a_field();
    let read = an_extract_variable_of(
        &workspace,
        ORIGIN_LIB,
        THE_LINE_READING_THE_FIELD,
        "self.clones",
        "held",
    );

    // When
    performing(&workspace, read).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("let held = ") && lib.contains("held.iter()"),
        "the field read is not bound as `held`:\n{lib}"
    );
    assert_compiles(&workspace);
}

/// A plan whose `name` is the one rust-analyzer suggested has nothing to rename.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_binding_when_the_plan_names_it_as_rust_analyzer_did() {
    // Given
    let workspace = a_crate_whose_method_reads_a_field();
    let read = an_extract_variable_of(
        &workspace,
        ORIGIN_LIB,
        THE_LINE_READING_THE_FIELD,
        "self.clones",
        "clones",
    );

    // When
    performing(&workspace, read).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("let clones = ") && lib.contains("clones.iter()"),
        "the field read is not bound as `clones`:\n{lib}"
    );
    assert_compiles(&workspace);
}
