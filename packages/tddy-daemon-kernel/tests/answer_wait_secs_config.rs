//! `claude_cli.answer_wait_secs`: how long an orchestrated claude-cli session's `PermissionRequest`
//! hook waits for its orchestrator's answer (`#home-inbox` 6/8).

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::{DaemonConfig, DEFAULT_ANSWER_WAIT_SECS};

fn a_config(yaml: &str) -> DaemonConfig {
    serde_yaml::from_str(yaml).expect("the config parses")
}

#[test]
fn a_claude_cli_section_without_answer_wait_secs_waits_the_default_two_minutes() {
    // Given a claude_cli section that names only its binary
    let config = a_config("claude_cli:\n  binary_path: claude\n");

    // When / Then
    assert_eq!(
        config.claude_cli.as_ref().map(|c| c.answer_wait_secs),
        Some(120)
    );
    assert_eq!(
        config.claude_cli_answer_wait_secs(),
        DEFAULT_ANSWER_WAIT_SECS
    );
}

#[test]
fn a_config_with_no_claude_cli_section_waits_the_default() {
    // Given a config with no claude_cli section at all
    let config = a_config("{}");

    // When / Then
    assert_eq!(config.claude_cli_answer_wait_secs(), 120);
}

#[test]
fn a_configured_answer_wait_secs_is_the_wait() {
    // Given an operator who allows 45 seconds
    let config = a_config("claude_cli:\n  answer_wait_secs: 45\n");

    // When / Then
    assert_eq!(config.claude_cli_answer_wait_secs(), 45);
}
