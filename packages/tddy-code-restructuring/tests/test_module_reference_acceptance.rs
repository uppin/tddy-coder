//! A seam whose moved function the file's own `#[cfg(test)]` module reaches, against a live
//! rust-analyzer.
//!
//! `extract_module` rewrites every reference to an item it moves, including one inside a module the
//! file already had: its `mod tests`. The destructure's plan 04 moves `split_claude_extra_args` out of
//! `split_session.rs`, whose `mod withdrawal_contract_tests` imports it with
//! `use super::split_claude_extra_args;`, and was refused as though the reference sat inside an
//! already-extracted module. The assist had repointed that module's import and then rewritten the
//! call beside it as `modname::split_claude_extra_args(…)`, a path that names nothing there.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_test_module_globs_a_function_the_seam_moves,
    a_crate_whose_test_module_imports_a_function_the_seam_moves, an_extract_module_of,
    assert_compiles_with_its_tests, performing_once_settled, ORIGIN_LIB,
};

/// `pub fn base() -> u32`, which the test module calls.
const THE_SEAM_TAKING_BASE: RangeInclusive<u32> = 7..=9;

/// The test module imports the moved function by name. That `use` has to reach it in its new module.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_function_the_file_s_test_module_imports_by_name() {
    // Given
    let workspace = a_crate_whose_test_module_imports_a_function_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_BASE, "basics");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("    use super::basics::base;"),
        "the test module's import does not reach `base` in its new module:\n{lib}"
    );
    assert!(
        lib.contains("        let read = base();"),
        "the test module's call no longer goes through its own import:\n{lib}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// The test module reaches the moved function through `use super::*;`, which no longer brings it in
/// once it has moved; its call has to reach it in its new module.
#[tokio::test(flavor = "multi_thread")]
async fn moves_a_function_the_file_s_test_module_reaches_through_a_glob() {
    // Given
    let workspace = a_crate_whose_test_module_globs_a_function_the_seam_moves();
    let seam = an_extract_module_of(&workspace, ORIGIN_LIB, THE_SEAM_TAKING_BASE, "basics");

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    assert_compiles_with_its_tests(&workspace);
}
