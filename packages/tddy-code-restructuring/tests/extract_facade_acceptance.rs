//! What the facade an `extract_module` leaves in the parent re-exports, against a live
//! rust-analyzer: a glob as wide as the widest moved item, a `named` facade that keeps every `pub`
//! item on the crate's public path, and the names only the parent's tests reach re-exported for the
//! test build alone.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_parent_holds_an_unreferenced_pub_fn,
    a_crate_whose_parent_tests_alone_call_a_helper,
    a_crate_whose_root_re_exports_documented_pub_items_of_parent, a_sink_that_keeps_what_it_hears,
    an_extract_module_of, applying_a_plan_of, applying_the_plan_with,
    assert_compiles_with_its_tests, assert_lints_clean, PARENT_MODULE,
};
use tddy_code_restructuring::runner::RunSummary;
use tddy_code_restructuring::{Reexport, RefactorOp};

fn splitting_into_a_file(
    workspace: &harness::AFixtureWorkspace,
    lines: std::ops::RangeInclusive<u32>,
    name: &str,
    reexport: Reexport,
) -> RefactorOp {
    let mut seam = an_extract_module_of(workspace, PARENT_MODULE, lines, name);
    seam.reexport = Some(reexport);
    seam.to_file = true;
    seam
}

const ONE_OPERATION_APPLIED: RunSummary = RunSummary {
    applied: 1,
    total: 1,
    stopped_early: false,
};

/// A reproduction of `#live-plan 10/15`: the cold path may already write `pub use`, in which case
/// this guards it and the survey's refusal is pinned by `seam_survey_tests`.
#[tokio::test(flavor = "multi_thread")]
async fn a_glob_extraction_of_documented_pub_items_that_lib_re_exports_leaves_pub_use_and_compiles()
{
    // Given two documented, derived `pub struct`s the crate root re-exports publicly
    let workspace = a_crate_whose_root_re_exports_documented_pub_items_of_parent();
    let seam = splitting_into_a_file(&workspace, 7..=17, "group", Reexport::Glob);

    // When they are split into a file of their own with a glob facade
    let summary = applying_a_plan_of(&workspace, &[seam]).await;

    // Then the glob is `pub`, so the root's re-export still resolves
    assert_eq!(summary, Ok(ONE_OPERATION_APPLIED));
    let parent = workspace.read(PARENT_MODULE);
    assert!(parent.contains("pub use group::*;"), "{parent}");
    assert_compiles_with_its_tests(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_named_extraction_keeps_an_unreferenced_pub_fn_reachable_through_the_old_path() {
    // Given a `pub fn` nothing calls, beside one something does
    let workspace = a_crate_whose_parent_holds_an_unreferenced_pub_fn();
    let seam = splitting_into_a_file(&workspace, 3..=9, "counting", Reexport::Named);

    // When both are split into a file of their own with a named facade
    let summary = applying_a_plan_of(&workspace, &[seam]).await;

    // Then both keep their public path, and nothing is left dead
    assert_eq!(summary, Ok(ONE_OPERATION_APPLIED));
    let parent = workspace.read(PARENT_MODULE);
    assert!(
        parent.contains("pub use counting::{count, statements};"),
        "the unreferenced `pub fn` left the public path:\n{parent}"
    );
    assert_lints_clean(&workspace);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_named_extraction_gates_the_names_only_the_parents_tests_use_and_the_tidy_gates_nothing_more(
) {
    // Given a helper production code calls, and one only the parent's tests call
    let workspace = a_crate_whose_parent_tests_alone_call_a_helper();
    let seam = splitting_into_a_file(&workspace, 3..=9, "parts", Reexport::Named);
    let (sink, heard) = a_sink_that_keeps_what_it_hears();

    // When both are split into a file of their own with a named facade, through an apply
    let summary = applying_the_plan_with(&workspace, workspace.a_plan_of(&[seam]), |options| {
        options.progress = sink;
    })
    .await;

    // Then the facade already gates the test-only name, so the tidy has nothing to gate
    assert_eq!(summary, Ok(ONE_OPERATION_APPLIED));
    let parent = workspace.read(PARENT_MODULE);
    // (the assist re-points `total`'s call to `parts::part()`, so the tidy drops the production
    // name the facade wrote; the test-only name is the one that must arrive gated)
    assert!(
        parent.contains("#[cfg(test)]\npub(crate) use parts::checked;\n"),
        "the test-only name is not gated:\n{parent}"
    );
    let gated: Vec<String> = heard
        .lock()
        .expect("the progress is readable")
        .iter()
        .filter(|line| line.starts_with("gated for tests:"))
        .cloned()
        .collect();
    assert!(gated.is_empty(), "the tidy had to gate: {gated:?}");
    assert_compiles_with_its_tests(&workspace);
}
