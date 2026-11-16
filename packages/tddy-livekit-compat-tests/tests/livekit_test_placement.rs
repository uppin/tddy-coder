//! Where LiveKit test suites live, and which crates may still pay for a real room.
//!
//! The rule (see `docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`):
//! a crate that owns product logic tests that logic over a cheap transport and never names
//! `tddy-livekit-testkit`; suites whose job is to prove a LiveKit adapter connects and honours the
//! RPC contract live here, in `tddy-livekit-compat-tests`, which is the one package whose
//! `[dev-dependencies]` may name the testkit.
//!
//! The logic suites have not all been converted yet. [`REMAINING_LOGIC_SUITES`] names the ones
//! still running over a real room, in the crate that owns them, so this test passes today and the
//! list can only shrink: converting a suite means deleting its line here.

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

/// Contract suites that live here, and the crate each one left.
const MOVED_HERE: [(&str, &str); 24] = [
    // The LiveKit RPC transport itself (`tddy-livekit`).
    ("tddy-livekit", "acp_session_livekit.rs"),
    ("tddy-livekit", "broadcast_and_room_metadata.rs"),
    ("tddy-livekit", "participant_metadata_acceptance.rs"),
    ("tddy-livekit", "room_connector.rs"),
    ("tddy-livekit", "room_roster_livekit.rs"),
    ("tddy-livekit", "rpc_client_factory.rs"),
    ("tddy-livekit", "rpc_scenarios.rs"),
    ("tddy-livekit", "rpc_three_participant_forward.rs"),
    ("tddy-livekit", "webrtc_runtime_bounded.rs"),
    // The daemon's LiveKit surface (`tddy-daemon-livekit`).
    (
        "tddy-daemon-livekit",
        "common_room_duplicate_identity_repro.rs",
    ),
    ("tddy-daemon-livekit", "common_room_integration_status.rs"),
    (
        "tddy-daemon-livekit",
        "common_room_rpc_participant_rejoin_repro.rs",
    ),
    (
        "tddy-daemon-livekit",
        "common_room_set_metadata_handshake_repro.rs",
    ),
    ("tddy-daemon-livekit", "forward_to_peer_shared_registry.rs"),
    (
        "tddy-daemon-livekit",
        "forwarded_rpc_is_stamped_by_the_receiver.rs",
    ),
    ("tddy-daemon-livekit", "livekit_integration_status.rs"),
    ("tddy-daemon-livekit", "livekit_peer_daemons_acceptance.rs"),
    ("tddy-daemon-livekit", "session_room_livekit_acceptance.rs"),
    // End-to-end LiveKit pipelines that named no owning crate (`tddy-e2e`).
    ("tddy-e2e", "livekit_terminal_rpc.rs"),
    ("tddy-e2e", "terminal_service_livekit.rs"),
    ("tddy-e2e", "token_generation_livekit.rs"),
    ("tddy-e2e", "token_service_livekit.rs"),
    // A spawned participant's serving identity (`tddy-coder-cli`).
    (
        "tddy-coder-cli",
        "coder_publishes_session_metadata_to_participant.rs",
    ),
    (
        "tddy-coder-cli",
        "coder_serves_connection_service_from_participant.rs",
    ),
];

/// Logic suites still running over a real LiveKit room, in the crate that owns the logic.
/// Converting one onto an in-memory, stdio or IPC transport deletes its line.
const REMAINING_LOGIC_SUITES: [(&str, &str); 10] = [
    ("tddy-daemon", "common_room_key_trust_acceptance.rs"),
    ("tddy-daemon", "first_login_enrolment_acceptance.rs"),
    (
        "tddy-daemon",
        "remote_managed_worktree_cross_host_acceptance.rs",
    ),
    ("tddy-daemon", "runtime_signing_identity_acceptance.rs"),
    ("tddy-daemon", "session_agent_remote_acceptance.rs"),
    ("tddy-daemon", "session_room_cross_host_acceptance.rs"),
    ("tddy-daemon", "split_session_resume_acceptance.rs"),
    ("tddy-session-lifecycle", "session_room_acceptance.rs"),
    (
        "tddy-session-lifecycle",
        "session_sync_livekit_acceptance.rs",
    ),
    ("tddy-worktree-service", "remote_git_livekit_acceptance.rs"),
];

#[test]
fn contract_suites_live_here_and_nowhere_else() {
    let here = test_binaries_of("tddy-livekit-compat-tests");
    for (origin, suite) in MOVED_HERE {
        assert!(
            here.contains(suite),
            "`{suite}` should live in `tddy-livekit-compat-tests`"
        );
        assert!(
            !test_binaries_of(origin).contains(suite),
            "`{suite}` still lives in `{origin}` as well as here"
        );
    }
}

#[test]
fn remaining_logic_suites_are_the_only_ones_still_paying_for_a_room() {
    for (owner, suite) in REMAINING_LOGIC_SUITES {
        assert!(
            test_binaries_of(owner).contains(suite),
            "`{suite}` left `{owner}` without a line being deleted here — convert it and delete the line"
        );
    }
}

/// Crates whose `[dev-dependencies]` name `tddy-livekit-testkit`, read from each manifest's
/// dev-dependencies table only (a crate that ships the testkit as an optional library dependency
/// for an example is a different shape, and names it in `[dependencies]`).
fn dev_dependency_holders() -> BTreeSet<String> {
    let packages_dir = package("tddy-livekit-compat-tests")
        .parent()
        .expect("the packages directory")
        .to_path_buf();
    let mut holders = BTreeSet::new();
    for entry in std::fs::read_dir(&packages_dir).expect("read the packages directory") {
        let entry = entry.expect("a packages entry");
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let mut in_dev_dependencies = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_dev_dependencies = line == "[dev-dependencies]";
            } else if in_dev_dependencies && line.starts_with("tddy-livekit-testkit") {
                holders.insert(
                    entry
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                        .trim_start_matches("packages/")
                        .to_string(),
                );
            }
        }
    }
    // Entries are package directories; the manifest's own package name is the identifier CI uses.
    holders
        .into_iter()
        .map(|dir| {
            let manifest = packages_dir.join(&dir).join("Cargo.toml");
            let text = std::fs::read_to_string(manifest).expect("read a manifest");
            text.lines()
                .find_map(|l| l.trim().strip_prefix("name = \""))
                .and_then(|rest| rest.strip_suffix('"'))
                .unwrap_or(&dir)
                .to_string()
        })
        .collect()
}

#[test]
fn only_this_package_and_the_logic_owners_still_converting_hold_the_testkit() {
    let expected: BTreeSet<String> = ["tddy-livekit-compat-tests"]
        .into_iter()
        .chain(REMAINING_LOGIC_SUITES.iter().map(|(owner, _)| *owner))
        .map(str::to_string)
        .collect();
    assert_eq!(
        dev_dependency_holders(),
        expected,
        "a crate gained or lost a `tddy-livekit-testkit` dev-dependency outside the named sets"
    );
}
