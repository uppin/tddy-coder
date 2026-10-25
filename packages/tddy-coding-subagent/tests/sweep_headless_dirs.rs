//! Unit tests: the start-up sweep removes the files every headless conversation of a previous daemon
//! run left under `<tddy_data_dir>/headless/` — each holds an MCP config carrying its agent's
//! credential — and nothing else in the data directory.
//!
//! Backlog: docs/dev/todo/2026-10-25-headless-approval-route-residuals.md

use tddy_coding_subagent::launcher::{
    headless_conversation_dir, sweep_headless_dirs, HEADLESS_DIR,
};

/// A data directory as a daemon that stopped mid-conversation left it: two conversations' headless
/// files, and the daemon's other state beside them.
fn a_data_dir_left_by_a_stopped_daemon() -> tempfile::TempDir {
    let data_dir = tempfile::tempdir().expect("data dir");
    for conversation in ["conv-a", "conv-b"] {
        let dir = headless_conversation_dir(data_dir.path(), conversation);
        std::fs::create_dir_all(&dir).expect("conversation dir");
        std::fs::write(dir.join("mcp.json"), "{\"mcpServers\":{}}").expect("mcp config");
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
fn the_sweep_removes_every_conversations_headless_files() {
    // Given
    let data_dir = a_data_dir_left_by_a_stopped_daemon();

    // When
    sweep_headless_dirs(data_dir.path()).expect("the sweep succeeds");

    // Then
    for conversation in ["conv-a", "conv-b"] {
        let dir = headless_conversation_dir(data_dir.path(), conversation);
        assert!(!dir.exists(), "{} was swept", dir.display());
    }
}

#[test]
fn the_sweep_leaves_the_rest_of_the_data_directory_alone() {
    // Given
    let data_dir = a_data_dir_left_by_a_stopped_daemon();

    // When
    sweep_headless_dirs(data_dir.path()).expect("the sweep succeeds");

    // Then
    assert!(data_dir.path().join("agents/explorer.yaml").exists());
}

#[test]
fn a_data_directory_with_no_headless_files_is_swept_without_error() {
    // Given
    let data_dir = tempfile::tempdir().expect("data dir");
    assert!(!data_dir.path().join(HEADLESS_DIR).exists());

    // When / Then
    sweep_headless_dirs(data_dir.path()).expect("nothing to sweep is not an error");
}
