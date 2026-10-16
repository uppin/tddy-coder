//! LiveKit is a satellite: only its own crates and the crates that build binaries may depend on it.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md`. The core reaches
//! LiveKit through the extension ports in this crate; LiveKit's crates implement them; a binary's
//! wiring brings the two together. The `#satellite` stack shrank a list of known offenders node by
//! node until it was empty; what is left is the rule itself.
//!
//! "Names" is a `[dependencies]` or `[build-dependencies]` entry, optional ones included — an
//! optional dependency is one the crate can be built with. `[dev-dependencies]` are not counted: a
//! test may stand up a real LiveKit server.

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use tddy_testing_commons::dependency_graph::WorkspaceGraph;

/// The crates that are LiveKit: the SDK and every workspace crate built on it as an integration.
const LIVEKIT_FAMILY: &[&str] = &[
    "livekit",
    "livekit-api",
    "livekit-protocol",
    "tddy-livekit",
    "tddy-livekit-testkit",
    "tddy-daemon-livekit",
    "tddy-livekit-screen-capture",
    "tddy-screenshare",
];

/// LiveKit's own workspace crates — the plugin. They implement the ports.
const PLUGINS: &[&str] = &[
    "tddy-livekit",
    "tddy-livekit-testkit",
    "tddy-daemon-livekit",
    "tddy-livekit-screen-capture",
    "tddy-screenshare",
];

/// Crates that build a binary and do nothing but wire it, plus the end-to-end harness that stands
/// a whole deployment up.
const WIRING: &[&str] = &[
    "tddy-daemon",
    "tddy-coder-cli",
    "tddy-rdp",
    "tddy-vnc",
    "tddy-e2e",
];


fn set_of(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Every workspace crate that names at least one LiveKit crate.
fn crates_naming_livekit(graph: &WorkspaceGraph) -> BTreeSet<String> {
    let family = set_of(LIVEKIT_FAMILY);
    graph
        .workspace_crates()
        .into_iter()
        .filter(|krate| !graph.names_directly(krate).is_disjoint(&family))
        .collect()
}

#[test]
fn the_extension_ports_reach_no_livekit_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything this crate can be built with is followed
    let reached = graph.reaches("tddy-daemon-extensions");

    // Then none of it is LiveKit: a core crate that depends on the ports gains no LiveKit
    assert_eq!(
        reached.intersection(&set_of(LIVEKIT_FAMILY)).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[test]
fn only_plugins_and_wiring_name_a_livekit_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();
    let allowed: BTreeSet<String> = set_of(PLUGINS).into_iter().chain(set_of(WIRING)).collect();

    // When every crate naming a LiveKit crate is collected
    let naming = crates_naming_livekit(&graph);

    // Then each one is LiveKit itself, or a binary's wiring
    assert_eq!(
        naming.difference(&allowed).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "these crates name a LiveKit crate: reach it through tddy-daemon-extensions instead"
    );
}

#[test]
fn the_daemon_rpc_and_session_lifecycle_crates_do_not_name_the_livekit_sdk() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When the two crates whose `livekit` edge no source uses are read
    let naming_the_sdk: Vec<&str> = ["tddy-daemon-rpc", "tddy-session-lifecycle"]
        .into_iter()
        .filter(|krate| graph.names_directly(krate).contains("livekit"))
        .collect();

    // Then neither names the SDK itself
    assert_eq!(naming_the_sdk, Vec::<&str>::new());
}
