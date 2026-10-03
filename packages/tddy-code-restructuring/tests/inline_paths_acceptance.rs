//! Inline `super::` and `self::` paths across a seam, against a live rust-analyzer.
//!
//! `super::top_level()` written in `outer` names the crate root. The same text in the new child
//! `outer::inner` names `outer`, where `top_level` is not: `E0425`. The assist rebases nothing in
//! the moved code, so the paths in it have to be written one module deeper.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_moved_function_calls_through_relative_paths, an_extract_module_of,
    assert_compiles, performing_once_settled, OUTER_MODULE,
};
use tddy_code_restructuring::Reexport;

/// `caller`, the last function of `outer.rs`.
const THE_CALLER: std::ops::RangeInclusive<u32> = 7..=9;

const THE_MOVED_FILE: &str = "crates/origin/src/outer/inner.rs";

fn splitting_the_caller_into_a_file(
    workspace: &harness::AFixtureWorkspace,
) -> tddy_code_restructuring::RefactorOp {
    let mut seam = an_extract_module_of(workspace, OUTER_MODULE, THE_CALLER, "inner");
    seam.reexport = Some(Reexport::Named);
    seam.to_file = true;
    seam
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_a_super_path_one_module_deeper() {
    // Given
    let workspace = a_crate_whose_moved_function_calls_through_relative_paths();
    let seam = splitting_the_caller_into_a_file(&workspace);

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    assert_compiles(&workspace);
    let moved = workspace.read(THE_MOVED_FILE);
    assert!(
        moved.contains("super::super::top_level()"),
        "the moved `super::` path still names the module it was written in:\n{moved}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_crate_path_as_it_was_written() {
    // Given
    let workspace = a_crate_whose_moved_function_calls_through_relative_paths();
    let seam = splitting_the_caller_into_a_file(&workspace);

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let moved = workspace.read(THE_MOVED_FILE);
    assert!(
        moved.contains("crate::top_level()") && !moved.contains("super::crate"),
        "the absolute path was rewritten:\n{moved}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn turns_a_self_path_into_a_path_through_the_parent() {
    // Given
    let workspace = a_crate_whose_moved_function_calls_through_relative_paths();
    let seam = splitting_the_caller_into_a_file(&workspace);

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    assert_compiles(&workspace);
    let moved = workspace.read(THE_MOVED_FILE);
    assert!(
        moved.contains("super::sibling()") && !moved.contains("self::sibling()"),
        "the `self::` path still names the module the code left:\n{moved}"
    );
}
