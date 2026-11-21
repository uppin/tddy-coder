//! `TerminalKind::from_wrapped_request` with a `sandbox` wrapper around an **agent** kind: the agent
//! runs in a jail of its own — a `SandboxedAgent` naming the harness — and a wrapper inside the
//! sandbox is still refused.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-agent-tab.md (`#session-liveness` 7/8)

use std::collections::HashMap;

use pretty_assertions::assert_eq;
use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::{TerminalKind, SANDBOX_WRAPPER_KIND, TMUX_WRAPPER_KIND};

fn a_wrapper(kind: &str) -> LaunchNode {
    LaunchNode {
        kind: kind.to_string(),
        properties: HashMap::new(),
    }
}

fn a_sandboxed_agent(agent: &str) -> TerminalKind {
    TerminalKind::SandboxedAgent {
        agent: agent.to_string(),
    }
}

#[test]
fn claude_wrapped_in_sandbox_is_a_sandboxed_agent() {
    // When
    let kind =
        TerminalKind::from_wrapped_request("claude-cli", "", &[a_wrapper(SANDBOX_WRAPPER_KIND)])
            .expect("a jailed agent");

    // Then
    assert_eq!(kind, a_sandboxed_agent("claude-cli"));
}

#[test]
fn an_agent_kind_wrapped_in_sandbox_is_named_without_its_surrounding_whitespace() {
    // When
    let kind = TerminalKind::from_wrapped_request(
        " opencode-cli ",
        "",
        &[a_wrapper(SANDBOX_WRAPPER_KIND)],
    )
    .expect("a jailed agent");

    // Then
    assert_eq!(kind, a_sandboxed_agent("opencode-cli"));
}

#[test]
fn an_agent_in_a_tmux_wrapper_inside_a_sandbox_is_refused_naming_the_tmux_wrapper() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &[
            a_wrapper(SANDBOX_WRAPPER_KIND),
            a_wrapper(TMUX_WRAPPER_KIND),
        ],
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
fn an_agent_in_a_sandbox_inside_a_sandbox_is_refused() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
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
