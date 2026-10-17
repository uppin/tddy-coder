//! LiveKit is a satellite: only its own crates and the crates that build binaries may depend on it.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md`. The core reaches
//! LiveKit through the extension ports in this crate; LiveKit's crates implement them; a binary's
//! wiring brings the two together. Every other crate that names a LiveKit crate today is listed in
//! [`KNOWN_OFFENDERS`], and the list may only shrink:
//!
//! - a crate that starts naming a LiveKit crate and is not on a list fails
//!   [`only_plugins_wiring_and_known_offenders_name_a_livekit_crate`];
//! - a known offender that stops naming one and is not struck off fails
//!   [`every_known_offender_still_names_a_livekit_crate`], so the list always says what the tree
//!   does.
//!
//! The stack that decouples LiveKit strikes entries off as it moves each cluster behind a port;
//! its last node deletes the list.
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
const WIRING: &[&str] = &["tddy-daemon", "tddy-rdp", "tddy-vnc", "tddy-e2e"];

/// Library crates that still name a LiveKit crate. Strike a crate off when it stops.
const KNOWN_OFFENDERS: &[&str] = &[
    "tddy-agent-launch",
    "tddy-cli-sessions",
    "tddy-coder",
    "tddy-daemon-auth",
    "tddy-daemon-rpc",
    "tddy-remote-git-repo",
    "tddy-screen-sharing",
    "tddy-session-agents",
    "tddy-session-files",
    "tddy-session-lifecycle",
    "tddy-session-split",
    "tddy-session-sync",
    "tddy-session-tool-client",
    "tddy-terminal-rpc",
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
fn only_plugins_wiring_and_known_offenders_name_a_livekit_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();
    let allowed: BTreeSet<String> = set_of(PLUGINS)
        .into_iter()
        .chain(set_of(WIRING))
        .chain(set_of(KNOWN_OFFENDERS))
        .collect();

    // When every crate naming a LiveKit crate is collected
    let naming = crates_naming_livekit(&graph);

    // Then each one is on a list
    assert_eq!(
        naming.difference(&allowed).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "these crates name a LiveKit crate: reach it through tddy-daemon-extensions instead"
    );
}

#[test]
fn every_known_offender_still_names_a_livekit_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When the known offenders are checked against it
    let naming = crates_naming_livekit(&graph);

    // Then each still does — one that stopped must be struck off, so the list only shrinks
    assert_eq!(
        set_of(KNOWN_OFFENDERS)
            .difference(&naming)
            .collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "these crates no longer name a LiveKit crate: strike them from KNOWN_OFFENDERS"
    );
}
