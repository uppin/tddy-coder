//! Where this crate's test binaries live, and which of them belong here.
//!
//! `tddy-daemon` is the daemon's **composition root**: `runtime.rs` wires ~20 services together and
//! `main.rs` starts them. A test that mounts that composition belongs here. A test that reaches a
//! module this crate merely **re-exports** does not — and 119 of 141 did, because
//! `src/lib.rs` carried a facade over 82 `tddy-session-lifecycle` modules whose stated purpose was
//! *"Legacy paths for integration suites"*.
//!
//! The counts moved while this node was planned and are stated here as measured, not as first
//! written: the crate gained two suites, and four the plan had listed as strays are named in
//! [`BELONGS_HERE`] instead, because each is about `tddy-daemon` itself.
//!
//! These assertions are what keeps the tree honest once they have moved.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn test_binaries_of(crate_name: &str) -> BTreeSet<String> {
    let dir = package(crate_name).join("tests");
    match std::fs::read_dir(&dir) {
        Err(_) => BTreeSet::new(),
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".rs"))
            .collect(),
    }
}

/// The suites that genuinely exercise this crate's own production modules — `runtime`, `server`,
/// `startup`, `daemon_settings`, `daemon_config_service`, `local_socket_server`, `relay_idle`,
/// `index_daemon` — plus this file.
///
/// Membership is not a matter of taste. A suite belongs here when it names a module this crate
/// **defines**, or when it asserts about this package's own `src/` or `Cargo.toml` — the second
/// kind cannot move at all, because `CARGO_MANIFEST_DIR` would then name whichever crate it landed
/// in and the assertion would silently be about something else. `tddy-workflow-recipes`'
/// `proto_workflow_contracts.rs` is the same shape and stays put for the same reason.
const BELONGS_HERE: [&str; 21] = [
    "session_agent_remote_acceptance.rs",
    "remote_managed_worktree_cross_host_acceptance.rs",
    "split_session_resume_acceptance.rs",
    "session_attach_cross_host_acceptance.rs",
    "local_token_uds.rs",
    "session_room_cross_host_acceptance.rs",
    "daemon_config_service.rs",
    "multi_host_acceptance.rs",
    "staging_forwarding_acceptance.rs",
    "relay_e2e_acceptance.rs",
    "server_options_acceptance.rs",
    "relay_runtime_acceptance.rs",
    "service_registration_acceptance.rs",
    "livekit_service_registration_acceptance.rs",
    "embedded_runtime.rs",
    "relay_idle_wired_acceptance.rs",
    "relay_idle_shutdown_acceptance.rs",
    // Names `tddy_daemon::index_daemon`, which this crate defines.
    "index_daemon_lifecycle_acceptance.rs",
    // Read this package's own `src/`, so they are about `tddy-daemon` wherever they sit.
    "local_socket_reachability_acceptance.rs",
    "unbundle_endpoint.rs",
    // Reads this package's own `Cargo.toml`.
    "unbundle_tools_dependency_dropped.rs",
];

/// AC5 — only the suites that exercise this crate remain.
#[test]
fn only_suites_that_exercise_this_crate_remain_here() {
    // Given what is in `tests/` now
    let present = test_binaries_of("tddy-daemon");
    let expected: BTreeSet<String> = BELONGS_HERE
        .into_iter()
        .map(str::to_string)
        .chain(["test_placement.rs".to_string()])
        .collect();

    // When the strays are identified
    let strays: Vec<&String> = present.difference(&expected).collect();

    // Then none is left
    assert!(
        strays.is_empty(),
        "{} suites still here that do not exercise this crate: {:?}",
        strays.len(),
        strays
    );
}

/// AC6 — the facade whose only purpose was those suites is gone.
///
/// It re-exported 82 modules under the comment "Legacy paths for integration suites". With the
/// suites in their own crates, every one of those lines is dead.
#[test]
fn the_legacy_facade_is_gone() {
    // Given this crate's module root
    let lib = std::fs::read_to_string(package("tddy-daemon").join("src/lib.rs"))
        .expect("tddy-daemon/src/lib.rs");

    // Then it re-exports nothing from the crate it was a facade over
    assert!(
        !lib.contains("pub use tddy_session_lifecycle::"),
        "`src/lib.rs` still re-exports `tddy-session-lifecycle` for suites that have moved"
    );

    // And the four one-line shims are gone with it
    for shim in [
        "src/config.rs",
        "src/tddy_user_config.rs",
        "src/user_sessions_path.rs",
        "src/relay_idle.rs",
    ] {
        let path = package("tddy-daemon").join(shim);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !text.contains("pub use tddy_session_lifecycle::"),
            "`{shim}` is still a re-export shim for a moved module"
        );
    }
}

/// AC7 — the crate that owns 97 of those suites finally has them.
///
/// `tddy-session-lifecycle` had **no `tests/` directory at all**, which is why it looked untested
/// and could not be verified on its own.
#[test]
fn the_session_lifecycle_crate_has_its_own_suites() {
    // Given its tests directory
    let suites = test_binaries_of("tddy-session-lifecycle");

    // Then it is not empty
    assert!(
        !suites.is_empty(),
        "`tddy-session-lifecycle` still has no `tests/` directory of its own"
    );
}

/// Every `.rs` file under `src/`, concatenated — **including the ones in subdirectories**.
///
/// A flat `read_dir` reads `src/*.rs` only, which is not where all of this crate's production code
/// lives: `src/index_daemon/registry.rs` names `tddy_sandbox_runner` and `tddy_daemon_sandbox`, so
/// a flat scan reports two genuine runtime dependencies as unused and this assertion would have
/// them deleted out of a crate that calls them.
fn every_source_file_under(dir: &Path) -> String {
    let mut source = String::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return source;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            source.push_str(&every_source_file_under(&path));
        } else {
            source.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        }
    }
    source
}

/// AC8 — no crate declares as a runtime dependency something only its tests name.
///
/// Sixteen `tddy-*` entries sat in `[dependencies]` while no file in `src/` named them, so every
/// consumer of this crate rebuilt them. With the suites gone they are unreferenced entirely.
#[test]
fn no_runtime_dependency_is_named_only_by_tests() {
    // Given the manifest and every source file
    let manifest = std::fs::read_to_string(package("tddy-daemon").join("Cargo.toml"))
        .expect("tddy-daemon/Cargo.toml");
    let runtime: Vec<String> = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("tddy-"))
        .filter_map(|l| l.split(&[' ', '='][..]).next())
        .map(|n| format!("tddy_{}", n.replace('-', "_")))
        .collect();

    let source = every_source_file_under(&package("tddy-daemon").join("src"));

    // When each runtime dependency is checked against what the source names
    let unused: Vec<&String> = runtime.iter().filter(|c| !source.contains(*c)).collect();

    // Then every one of them is reached from production code
    assert!(
        unused.is_empty(),
        "{} runtime dependencies are named by no file in `src/`, so every consumer rebuilds them: {:?}",
        unused.len(),
        unused
    );
}
