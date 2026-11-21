//! `TerminalKind::from_wrapped_request` with a `sandbox` wrapper: the login shell run in a jail of
//! its own is accepted, and nothing else is — an agent in a jail is a later node's, and a sandbox
//! wraps no other wrapper.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md (`#session-liveness` 5/8)

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::{TerminalKind, SANDBOX_WRAPPER_KIND};

fn a_wrapper(kind: &str) -> LaunchNode {
    LaunchNode {
        kind: kind.to_string(),
        properties: HashMap::new(),
    }
}

#[test]
fn bash_wrapped_in_sandbox_is_a_sandboxed_shell() {
    // When
    let kind = TerminalKind::from_wrapped_request("bash", "", &[a_wrapper(SANDBOX_WRAPPER_KIND)])
        .expect("a jailed shell");

    // Then
    assert_eq!(kind, TerminalKind::SandboxedShell);
}

#[test]
fn an_empty_kind_wrapped_in_sandbox_is_a_sandboxed_shell() {
    // When
    let kind = TerminalKind::from_wrapped_request(" ", "", &[a_wrapper(SANDBOX_WRAPPER_KIND)])
        .expect("a jailed shell");

    // Then
    assert_eq!(kind, TerminalKind::SandboxedShell);
}

#[test]
fn an_agent_wrapped_in_sandbox_is_refused_naming_the_agent() {
    // When
    let refusal =
        TerminalKind::from_wrapped_request("claude-cli", "", &[a_wrapper(SANDBOX_WRAPPER_KIND)])
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
fn a_tmux_attach_wrapped_in_sandbox_is_refused() {
    // When
    let refusal =
        TerminalKind::from_wrapped_request("tmux", "work", &[a_wrapper(SANDBOX_WRAPPER_KIND)])
            .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn a_sandbox_wrapping_a_tmux_wrapper_is_refused_naming_it() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &[a_wrapper(SANDBOX_WRAPPER_KIND), a_wrapper("tmux")],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("\"tmux\""),
        "the refusal names the wrapper inside the sandbox: {}",
        refusal.message()
    );
}

#[test]
fn a_shell_in_a_sandbox_inside_a_sandbox_is_refused() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "bash",
        "",
        &[
            a_wrapper(SANDBOX_WRAPPER_KIND),
            a_wrapper(SANDBOX_WRAPPER_KIND),
        ],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}
