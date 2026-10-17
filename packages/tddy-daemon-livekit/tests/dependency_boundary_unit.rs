//! The check that proves the extraction is real rather than a re-export.
//!
//! `tddy-daemon-livekit` exists so the daemon stops owning the LiveKit subsystem. If `tddy-daemon`
//! were still on its dependency path, every symbol it "left behind" would still be compiled into
//! it, the crate split would be a directory rename, and the three module cycles the split exists to
//! cut would be intact — while the changeset read as done. Cargo would not complain: a crate may
//! depend on a crate that depends on nothing of it.
//!
//! Asserted against the workspace graph Cargo resolves rather than described in prose, and
//! *transitively*, because a direct edge is the one shape a reviewer would notice anyway.

use tddy_testing_commons::dependency_graph::WorkspaceGraph;

/// The crate no crate in this graph may reach.
const THE_GOD_CRATE: &str = "tddy-daemon";

/// This crate, as its package is named.
const THIS_CRATE: &str = "tddy-daemon-livekit";

#[test]
fn does_not_declare_the_daemon_as_a_dependency_of_its_own() {
    // Given the workspace as Cargo resolves it, whichever way a manifest spells a dependency
    let graph = WorkspaceGraph::load();

    // When every crate this one names — normal, build and optional — is read
    let declared = graph.names_directly(THIS_CRATE);

    // Then the daemon is not among them
    assert!(
        !declared.contains(THE_GOD_CRATE),
        "{THIS_CRATE} must not depend on {THE_GOD_CRATE} — a crate that does has not left it. \
         It declares: {declared:?}"
    );
}

#[test]
fn has_no_path_from_itself_to_the_daemon_through_any_workspace_crate() {
    // Given the whole build-time dependency closure of this crate
    let reachable = WorkspaceGraph::load().reaches(THIS_CRATE);

    // When the daemon is looked for anywhere in it
    let reaches_the_daemon = reachable.contains(THE_GOD_CRATE);

    // Then it is absent — no intermediate crate re-introduces the edge
    assert!(
        !reaches_the_daemon,
        "{THE_GOD_CRATE} is on {THIS_CRATE}'s dependency path; it is reachable through \
         {reachable:?}"
    );
}

#[test]
fn reaches_the_kernel_it_takes_its_config_and_identities_from() {
    // Given the same closure
    let reachable = WorkspaceGraph::load().reaches(THIS_CRATE);

    // When the crate publishing `DaemonConfig` and the daemon ids is looked for
    // Then it is there — otherwise the walk above proves nothing, because it walked nothing
    assert!(
        reachable.contains("tddy-daemon-kernel"),
        "{THIS_CRATE} takes `DaemonConfig`, both daemon ids and the split-agent identity prefix \
         from tddy-daemon-kernel, so the dependency walk must find it; it found {reachable:?}"
    );
}

/// `config.livekit.api_secret` signs both LiveKit room JWTs and session tokens, and this crate
/// deliberately does not reach for it: [`SessionTokenMinter`] is a port, so the credential is
/// handed in by whoever wired the room up. Depending on the identity boundary would make a second
/// signer here a one-line mistake rather than an impossible one.
///
/// [`SessionTokenMinter`]: tddy_daemon_livekit::session_room::SessionTokenMinter
#[test]
fn does_not_reach_the_identity_boundary_it_mints_no_tokens_of_its_own() {
    // Given the same closure
    let reachable = WorkspaceGraph::load().reaches(THIS_CRATE);

    // Then the crate that signs is absent from it
    assert!(
        !reachable.contains("tddy-daemon-auth"),
        "{THIS_CRATE} must mint nothing: a room token arrives through the SessionTokenMinter port, \
         so tddy-daemon-auth has no business on this path. It reaches {reachable:?}"
    );
}
