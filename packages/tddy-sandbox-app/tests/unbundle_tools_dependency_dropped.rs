//! `#unbundle` node 5's headline outcome, asserted for this crate.
//!
//! This crate carried a **dev-dependency on `tddy-tools`**, and it existed only for
//! `session_tool_client::{dispatch_via_sandbox_ipc, dispatch_session_tool}` and
//! `session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`. The two client functions moved to the new
//! **`tddy-session-tool-client`** crate — `tddy-service` could not hold them, because
//! `tddy-livekit` already depends on it and the LiveKit transport arm would close a Cargo cycle —
//! and the constant moved to `tddy_service::session_agents`. Between them the three surfaces leave
//! the dependency with nothing to justify it.
//!
//! Asserted against the manifest because a dropped dependency is exactly the kind of claim that
//! reads as done in a changeset while the edge quietly survives — and a dev-dependency survives
//! invisibly, since nothing fails to compile when it is merely unused.
//!
//! ⚠ **This reads the manifest as raw text, not as parsed dependency tables**, because none of the
//! three crates asserting this has a TOML parser in `[dev-dependencies]` and node 5 adds no
//! dependency to gain one. So the assertion is stricter than it means to be: the string
//! `tddy-tools` must not appear anywhere in this crate's `Cargo.toml`, **including inside a
//! comment**. A comment that merely mentions the crate by name will fail this test; say "the tool
//! CLI" or reword rather than weakening the assertion.

#[test]
fn no_longer_depends_on_tddy_tools() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("this crate's Cargo.toml is readable");

    assert!(
        !manifest.contains("tddy-tools"),
        "the tddy-tools dev-dependency existed only for two client functions that moved to \
         tddy-session-tool-client and one constant that moved to tddy-service in node 5; \
         it has nothing left to justify it"
    );
}
