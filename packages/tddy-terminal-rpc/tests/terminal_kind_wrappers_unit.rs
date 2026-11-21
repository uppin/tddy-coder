//! `TerminalKind::from_wrapped_request`: a `StartTerminalSession` naming what its terminal runs
//! wrapped in — an agent in tmux, and the wrappers this host refuses.
//!
//! PRD: docs/ft/daemon/session-liveness.md (`#session-liveness` 2/4)

use std::collections::HashMap;

use tddy_rpc::Code;
use tddy_terminal_rpc::proto::terminal_session::LaunchNode;
use tddy_terminal_rpc::TerminalKind;

fn a_wrapper(kind: &str) -> LaunchNode {
    LaunchNode {
        kind: kind.to_string(),
        properties: HashMap::new(),
    }
}

#[test]
fn an_agent_kind_wrapped_in_tmux_is_a_tmux_wrapped_agent() {
    // When
    let kind = TerminalKind::from_wrapped_request("muse-cli", "", &[a_wrapper("tmux")])
        .expect("a tmux-wrapped agent");

    // Then
    assert_eq!(
        kind,
        TerminalKind::TmuxWrappedAgent {
            agent: "muse-cli".to_string()
        }
    );
}

#[test]
fn a_kind_with_no_wrappers_is_read_as_before() {
    // When
    let kind = TerminalKind::from_wrapped_request(" warp-cli ", "", &[]).expect("an agent tab");

    // Then
    assert_eq!(kind, TerminalKind::Agent("warp-cli".to_string()));
}

#[test]
fn an_ssh_wrapper_is_refused_naming_it() {
    // When
    let refusal = TerminalKind::from_wrapped_request(
        "claude-cli",
        "",
        &[a_wrapper("ssh"), a_wrapper("tmux")],
    )
    .unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(
        refusal.message().contains("\"ssh\""),
        "the refusal names the wrapper kind: {}",
        refusal.message()
    );
}

#[test]
fn a_shell_wrapped_in_tmux_is_refused() {
    // When
    let refusal = TerminalKind::from_wrapped_request("bash", "", &[a_wrapper("tmux")]).unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}
