//! Telegram is a satellite: only its own crates and the daemon's wiring may depend on it.
//!
//! PRD `docs/ft/daemon/telegram-notifications.md`. The core reaches an
//! operator's chat through `tddy_daemon_kernel::operator_channel::OperatorChannel`; the Telegram
//! crates implement it; `tddy-daemon` brings the two together. The same rule `livekit_is_a_satellite`
//! holds for LiveKit, and read the same way: "names" is a `[dependencies]` or
//! `[build-dependencies]` entry, optional ones included, and `[dev-dependencies]` are not counted.

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use tddy_testing_commons::dependency_graph::WorkspaceGraph;

/// The crates that are Telegram: the Bot API client and every workspace crate built on it.
const TELEGRAM_FAMILY: &[&str] = &["teloxide", "tddy-telegram", "tddy-telegram-control"];

/// Telegram's own workspace crates — the plugin. They implement the operator channel.
const PLUGINS: &[&str] = &["tddy-telegram", "tddy-telegram-control"];

/// The crate that builds the daemon binary and wires the plugin into it.
const WIRING: &[&str] = &["tddy-daemon"];

/// Core crates that once carried Telegram into everything built on them.
const SESSION_CRATES: &[&str] = &[
    "tddy-session-activity",
    "tddy-session-lifecycle",
    "tddy-agent-launch",
    "tddy-cli-sessions",
    "tddy-session-split",
];

fn set_of(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// The Telegram crates `krate` can be built with, following every normal and build edge.
fn telegram_reached_from(graph: &WorkspaceGraph, krate: &str) -> Vec<String> {
    graph
        .reaches(krate)
        .intersection(&set_of(TELEGRAM_FAMILY))
        .cloned()
        .collect()
}

#[test]
fn the_daemon_reaches_the_telegram_plugin() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything the daemon can be built with is followed
    let reached = graph.reaches("tddy-daemon");

    // Then the plugin is in it: "reaches no Telegram crate" below is not vacuous
    assert!(
        reached.contains("tddy-telegram-control"),
        "tddy-daemon reaches {reached:?}, which should include tddy-telegram-control"
    );
}

#[test]
fn only_telegram_plugins_and_wiring_name_a_telegram_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();
    let family = set_of(TELEGRAM_FAMILY);
    let allowed: BTreeSet<String> = set_of(PLUGINS).into_iter().chain(set_of(WIRING)).collect();

    // When every crate naming a Telegram crate is collected
    let naming: BTreeSet<String> = graph
        .workspace_crates()
        .into_iter()
        .filter(|krate| !graph.names_directly(krate).is_disjoint(&family))
        .collect();

    // Then each one is Telegram itself, or the daemon's wiring
    assert_eq!(
        naming.difference(&allowed).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "these crates name a Telegram crate: reach it through the operator channel instead"
    );
}

#[test]
fn the_kernel_reaches_no_telegram_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything the crate defining the operator channel can be built with is followed
    let reached = telegram_reached_from(&graph, "tddy-daemon-kernel");

    // Then none of it is Telegram: a crate that depends on the port gains no Telegram
    assert_eq!(reached, Vec::<String>::new());
}

#[test]
fn no_session_crate_reaches_a_telegram_crate() {
    // Given the workspace as Cargo resolves it
    let graph = WorkspaceGraph::load();

    // When everything each session crate can be built with is followed
    let reaching: Vec<(&str, Vec<String>)> = SESSION_CRATES
        .iter()
        .map(|krate| (*krate, telegram_reached_from(&graph, krate)))
        .filter(|(_, reached)| !reached.is_empty())
        .collect();

    // Then none of them compiles Telegram
    assert_eq!(reaching, Vec::<(&str, Vec<String>)>::new());
}
