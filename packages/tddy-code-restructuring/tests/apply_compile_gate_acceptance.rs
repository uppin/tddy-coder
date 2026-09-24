//! An apply is judged by whether the tree it leaves behind compiles.
//!
//! Plan 10 of the lifecycle destructure reported "applied 6 of 6" over a crate with seven `E0308`s.
//! An operation the engine accepts can still leave source the compiler rejects — an assist's output
//! it could not see through, a file a move left behind — so `apply` ends with `cargo check
//! --all-targets` over every package it touched, and a tree that does not compile is a failed run.
//! A tree that did not compile *before* the plan is refused up front, because afterwards the two
//! could not be told apart.
//!
//! Driven through the library's `apply`, with a test-binary move: the engine authors it without
//! asking the server anything, and it can leave a binary that no longer compiles.

mod harness;

use harness::{
    a_workspace_whose_test_binary_reads_a_file_beside_it,
    a_workspace_whose_test_binary_stands_alone, applying_a_move_of_the_test_binary, ORIGIN_LIB,
};
use tddy_code_restructuring::runner::RunSummary;

#[tokio::test(flavor = "multi_thread")]
async fn fails_an_apply_that_leaves_a_tree_the_compiler_rejects() {
    // Given
    let workspace = a_workspace_whose_test_binary_reads_a_file_beside_it();

    // When
    let refusal = applying_a_move_of_the_test_binary(&workspace)
        .await
        .expect_err("an apply whose result does not compile is a failed run");

    // Then
    assert!(
        refusal.starts_with(
            "1 of 1 operation(s) were applied, and the tree no longer compiles: `cargo check \
             --all-targets -p destination -p origin` fails."
        ),
        "the refusal does not say the applied tree fails to compile:\n{refusal}"
    );
    // rustc's own wording, quoted from the check; its exact text is the toolchain's, not ours.
    assert!(
        refusal.contains("couldn't read"),
        "the refusal does not carry the compiler's error:\n{refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_the_edits_of_a_failed_apply_on_disk_for_inspection() {
    // Given
    let workspace = a_workspace_whose_test_binary_reads_a_file_beside_it();

    // When
    let _refusal = applying_a_move_of_the_test_binary(&workspace).await;

    // Then
    assert!(
        workspace.holds("crates/destination/tests/golden.rs"),
        "the edits of the failed apply were not left on disk"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_an_apply_whose_tree_still_compiles_as_the_success_it_is() {
    // Given
    let workspace = a_workspace_whose_test_binary_stands_alone();

    // When
    let summary = applying_a_move_of_the_test_binary(&workspace).await;

    // Then
    assert_eq!(
        summary,
        Ok(RunSummary {
            applied: 1,
            total: 1,
            stopped_early: false,
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_apply_to_a_tree_that_did_not_compile_before_the_plan() {
    // Given
    let workspace = a_workspace_whose_test_binary_stands_alone();
    workspace.rewriting(ORIGIN_LIB, "pub fn level() -> u32 {\n    \"two\"\n}\n");

    // When
    let refusal = applying_a_move_of_the_test_binary(&workspace)
        .await
        .expect_err("an apply to a tree that does not compile is refused");

    // Then
    assert!(
        refusal.starts_with(
            "the tree does not compile before the plan runs: `cargo check --all-targets -p \
             origin` fails, so a failure after it could not be told from one the plan caused. \
             Nothing was written."
        ),
        "the refusal does not say the tree was already broken:\n{refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_nothing_to_a_tree_that_did_not_compile_before_the_plan() {
    // Given
    let workspace = a_workspace_whose_test_binary_stands_alone();
    workspace.rewriting(ORIGIN_LIB, "pub fn level() -> u32 {\n    \"two\"\n}\n");

    // When
    let _refusal = applying_a_move_of_the_test_binary(&workspace).await;

    // Then
    assert!(
        workspace.holds("crates/origin/tests/golden.rs"),
        "the plan was applied to a tree that did not compile"
    );
}
