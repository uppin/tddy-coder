//! Unit tests: the start-up sweep removes the files every headless conversation of this daemon's
//! previous run left under `<tddy_data_dir>/headless/` — each holds an MCP config carrying its
//! agent's credential — and nothing another daemon sharing the data directory created.
//!
//! Backlog: docs/dev/todo/2026-10-25-headless-approval-route-residuals.md

use std::path::Path;

use tddy_coding_subagent::launcher::{
    claim_headless_conversation_dir, headless_conversation_dir, sweep_headless_dirs, HEADLESS_DIR,
};

/// The durable instance id of the daemon sweeping.
const THIS_DAEMON: &str = "mac-desktop";
/// Another daemon sharing the data directory.
const ANOTHER_DAEMON: &str = "dev";

/// A conversation's headless files, as the daemon `owner` writes them.
fn a_conversation_of(data_dir: &Path, owner: &str, conversation: &str) {
    let dir = headless_conversation_dir(data_dir, conversation);
    claim_headless_conversation_dir(&dir, owner).expect("claim the conversation dir");
    std::fs::write(dir.join("mcp.json"), "{\"mcpServers\":{}}").expect("mcp config");
}

/// A data directory as this daemon left it when it stopped mid-conversation: two of its
/// conversations' headless files, and the daemon's other state beside them.
fn a_data_dir_left_by_a_stopped_daemon() -> tempfile::TempDir {
    let data_dir = tempfile::tempdir().expect("data dir");
    for conversation in ["conv-a", "conv-b"] {
        a_conversation_of(data_dir.path(), THIS_DAEMON, conversation);
    }
    std::fs::create_dir_all(data_dir.path().join("agents")).expect("agents dir");
    std::fs::write(
        data_dir.path().join("agents/explorer.yaml"),
        "name: explorer\n",
    )
    .expect("agent def");
    data_dir
}

#[test]
fn the_sweep_removes_every_headless_conversation_this_daemon_left() {
    // Given
    let data_dir = a_data_dir_left_by_a_stopped_daemon();

    // When
    let mut swept = sweep_headless_dirs(data_dir.path(), THIS_DAEMON).expect("the sweep succeeds");

    // Then
    swept.sort();
    assert_eq!(swept, vec!["conv-a".to_string(), "conv-b".to_string()]);
    for conversation in ["conv-a", "conv-b"] {
        let dir = headless_conversation_dir(data_dir.path(), conversation);
        assert!(!dir.exists(), "{} was swept", dir.display());
    }
}

#[test]
fn the_sweep_leaves_another_daemons_conversations_alone() {
    // Given another daemon sharing the data directory, with a conversation open
    let data_dir = a_data_dir_left_by_a_stopped_daemon();
    a_conversation_of(data_dir.path(), ANOTHER_DAEMON, "conv-theirs");

    // When this daemon sweeps
    sweep_headless_dirs(data_dir.path(), THIS_DAEMON).expect("the sweep succeeds");

    // Then the other daemon's conversation keeps its files
    let theirs = headless_conversation_dir(data_dir.path(), "conv-theirs");
    assert!(
        theirs.join("mcp.json").exists(),
        "{} survived the sweep",
        theirs.display()
    );
}

#[test]
fn the_sweep_leaves_a_directory_that_names_no_daemon_alone() {
    // Given a conversation directory no daemon claimed
    let data_dir = a_data_dir_left_by_a_stopped_daemon();
    let unclaimed = headless_conversation_dir(data_dir.path(), "conv-unclaimed");
    std::fs::create_dir_all(&unclaimed).expect("unclaimed dir");

    // When
    sweep_headless_dirs(data_dir.path(), THIS_DAEMON).expect("the sweep succeeds");

    // Then
    assert!(
        unclaimed.exists(),
        "nothing says it is this daemon's to remove"
    );
}

#[test]
fn the_sweep_leaves_the_rest_of_the_data_directory_alone() {
    // Given
    let data_dir = a_data_dir_left_by_a_stopped_daemon();

    // When
    sweep_headless_dirs(data_dir.path(), THIS_DAEMON).expect("the sweep succeeds");

    // Then
    assert!(data_dir.path().join("agents/explorer.yaml").exists());
}

#[test]
fn a_data_directory_with_no_headless_files_is_swept_without_error() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    assert!(!data_dir.path().join(HEADLESS_DIR).exists());

    // When / Then
    let swept = sweep_headless_dirs(data_dir.path(), THIS_DAEMON)
        .expect("nothing to sweep is not an error");
    assert!(swept.is_empty());
}
