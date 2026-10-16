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
//!
//! A crate that ships a library *and* a binary cannot name a LiveKit crate for its binary alone —
//! Cargo has no binary-only dependency, so the edge appears on the crate. Those crates are listed in
//! [`WIRING_WITH_A_LIBRARY`], and [`a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary`]
//! holds their library sources clean, so the allowance stays what it claims: the binary's wiring.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

/// Crates that ship a library *and* a binary that wires LiveKit into it: the library joins rooms
/// through `tddy-room-client`, and only `src/main.rs` names the plugin.
const WIRING_WITH_A_LIBRARY: &[&str] = &["tddy-remote-git-repo", "tddy-session-sync", "tddy-tools"];

/// Library crates that still name a LiveKit crate. Strike a crate off when it stops.
const KNOWN_OFFENDERS: &[&str] = &[
    "tddy-agent-launch",
    "tddy-session-lifecycle",
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
fn the_extension_ports_reach_the_crates_they_are_built_on() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything this crate can be built with is followed
    let reached = graph.reaches("tddy-daemon-extensions");

    // Then the walk saw something: "reaches no LiveKit crate" below is not vacuous
    assert!(
        reached.contains("tddy-rpc"),
        "tddy-daemon-extensions reaches {reached:?}, which should include tddy-rpc"
    );
}

#[test]
fn the_extension_ports_reach_no_livekit_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything this crate can be built with is followed
    let reached = graph.reaches("tddy-daemon-extensions");

    // Then none of it is LiveKit: a core crate that depends on the ports gains no LiveKit
    assert_eq!(
        reached
            .intersection(&set_of(LIVEKIT_FAMILY))
            .collect::<Vec<_>>(),
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
        .chain(set_of(WIRING_WITH_A_LIBRARY))
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

/// What names a LiveKit crate in source: a path into one.
const LIVEKIT_PATHS: &[&str] = &["livekit::", "livekit_api::", "livekit_protocol::"];

/// Every `.rs` file under `dir`.
fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).expect("a crate's source directory is readable") {
        let path = entry.expect("a directory entry is readable").path();
        if path.is_dir() {
            files.extend(rust_files_under(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// The sources of `krate` that are not its binary's entry point.
fn library_sources(krate: &str) -> Vec<PathBuf> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src");
    rust_files_under(&src)
        .into_iter()
        .filter(|file| file != &src.join("main.rs"))
        .collect()
}

#[test]
fn a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary() {
    // Given the crates that ship a library and a binary
    // When their sources, apart from the binary's entry point, are read for a path into LiveKit
    let naming_livekit: Vec<String> = WIRING_WITH_A_LIBRARY
        .iter()
        .flat_map(|krate| library_sources(krate))
        .filter(|file| {
            let source = std::fs::read_to_string(file).expect("a source file is readable");
            LIVEKIT_PATHS.iter().any(|path| source.contains(path))
        })
        .map(|file| file.display().to_string())
        .collect();

    // Then none does: the library reaches a room through the client port
    assert_eq!(naming_livekit, Vec::<String>::new());
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
