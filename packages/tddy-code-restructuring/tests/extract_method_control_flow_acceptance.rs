//! An `extract_method` over a range that returns early from its caller, against a live server.
//!
//! rust-analyzer's "extract into function" copies a `return` in the range verbatim into the new
//! function, whose return type is not the caller's. The destructure's plan 10 applied six such
//! extractions, reported "applied 6 of 6", and left seven `E0308`s behind. The engine now refuses
//! the range before asking the server anything.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_crate_whose_function_returns_early, an_extract_method_of, refusal_from, ORIGIN_LIB,
};

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_range_that_returns_early_from_the_enclosing_function() {
    // Given
    let workspace = a_crate_whose_function_returns_early();
    let statements = an_extract_method_of(&workspace, ORIGIN_LIB, 4..=7, "base_or_early");

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
