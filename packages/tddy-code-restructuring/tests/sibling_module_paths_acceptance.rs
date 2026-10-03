//! An inline path into a module the parent declares, across a seam, against a live rust-analyzer.
//!
//! One plan, two seams. The first moves `widest` into `visibility`, and the assist points `user`
//! at it as `visibility::widest()`, which resolves in `outer` because `visibility` is its child.
//! The second moves `user` into `facade`, another child of `outer`. There `visibility::widest()`
//! names nothing (`E0433`): the module is a sibling and has to be reached through `super::`.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_second_seam_calls_what_the_first_moved,
    an_extract_of_outer_functions_into_a_file, applying_a_plan_of, assert_compiles,
};
use tddy_code_restructuring::runner::RunSummary;

const WIDEST: &str = "pub(crate) fn widest() -> u32 {\n    3\n}";
const USER: &str =
    "pub(crate) fn user(member: &Member) -> bool {\n    member.visibility != 0 && widest() > 1\n}";

const THE_SECOND_FILE: &str = "crates/origin/src/outer/facade.rs";

async fn applying_both_seams(workspace: &harness::AFixtureWorkspace) -> Result<RunSummary, String> {
    let first = an_extract_of_outer_functions_into_a_file(&[("widest", WIDEST)], "visibility");
    let second = an_extract_of_outer_functions_into_a_file(&[("user", USER)], "facade");
    applying_a_plan_of(workspace, &[first, second]).await
}

#[tokio::test(flavor = "multi_thread")]
async fn applies_a_plan_whose_second_seam_reads_the_module_the_first_made() {
    // Given
    let workspace = a_crate_whose_second_seam_calls_what_the_first_moved();

    // When
    let applied = applying_both_seams(&workspace).await;

    // Then
    assert_eq!(applied.map(|run| run.applied), Ok(2));
    assert_compiles(&workspace);
}

/// The imports pass writes `use crate::outer::visibility;` into `facade` here, which binds the
/// name; re-rooting the path as well would leave a `use` and a `super::` path saying the same thing.
#[tokio::test(flavor = "multi_thread")]
async fn leaves_a_path_alone_where_the_import_pass_already_bound_its_module() {
    // Given
    let workspace = a_crate_whose_second_seam_calls_what_the_first_moved();

    // When
    applying_both_seams(&workspace).await.ok();

    // Then
    let moved = workspace.read(THE_SECOND_FILE);
    assert!(
        moved.contains("use crate::outer::visibility;")
            && moved.contains(" visibility::widest()")
            && !moved.contains("super::visibility"),
        "the path was re-rooted although the module is imported:\n{moved}"
    );
}
