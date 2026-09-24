//! An index rust-analyzer itself reports as degraded is refused, against a live server.
//!
//! rust-analyzer finishes loading a workspace whose build script failed, says `quiescent: true`,
//! and from then on answers as though the generated code did not exist. The only account of that
//! is the `health` and `message` of its `experimental/serverStatus`. An operation that went ahead
//! would write `req: _` or miss an import and report success; these suites pin the refusal that
//! replaces that silent success, on the cold path and on a warm server's later requests.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use std::ops::RangeInclusive;

use harness::{
    a_crate_whose_build_script_fails, an_extract_method_of, refusal_from,
    refusal_from_a_server_served_before, ORIGIN_LIB,
};

/// `let twice = …; let settled = …;`: statements that need nothing the failed build script makes.
const STATEMENTS_NEEDING_NO_BUILD_SCRIPT: RangeInclusive<u32> = 4..=5;

/// Handed over as `tddy-tools restructure` hands it over cold.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_operation_against_an_index_whose_build_script_failed() {
    // Given
    let workspace = a_crate_whose_build_script_fails();
    let statements = an_extract_method_of(
        &workspace,
        ORIGIN_LIB,
        STATEMENTS_NEEDING_NO_BUILD_SCRIPT,
        "settled_from",
    );

    // When
    let refusal = refusal_from(&workspace, statements).await;

    // Then
    assert!(
        refusal.starts_with(
            "rust-analyzer's answer was unusable: rust-analyzer reports its index as degraded \
             (health `warning`): \"Failed to run build scripts of some packages."
        ),
        "the refusal does not quote the server's own account of its degraded index:\n{refusal}"
    );
}

/// Handed over as `tddy-index-daemon` hands over its second request on a warm root: the server
/// reported its health once, to whoever read it first, and will not say it again.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_an_operation_against_a_warm_index_whose_build_script_failed() {
    // Given
    let workspace = a_crate_whose_build_script_fails();
    let statements = an_extract_method_of(
        &workspace,
        ORIGIN_LIB,
        STATEMENTS_NEEDING_NO_BUILD_SCRIPT,
        "settled_from",
    );

    // When
    let refusal = refusal_from_a_server_served_before(&workspace, statements).await;

    // Then
    assert!(
        refusal.starts_with(
            "rust-analyzer's answer was unusable: rust-analyzer reports its index as degraded \
             (health `warning`): \"Failed to run build scripts of some packages."
        ),
        "the refusal does not quote the server's own account of its degraded index:\n{refusal}"
    );
}
