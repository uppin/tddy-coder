//! `#unbundle` node 5's headline outcome, asserted for this crate.
//!
//! This crate carried a **dev-dependency on `tddy-tools`**, and it existed only for
//! `session_tool_client::{dispatch_via_sandbox_ipc, dispatch_session_tool}` and
//! `session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`. All three moved to `tddy-service` in node 5,
//! so the dependency has nothing left to justify it.
//!
//! Asserted against the manifest because a dropped dependency is exactly the kind of claim that
//! reads as done in a changeset while the edge quietly survives — and a dev-dependency survives
//! invisibly, since nothing fails to compile when it is merely unused.

#[test]
fn no_longer_depends_on_tddy_tools() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("this crate's Cargo.toml is readable");

    assert!(
        !manifest.contains("tddy-tools"),
        "the tddy-tools dev-dependency existed only for surfaces that moved to tddy-service in          node 5; it has nothing left to justify it"
    );
}
