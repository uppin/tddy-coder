//! `TerminalKind::from_wrapped_request` with a `target` wrapper around an **agent** kind: the agent
//! runs on the session's target — a `TargetAgent` naming the harness — and a wrapper inside the
//! target is still refused.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs) (`#session-liveness` 8/8)

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::{
    TerminalKind, SANDBOX_WRAPPER_KIND, TARGET_WRAPPER_KIND, TMUX_WRAPPER_KIND,
};

fn a_wrapper(kind: &str) -> LaunchNode {
    LaunchNode {
        kind: kind.to_string(),
        properties: HashMap::new(),
    }
}

fn an_agent_on_target(agent: &str) -> TerminalKind {
    TerminalKind::TargetAgent {
        agent: agent.to_string(),
    }
}

#[test]
fn claude_wrapped_in_target_is_an_agent_on_the_target() {
    // When
    let kind =
        TerminalKind::from_wrapped_request("claude-cli", "", &[a_wrapper(TARGET_WRAPPER_KIND)])
            .expect("an agent on the target");

    // Then
    assert_eq!(kind, an_agent_on_target("claude-cli"));
}

#[test]
fn an_agent_kind_wrapped_in_target_is_named_without_its_surrounding_whitespace() {
    // When
    let kind =
        TerminalKind::from_wrapped_request(" cursor-cli ", "", &[a_wrapper(TARGET_WRAPPER_KIND)])
            .expect("an agent on the target");

    // Then
    assert_eq!(kind, an_agent_on_target("cursor-cli"));
}

#[test]
fn an_agent_in_a_tmux_wrapper_on_a_target_is_refused_naming_the_tmux_wrapper() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &[a_wrapper(TARGET_WRAPPER_KIND), a_wrapper(TMUX_WRAPPER_KIND)],
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
fn an_agent_in_a_sandbox_on_a_target_is_refused_naming_the_sandbox() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &[
            a_wrapper(TARGET_WRAPPER_KIND),
            a_wrapper(SANDBOX_WRAPPER_KIND),
        ],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("\"sandbox\""),
        "the refusal names the wrapper inside the target: {}",
        refusal.message()
    );
}
