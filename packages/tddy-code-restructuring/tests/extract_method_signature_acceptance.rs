//! The signature `extract_method` writes, against a live rust-analyzer: defect E2.
//!
//! rust-analyzer writes an extracted function's parameter types from its own inference. For a type
//! it does not know yet it writes `_`, and `_` is illegal in an item signature (`E0121`). The
//! destructure plans met this for `StartSessionRequest`, which `tonic-build` generates into
//! `OUT_DIR`. The fixture here generates its request type the same way, from a build script that
//! takes a code generator's time to run.
//!
//! Handed over as `tddy-tools restructure` hands it over cold: the defect is in how long the
//! engine waits, so waiting for it in the harness would hide it.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_function_reads_a_borrowed_view,
    a_crate_whose_request_type_a_slow_build_script_generates, an_extract_method_of,
    assert_compiles, performing, ORIGIN_LIB,
};

/// `let session = req.session_id * 2; let resumed = …;`: statements reading the generated request.
const STATEMENTS_READING_THE_REQUEST: RangeInclusive<u32> = 10..=11;

/// `let above = roster.levels…; above + floor`: the tail that reads the borrowed view.
const THE_TAIL_READING_THE_VIEW: RangeInclusive<u32> = 14..=15;

/// An elided lifetime is a type rust-analyzer inferred, not one it left as `_`.
///
/// The port-move pilot's cold `check --deep` was refused with `state: AgentRosterState<'_>` in the
/// signature, read as an untyped placeholder (`E0121`) with advice to retry against a warm server.
#[tokio::test(flavor = "multi_thread")]
async fn names_a_parameter_whose_type_borrows_through_an_elided_lifetime() {
    // Given
    let workspace = a_crate_whose_function_reads_a_borrowed_view();
    let tail = an_extract_method_of(
        &workspace,
        ORIGIN_LIB,
        THE_TAIL_READING_THE_VIEW,
        "count_above",
    );

    // When
    performing(&workspace, tail).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn count_above(floor: u32, roster: Roster<'_>) -> u32 {"),
        "the extracted signature does not borrow the view through `'_`:\n{lib}"
    );
    assert_compiles(&workspace);
}

/// The request type is named in the signature, not left as `_`.
///
/// rust-analyzer answers hover before it has loaded the build script's output. The engine takes
/// that answer as readiness, asks for the extraction, and gets `fn resumed_session(req: _)`. The
/// placeholder post-condition then refuses it, with advice to retry against a warm server. The type
/// is only a build script away, so the engine should wait for the server to load it.
#[tokio::test(flavor = "multi_thread")]
async fn names_a_parameter_whose_type_a_build_script_generates() {
    // Given
    let workspace = a_crate_whose_request_type_a_slow_build_script_generates();
    let statements = an_extract_method_of(
        &workspace,
        ORIGIN_LIB,
        STATEMENTS_READING_THE_REQUEST,
        "resumed_session",
    );

    // When
    performing(&workspace, statements).await;

    // Then
    let lib = workspace.read(ORIGIN_LIB);
    assert!(
        lib.contains("fn resumed_session(req: StartSessionRequest) -> u32 {"),
        "the extracted signature does not name the generated request type:\n{lib}"
    );
    assert_compiles(&workspace);
}
