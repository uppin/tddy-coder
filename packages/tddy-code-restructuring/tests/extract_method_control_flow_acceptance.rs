//! An `extract_method` over a range that returns early from its caller, against a live server.
//!
//! rust-analyzer's "extract into function" copies a `return` in the range verbatim into the new
//! function, whose return type is not the caller's. The destructure's plan 10 applied six such
//! extractions, reported "applied 6 of 6", and left seven `E0308`s behind. The engine now refuses
//! the range before asking the server anything.
//!
//! Except where the range runs to the end of the function: there the call is the caller's tail, and
//! the new function's return type is the tail's, which is the caller's, so a `return` in it means
//! what it did. The port-move pilot found six T3 methods whose tails hold one.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_function_ends_with_a_return, a_crate_whose_function_propagates_and_returns_early,
    a_crate_whose_function_returns_early, an_extract_method_of, assert_compiles, performing,
    refusal_from, ORIGIN_LIB,
};

/// `let base = 2;` through the `if x { return Ok(1); }` block: a range whose line 6 returns early.
const A_RANGE_THAT_RETURNS_EARLY: RangeInclusive<u32> = 4..=7;

/// `let base = 2;` through the tail `Ok(base)`: the same early return, in a range that runs to the
/// end of the function.
const THE_WHOLE_TAIL: RangeInclusive<u32> = 4..=8;

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_range_that_returns_early_from_the_enclosing_function() {
    // Given
    let workspace = a_crate_whose_function_returns_early();
    let statements = an_extract_method_of(
        &workspace,
        ORIGIN_LIB,
        A_RANGE_THAT_RETURNS_EARLY,
        "base_or_early",
    );

    // When
    let refusal = refusal_from(&workspace, statements).await;

    // Then
    assert!(
        refusal.starts_with(
            "this seam cannot be cut here: the range returns early from the function around it, \
             on line 6 (`return Ok(1);`)."
        ),
        "the refusal does not name the early return:\n{refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extracts_a_range_that_returns_early_when_it_runs_to_the_end_of_the_function() {
    // Given
    let workspace = a_crate_whose_function_returns_early();
    let tail = an_extract_method_of(&workspace, ORIGIN_LIB, THE_WHOLE_TAIL, "base_or_early");

    // When
    performing(&workspace, tail).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("pub fn level(x: bool) -> Result<u32, String> {\n    base_or_early(x)\n}"),
        "the call is not the caller's tail:\n{lib}"
    );
    assert!(
        lib.contains("fn base_or_early(x: bool) -> Result<u32, String> {"),
        "the new function does not return the caller's type:\n{lib}"
    );
    assert_compiles(&workspace);
}

/// Ending with `return Ok(base);` is not ending with a tail. rust-analyzer then rewrites every
/// `return` in the range into an `Option` (`return Some(Ok(base)); None`) matched at the call
/// (`if let Some(value) = base_or_early(x) { return value; }`), which leaves the caller with no
/// tail: `E0317`, measured by compiling this fixture with the range allowed.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_range_that_ends_with_the_functions_last_return() {
    // Given
    let workspace = a_crate_whose_function_ends_with_a_return();
    let tail = an_extract_method_of(&workspace, ORIGIN_LIB, THE_WHOLE_TAIL, "base_or_early");

    // When
    let refusal = refusal_from(&workspace, tail).await;

    // Then
    assert!(
        refusal.starts_with(
            "this seam cannot be cut here: the range returns early from the function around it, \
             on line 6 (`return Ok(1);`) and line 8 (`return Ok(base);`)."
        ),
        "the refusal does not name both returns:\n{refusal}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn extracts_a_tail_that_propagates_an_error_and_returns_early() {
    // Given
    let workspace = a_crate_whose_function_propagates_and_returns_early();
    let tail = an_extract_method_of(&workspace, ORIGIN_LIB, THE_WHOLE_TAIL, "parsed_capped");

    // When
    performing(&workspace, tail).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn parsed_capped(text: &str) -> Result<u32, std::num::ParseIntError> {"),
        "the new function does not return the caller's type:\n{lib}"
    );
    assert_compiles(&workspace);
}
