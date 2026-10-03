//! A type that escapes through a signature, against a live rust-analyzer.
//!
//! The visibility pass keeps a moved item at its original visibility when nothing outside the new
//! module reaches it. "Reaches" is a survey of path references, and a type that leaves the module
//! only as the return type of a widened function has none: callers infer it. Narrowing it back to
//! private then leaves a `pub(crate) fn` returning a private type, which does not compile.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_private_helper_has_only_its_moved_caller,
    a_crate_whose_tests_read_a_field_of_a_moved_functions_private_return_type,
    an_extract_module_of, assert_compiles_with_its_tests, performing_once_settled,
    the_module_named, ORIGIN_LIB,
};

/// `struct W` and `fn make() -> W` in both fixtures' seams.
const THE_TYPE_AND_ITS_CONSTRUCTOR: std::ops::RangeInclusive<u32> = 3..=9;

/// The tests call `make()` and read `.line` off the result. `make` is reached from outside, so it is
/// widened; `W` is named by no path outside, but it is in `make`'s signature, so it must stay widened
/// with its field.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_type_widened_when_a_widened_function_returns_it() {
    // Given
    let workspace = a_crate_whose_tests_read_a_field_of_a_moved_functions_private_return_type();
    let seam = an_extract_module_of(
        &workspace,
        ORIGIN_LIB,
        THE_TYPE_AND_ITS_CONSTRUCTOR,
        "making",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "making");
    assert!(
        module.contains("pub(crate) struct W {"),
        "the returned type was narrowed back to private:\n{module}"
    );
    assert_compiles_with_its_tests(&workspace);
}

/// Nothing reaches `helper` and it is in no widened signature: it still travels private with its
/// only caller.
#[tokio::test(flavor = "multi_thread")]
async fn keeps_a_helper_private_that_nothing_reaches() {
    // Given
    let workspace = a_crate_whose_private_helper_has_only_its_moved_caller();
    let seam = an_extract_module_of(
        &workspace,
        ORIGIN_LIB,
        THE_TYPE_AND_ITS_CONSTRUCTOR,
        "calling",
    );

    // When
    performing_once_settled(&workspace, seam).await;

    // Then
    let module = the_module_named(&workspace.read(ORIGIN_LIB), "calling");
    assert!(
        module.contains("\n    fn helper() -> u32 {"),
        "the helper was widened although nothing reaches it:\n{module}"
    );
}
