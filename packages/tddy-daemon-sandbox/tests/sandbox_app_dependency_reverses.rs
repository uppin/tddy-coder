//! `#unbundle` node 3's most checkable outcome: `tddy-sandbox-app`'s dependency **reverses**.
//!
//! It consumed `tddy_daemon::{sandbox_session, tool_engine}`; afterwards it depends on
//! `tddy-daemon-sandbox` (and `tddy-tool-engine`) and not on `tddy-daemon` at all.
//!
//! Asserted rather than described, because a dependency reversal is exactly the kind of claim that
//! reads as done in a changeset while the edge quietly survives.
//!
//! # Why the resolved graph, and not the manifest text
//!
//! The obvious test greps `Cargo.toml` for `"tddy-daemon ="`. It is the weakest check available and
//! this node has already been bitten by the gap: M2 moved `tddy-supervisor` out of
//! `[dependencies]`, and it survived in `[dev-dependencies]` — a text search for the removed form
//! would have called that a clean reversal. A substring search also cannot tell a dependency from a
//! **comment**, and this very manifest carries a comment naming `tddy-daemon`.
//!
//! So the subject is `cargo metadata`'s resolved dependency list, which names every edge in one
//! shape regardless of how the manifest spells it — `tddy-daemon = { … }`, `tddy-daemon.path`,
//! `tddy-daemon.workspace`, a `[target.'cfg(…)'.dependencies]` table, or a dev-dependency.

use std::process::Command;

/// Every dependency edge `cargo` resolves for `crate_name`, whatever table it was declared in.
///
/// Returns `(dependency name, kind)` where kind is `"normal"`, `"dev"` or `"build"` — cargo reports
/// a normal dependency's kind as JSON `null`, which is exactly the distinction M2's
/// `tddy-supervisor` case turned on.
fn resolved_dependencies(crate_name: &str) -> Vec<(String, String)> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON");

    let package = meta["packages"]
        .as_array()
        .expect("metadata lists packages")
        .iter()
        .find(|p| p["name"] == crate_name)
        .unwrap_or_else(|| panic!("{crate_name} is a workspace member"));

    package["dependencies"]
        .as_array()
        .expect("a package lists its dependencies")
        .iter()
        .map(|d| {
            let name = d["name"].as_str().unwrap_or_default().to_string();
            // A normal dependency's kind is null; dev and build name themselves.
            let kind = d["kind"].as_str().unwrap_or("normal").to_string();
            (name, kind)
        })
        .collect()
}

fn depends_on(crate_name: &str, dependency: &str) -> bool {
    resolved_dependencies(crate_name)
        .iter()
        .any(|(name, _)| name == dependency)
}

#[test]
fn the_sandbox_app_reaches_the_sandbox_through_this_crate() {
    // Given the app that runs a jailed session

    // When its resolved dependency graph is read
    let reaches_the_sandbox_crate = depends_on("tddy-sandbox-app", "tddy-daemon-sandbox");

    // Then
    assert!(
        reaches_the_sandbox_crate,
        "tddy-sandbox-app must reach the sandbox through tddy-daemon-sandbox"
    );
}

/// The reversal is only real if the edge is gone from **every** table. M2 proved the failure mode:
/// a dependency moved out of `[dependencies]` and lived on under `[dev-dependencies]`.
#[test]
fn the_sandbox_app_no_longer_depends_on_the_daemon_in_any_dependency_table() {
    // Given the app's resolved dependencies, of every kind
    let dependencies = resolved_dependencies("tddy-sandbox-app");

    // When the daemon is looked for among them
    let surviving_daemon_edges: Vec<_> = dependencies
        .iter()
        .filter(|(name, _)| name == "tddy-daemon")
        .collect();

    // Then
    assert!(
        surviving_daemon_edges.is_empty(),
        "tddy-sandbox-app must not depend on tddy-daemon once the sandbox has its own crate; \
         found: {surviving_daemon_edges:?}"
    );
}

/// `tool_catalog_sync.rs` was a source file whose entire body was one `#[cfg(test)] mod tests`,
/// declared in the daemon's `lib.rs`. What was wrong was its location, so it was relocated rather
/// than deleted — the guard it carries is what node 8 relies on when it collapses the last
/// hand-copied catalog.
#[test]
fn the_catalog_guard_no_longer_lives_in_the_daemons_src_tree() {
    // Given the daemon's source tree
    let daemon_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-daemon/src");

    // Then
    assert!(
        !daemon_src.join("tool_catalog_sync.rs").exists(),
        "packages/tddy-daemon/src/tool_catalog_sync.rs is a test module living in src/"
    );
}
