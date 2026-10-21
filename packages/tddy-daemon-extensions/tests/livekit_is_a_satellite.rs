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
//!
//! A crate that ships a library *and* a binary cannot name a LiveKit crate for its binary alone —
//! Cargo has no binary-only dependency, so the edge appears on the crate. A client binary is
//! therefore built by a wiring crate of its own ([`CLIENT_BINARIES`]), and its library
//! ([`CLIENT_LIBRARIES`]) names LiveKit nowhere. There is no exception for a library that builds a
//! binary: the `#satellite` stack split the last three such crates.

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
    "tddy-remote-git-repo-cli",
    "tddy-session-sync-cli",
    "tddy-tools-cli",
    "tddy-rdp",
    "tddy-vnc",
    "tddy-e2e",
];

/// Libraries a wiring crate builds a client binary from. They join rooms through
/// `tddy-room-client` and name LiveKit nowhere — not in their manifest, not in any source file.
const CLIENT_LIBRARIES: &[&str] = &["tddy-remote-git-repo", "tddy-session-sync", "tddy-tools"];

/// Each client binary, and the wiring crate that builds it. The binary keeps its name, so every
/// script, config and test that resolves it by name is unaffected by which crate builds it.
const CLIENT_BINARIES: &[(&str, &str)] = &[
    ("tddy-remote-git-repo", "tddy-remote-git-repo-cli"),
    ("tddy-session-sync", "tddy-session-sync-cli"),
    ("tddy-tools", "tddy-tools-cli"),
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

/// The `src/` directory of workspace crate `krate`.
fn source_dir(krate: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src")
}

/// Whether `file` contains a path into a LiveKit crate.
fn names_a_livekit_path(file: &Path) -> bool {
    let source = std::fs::read_to_string(file).expect("a source file is readable");
    LIVEKIT_PATHS.iter().any(|path| source.contains(path))
}

#[test]
fn a_client_library_names_livekit_nowhere_in_its_sources() {
    // Given the libraries a wiring crate builds a client binary from
    // When every source file under their `src/`, a `main.rs` included, is read for a path into LiveKit
    let naming_livekit: Vec<String> = CLIENT_LIBRARIES
        .iter()
        .flat_map(|krate| rust_files_under(&source_dir(krate)))
        .filter(|file| names_a_livekit_path(file))
        .map(|file| file.display().to_string())
        .collect();

    // Then none does: the binding to LiveKit is the wiring crate's alone
    assert_eq!(naming_livekit, Vec::<String>::new());
}

#[test]
fn each_client_binary_is_built_by_its_wiring_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When the crates building each client binary are read
    let built_by: Vec<(&str, BTreeSet<String>)> = CLIENT_BINARIES
        .iter()
        .map(|(binary, _)| (*binary, graph.crates_building(binary)))
        .collect();

    // Then each is built by its wiring crate and by nothing else — its library builds no binary
    let expected: Vec<(&str, BTreeSet<String>)> = CLIENT_BINARIES
        .iter()
        .map(|(binary, wiring)| (*binary, set_of(&[*wiring])))
        .collect();
    assert_eq!(built_by, expected);
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
