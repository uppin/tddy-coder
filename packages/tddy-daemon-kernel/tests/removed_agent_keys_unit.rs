//! The daemon.yaml agent sections `#agent-tools 12/12` removes — `claude_cli:`, `cursor_cli:` and
//! `cli_harnesses:` — are refused with the tool file that replaces each, and where each of its
//! fields went.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`.

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::{refuse_removed_agent_keys, RemovedAgentKey};

fn a_document(yaml: &str) -> serde_yaml::Value {
    serde_yaml::from_str(yaml).expect("the daemon.yaml parses")
}

fn moved(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(field, to)| (field.to_string(), to.to_string()))
        .collect()
}

#[test]
fn a_claude_cli_section_is_refused_naming_claude_yaml_and_each_field_it_set() {
    // Given
    let document = a_document(
        "claude_cli:\n  binary_path: /opt/claude/bin/claude\n  claude_home_dir: /srv/claude-home\n  fullscreen: false\n  answer_wait_secs: 45\n",
    );

    // When
    let refused = refuse_removed_agent_keys(&document).expect_err("claude_cli is removed");

    // Then
    assert_eq!(
        (refused.removed.clone(), refused.to_string()),
        (
            vec![RemovedAgentKey {
                key: "claude_cli".to_string(),
                tool: "claude".to_string(),
                moved: moved(&[
                    ("binary_path", "binary.command"),
                    ("claude_home_dir", "home.key"),
                    ("fullscreen", "run.env"),
                    ("answer_wait_secs", "coding_agent.answer_wait_secs"),
                ]),
            }],
            "daemon.yaml `claude_cli:` is removed: moved to <tddyhome>/tools/claude.yaml \
             (binary_path → binary.command, claude_home_dir → home.key, fullscreen → run.env, \
             answer_wait_secs → coding_agent.answer_wait_secs)"
                .to_string(),
        )
    );
}

#[test]
fn a_cursor_cli_section_is_refused_naming_cursor_yaml() {
    // Given
    let document = a_document(
        "cursor_cli:\n  binary_path: /opt/cursor/agent\n  cursor_home_dir: /srv/cursor\n",
    );

    // When
    let refused = refuse_removed_agent_keys(&document).expect_err("cursor_cli is removed");

    // Then
    assert_eq!(
        refused.to_string(),
        "daemon.yaml `cursor_cli:` is removed: moved to <tddyhome>/tools/cursor.yaml \
         (binary_path → binary.command, cursor_home_dir → home.key)"
    );
}

#[test]
fn each_cli_harnesses_entry_is_refused_naming_the_tool_its_session_type_runs() {
    // Given
    let document = a_document(
        "cli_harnesses:\n  opencode-cli:\n    binary_path: /opt/opencode\n  muse-cli:\n    binary_path: /opt/muse\n",
    );

    // When
    let refused = refuse_removed_agent_keys(&document).expect_err("cli_harnesses is removed");

    // Then
    assert_eq!(
        refused.to_string(),
        "daemon.yaml `cli_harnesses.opencode-cli:` is removed: moved to \
         <tddyhome>/tools/opencode.yaml (binary_path → binary.command); \
         daemon.yaml `cli_harnesses.muse-cli:` is removed: moved to <tddyhome>/tools/muse.yaml \
         (binary_path → binary.command)"
    );
}

#[test]
fn a_daemon_yaml_without_an_agent_section_is_accepted() {
    // Given — a positive control: everything else a daemon.yaml says is untouched
    let document =
        a_document("users:\n  - github_user: u\n    os_user: u\nallowed_agents:\n  - id: claude\n");

    // When
    let accepted = refuse_removed_agent_keys(&document);

    // Then
    assert_eq!(accepted, Ok(()));
}
