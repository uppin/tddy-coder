//! `#unbundle` node 3's most checkable outcome: `tddy-sandbox-app`'s dependency **reverses**.
//!
//! It consumes `tddy_daemon::{sandbox_session, claude_cli_session, tool_engine}` today. Afterwards
//! it depends on `tddy-daemon-sandbox` and not on `tddy-daemon` for the sandbox path.
//!
//! Asserted against the manifest rather than described in prose, because a dependency reversal is
//! exactly the kind of claim that reads as done in a changeset while the edge quietly survives.

use std::path::Path;

fn manifest(crate_name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("Cargo.toml");
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{crate_name}/Cargo.toml is readable"))
}

#[test]
fn the_sandbox_app_depends_on_this_crate() {
    assert!(
        manifest("tddy-sandbox-app").contains("tddy-daemon-sandbox"),
        "tddy-sandbox-app must reach the sandbox through tddy-daemon-sandbox"
    );
}

#[test]
fn the_sandbox_app_no_longer_depends_on_the_daemon() {
    assert!(
        !manifest("tddy-sandbox-app").contains("tddy-daemon ="),
        "tddy-sandbox-app must not depend on tddy-daemon once the sandbox has its own crate"
    );
}

/// `tool_catalog_sync.rs` is a source file whose entire body is one `#[cfg(test)] mod tests` with a
/// single test, declared in the daemon's `lib.rs`. It is relocated here rather than deleted: the
/// assertion it makes — that the workspace exec tool names match the tool catalog — is the guard
/// node 8 relies on when it collapses the last hand-copied catalog. What was wrong was its location.
#[test]
fn the_catalog_guard_test_is_a_test_file_not_a_source_file() {
    let daemon_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-daemon/src");
    assert!(
        !daemon_src.join("tool_catalog_sync.rs").exists(),
        "packages/tddy-daemon/src/tool_catalog_sync.rs is a test module living in src/"
    );
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/tool_catalog_sync.rs")
            .exists(),
        "the catalog guard must survive the move — node 8 relies on it"
    );
}
