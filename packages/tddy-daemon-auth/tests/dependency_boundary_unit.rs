//! The check that proves the extraction is real rather than a re-export.
//!
//! `tddy-daemon-auth` exists to be the daemon's identity boundary. If `tddy-daemon` were still on
//! its dependency path, every symbol it "left behind" would still be compiled into it, the crate
//! split would be a directory rename, and the module cycle the split exists to cut would be intact
//! — while the changeset read as done. Cargo would not complain: a crate may depend on a crate
//! that depends on nothing of it.
//!
//! Asserted against the workspace graph Cargo resolves rather than described in prose, and
//! *transitively*, because a direct edge is the one shape a reviewer would notice anyway.

use tddy_testing_commons::dependency_graph::WorkspaceGraph;

/// The crate no crate in this graph may reach.
const THE_GOD_CRATE: &str = "tddy-daemon";

/// This crate, as its package is named.
const THIS_CRATE: &str = "tddy-daemon-auth";

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
fn reaches_the_kernel_it_takes_its_session_resolver_from() {
    // Given the same closure
    let reachable = WorkspaceGraph::load().reaches(THIS_CRATE);

    // When the crate publishing `SessionUserResolver` is looked for
    // Then it is there — otherwise the walk above proves nothing, because it walked nothing
    assert!(
        reachable.contains("tddy-daemon-kernel"),
        "{THIS_CRATE} resolves a session token's login through tddy-daemon-kernel, so the \
         dependency walk must find it; it found {reachable:?}"
    );
}
