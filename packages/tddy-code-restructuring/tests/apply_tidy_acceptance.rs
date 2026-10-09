//! An apply leaves the files it wrote ready for the lint gate and for `cargo fmt --check`.
//!
//! A move leaves imports nothing reads and writes use groups rustfmt would spell differently, and
//! CI fails on both. So a writing apply that landed every operation asks the compiler which
//! imports are unused, removes those, and formats what it wrote. A dry run writes nothing, so it
//! tidies nothing.
//!
//! Driven through the library's `apply`, with a test-binary move: the engine authors it without
//! asking the server anything.

mod harness;

use harness::{
    a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use,
    a_workspace_whose_test_binary_carries_an_unused_mut, moving_the_test_binary,
    THE_MOVED_TEST_BINARY, THE_TEST_BINARY, THE_UNUSED_IMPORT, THE_UNUSED_MUT,
};
use tddy_code_restructuring::runner::RunSummary;

#[tokio::test(flavor = "multi_thread")]
async fn removes_the_import_a_moved_test_binary_leaves_unused() {
    // Given a test binary carrying an import nothing uses
    let workspace = a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use();

    // When it is moved by an apply
    let (summary, _) = moving_the_test_binary(&workspace, false).await;

    // Then the apply succeeded and the moved file no longer holds the import
    assert_eq!(
        summary,
        Ok(RunSummary {
            applied: 1,
            total: 1,
            stopped_early: false,
        })
    );
    assert!(
        !workspace
            .read(THE_MOVED_TEST_BINARY)
            .contains(THE_UNUSED_IMPORT),
        "the unused import survived:\n{}",
        workspace.read(THE_MOVED_TEST_BINARY)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn says_which_file_it_removed_an_unused_import_from() {
    // Given a test binary carrying an import nothing uses
    let workspace = a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use();

    // When it is moved by an apply
    let (_, said) = moving_the_test_binary(&workspace, false).await;

    // Then the run's progress names the file and the count
    let line = format!("tidied: removed 1 unused import(s) from {THE_MOVED_TEST_BINARY}");
    assert!(
        said.contains(&line),
        "{line:?} was not reported:\n{said:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_the_file_it_wrote_as_rustfmt_would_write_it() {
    // Given a test binary whose use group rustfmt would spell differently
    let workspace = a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use();

    // When it is moved by an apply
    let (_, said) = moving_the_test_binary(&workspace, false).await;

    // Then the destination file is rustfmt's output, and the run said it formatted it
    assert!(
        workspace.is_rustfmt_clean(THE_MOVED_TEST_BINARY),
        "the destination is not what rustfmt writes:\n{}",
        workspace.read(THE_MOVED_TEST_BINARY)
    );
    let line = format!("formatted: {THE_MOVED_TEST_BINARY}");
    assert!(
        said.contains(&line),
        "{line:?} was not reported:\n{said:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_the_unused_import_in_place_on_a_dry_run() {
    // Given a test binary carrying an import nothing uses
    let workspace = a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use();

    // When the move is only a dry run
    let (summary, said) = moving_the_test_binary(&workspace, true).await;

    // Then nothing was written, so nothing was tidied
    summary.expect("a dry run succeeds");
    assert!(workspace.read(THE_TEST_BINARY).contains(THE_UNUSED_IMPORT));
    assert!(
        !said
            .iter()
            .any(|line| line.starts_with("tidied:") || line.starts_with("formatted:")),
        "a dry run reported tidying:\n{said:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn removes_an_unused_mut_from_a_file_the_run_wrote() {
    // Given a test binary binding a `mut` it never mutates
    let workspace = a_workspace_whose_test_binary_carries_an_unused_mut();

    // When it is moved by an apply
    let (summary, _) = moving_the_test_binary(&workspace, false).await;

    // Then the apply succeeded and the binding in the moved file is no longer `mut`
    summary.expect("the apply succeeds");
    let moved = workspace.read(THE_MOVED_TEST_BINARY);
    assert!(
        !moved.contains(THE_UNUSED_MUT) && moved.contains("let doubled = 2 * 2;"),
        "the unused `mut` survived:\n{moved}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn says_which_file_it_removed_an_unused_mut_from() {
    // Given a test binary binding a `mut` it never mutates
    let workspace = a_workspace_whose_test_binary_carries_an_unused_mut();

    // When it is moved by an apply
    let (_, said) = moving_the_test_binary(&workspace, false).await;

    // Then the run's progress names the file and the count
    let line = format!("tidied: removed 1 unused `mut`(s) from {THE_MOVED_TEST_BINARY}");
    assert!(
        said.contains(&line),
        "{line:?} was not reported:\n{said:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_removes_no_unused_mut() {
    // Given a test binary binding a `mut` it never mutates
    let workspace = a_workspace_whose_test_binary_carries_an_unused_mut();

    // When the move is only a dry run
    let (summary, _) = moving_the_test_binary(&workspace, true).await;

    // Then nothing moved and the binding is as it was
    summary.expect("a dry run succeeds");
    assert!(
        workspace.read(THE_TEST_BINARY).contains(THE_UNUSED_MUT),
        "a dry run edited the test binary:\n{}",
        workspace.read(THE_TEST_BINARY)
    );
}
