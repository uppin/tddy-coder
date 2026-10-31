//! Acceptance: a daemon.yaml that still carries a removed agent section fails start-up, naming the
//! `<tddyhome>/tools/<id>.yaml` that replaces it (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md` § Acceptance Criteria.
//!
//! Start-up reads its configuration through `DaemonConfig::load` before anything else
//! (`src/main.rs`, and the desktop host's `config_source`), so a refusal there is a refusal to
//! start: no logger, no listener, no spawn worker.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_daemon::config::DaemonConfig;

/// A daemon.yaml holding `body` after a valid `users:` mapping, in a directory kept alive by the
/// returned guard.
fn a_daemon_yaml(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(
        &path,
        format!("users:\n  - github_user: dev\n    os_user: dev\n{body}"),
    )
    .unwrap();
    (dir, path)
}

/// Why start-up refused the daemon.yaml at `path`.
fn the_startup_refusal(path: &Path) -> String {
    DaemonConfig::load(path)
        .map(|_| ())
        .expect_err("start-up must refuse a removed agent section")
        .to_string()
}

#[test]
fn a_daemon_yaml_with_claude_cli_fails_startup_naming_tddyhome_tools_claude_yaml() {
    // Given
    let (_dir, path) = a_daemon_yaml("claude_cli:\n  binary_path: /opt/claude/bin/claude\n");

    // When
    let refusal = the_startup_refusal(&path);

    // Then
    assert_eq!(
        refusal,
        format!(
            "{}: daemon.yaml `claude_cli:` is removed: moved to <tddyhome>/tools/claude.yaml \
             (binary_path → binary.command)",
            path.display()
        )
    );
}

#[test]
fn the_same_for_cursor_cli_and_cli_harnesses() {
    // Given
    let (_dir, path) = a_daemon_yaml(
        "cursor_cli:\n  binary_path: /opt/cursor/agent\ncli_harnesses:\n  opencode-cli:\n    binary_path: /opt/opencode\n",
    );

    // When
    let refusal = the_startup_refusal(&path);

    // Then
    assert_eq!(
        refusal,
        format!(
            "{}: daemon.yaml `cursor_cli:` is removed: moved to <tddyhome>/tools/cursor.yaml \
             (binary_path → binary.command); daemon.yaml `cli_harnesses.opencode-cli:` is \
             removed: moved to <tddyhome>/tools/opencode.yaml (binary_path → binary.command)",
            path.display()
        )
    );
}

#[test]
fn a_daemon_yaml_without_an_agent_section_still_starts() {
    // Given — a positive control: the refusal is about the removed sections, not about the file
    let (_dir, path) = a_daemon_yaml("allowed_agents:\n  - id: claude\n");

    // When
    let loaded = DaemonConfig::load(&path).map(|config| config.allowed_agents.len());

    // Then
    assert_eq!(loaded.map_err(|e| e.to_string()), Ok(1));
}
