//! How each harness is spoken to as a subagent, and who answers its permission requests.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use tddy_coding_subagent::{permission_policy_of, transport_of, PermissionPolicy, Transport};

#[test]
fn acp_harnesses_take_the_acp_transport_and_ask_the_caller() {
    for harness in ["claude-acp", "codex-acp", "opencode"] {
        assert_eq!(transport_of(harness), Some(Transport::Acp), "{harness}");
        assert_eq!(
            permission_policy_of(harness),
            Some(PermissionPolicy::AsksCaller),
            "{harness}"
        );
    }
}

#[test]
fn claude_headless_asks_the_caller_through_its_permission_prompt_tool() {
    assert_eq!(transport_of("claude"), Some(Transport::Headless));
    assert_eq!(
        permission_policy_of("claude"),
        Some(PermissionPolicy::AsksCaller)
    );
}

#[test]
fn cursor_and_codex_run_under_a_fixed_policy_the_row_names() {
    for harness in ["cursor", "codex"] {
        assert_eq!(
            transport_of(harness),
            Some(Transport::Headless),
            "{harness}"
        );
        assert!(
            matches!(permission_policy_of(harness), Some(PermissionPolicy::Fixed(ref text)) if !text.is_empty()),
            "{harness}: {:?}",
            permission_policy_of(harness)
        );
    }
}

#[test]
fn a_pty_only_harness_has_no_subagent_transport() {
    for harness in ["muse-cli", "claude-cli", "cursor-cli"] {
        assert_eq!(transport_of(harness), None, "{harness}");
    }
}

#[test]
fn warp_oz_is_headless_and_runs_under_its_agent_profiles_policy() {
    assert_eq!(transport_of("oz"), Some(Transport::Headless));
    assert!(
        matches!(permission_policy_of("oz"), Some(PermissionPolicy::Fixed(ref text)) if text.contains("profile")),
        "{:?}",
        permission_policy_of("oz")
    );
    assert_eq!(
        transport_of("warp-cli"),
        None,
        "the interactive Warp TUI is not a subagent"
    );
}
