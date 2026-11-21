//! `TerminalKind::from_wrapped_request` with a `target` wrapper: the login shell run on the session's
//! target is accepted, and nothing else is — an agent on a target is `target-agent-tab`'s, and a
//! target wraps no other wrapper.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md (`#session-liveness` 6/8)

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::{TerminalKind, SANDBOX_WRAPPER_KIND, TARGET_WRAPPER_KIND};

fn a_wrapper(kind: &str) -> LaunchNode {
    LaunchNode {
        kind: kind.to_string(),
        properties: HashMap::new(),
    }
}

#[test]
fn bash_wrapped_in_target_is_a_target_shell() {
    // When
    let kind = TerminalKind::from_wrapped_request("bash", "", &[a_wrapper(TARGET_WRAPPER_KIND)])
        .expect("a shell on the target");

    // Then
    assert_eq!(kind, TerminalKind::TargetShell);
}

#[test]
fn an_empty_kind_wrapped_in_target_is_a_target_shell() {
    // When
    let kind = TerminalKind::from_wrapped_request(" ", "", &[a_wrapper(TARGET_WRAPPER_KIND)])
        .expect("a shell on the target");

    // Then
    assert_eq!(kind, TerminalKind::TargetShell);
}

#[test]
fn an_agent_wrapped_in_target_is_refused_naming_the_agent() {
    // When
    let refusal =
        TerminalKind::from_wrapped_request("claude-cli", "", &[a_wrapper(TARGET_WRAPPER_KIND)])
            .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("\"claude-cli\""),
        "the refusal names the agent: {}",
        refusal.message()
    );
}

#[test]
fn a_tmux_attach_wrapped_in_target_is_refused() {
    // When
    let refusal =
        TerminalKind::from_wrapped_request("tmux", "work", &[a_wrapper(TARGET_WRAPPER_KIND)])
            .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn a_target_wrapping_a_tmux_wrapper_is_refused_naming_it() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "bash",
        "",
        &[a_wrapper(TARGET_WRAPPER_KIND), a_wrapper("tmux")],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("\"tmux\""),
        "the refusal names the wrapper inside the target: {}",
        refusal.message()
    );
}

#[test]
fn a_shell_in_a_sandbox_on_a_target_is_refused() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "bash",
        "",
        &[
            a_wrapper(TARGET_WRAPPER_KIND),
            a_wrapper(SANDBOX_WRAPPER_KIND),
        ],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}
