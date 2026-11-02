//! `TerminalKind::from_request`: which terminal a `StartTerminalSession` names, including a tmux
//! terminal and the tmux session it attaches to.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-attach-tab.md (`#session-tabs 3/7`)

use tddy_rpc::Code;
use tddy_terminal_rpc::TerminalKind;

#[test]
fn a_tmux_kind_with_a_session_is_a_tmux_terminal_attached_to_it() {
    // When
    let kind = TerminalKind::from_request("tmux", " alpha ").expect("a tmux terminal");

    // Then
    assert_eq!(
        kind,
        TerminalKind::Tmux {
            session: "alpha".to_string()
        }
    );
}

#[test]
fn a_tmux_kind_without_a_session_is_refused_as_invalid_argument() {
    // When
    let refusal = TerminalKind::from_request("tmux", "  ").unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn bash_and_harness_kinds_are_read_as_before() {
    assert_eq!(
        TerminalKind::from_request("", "").unwrap(),
        TerminalKind::Bash
    );
    assert_eq!(
        TerminalKind::from_request("bash", "").unwrap(),
        TerminalKind::Bash
    );
    assert_eq!(
        TerminalKind::from_request(" warp-cli ", "").unwrap(),
        TerminalKind::Agent("warp-cli".to_string())
    );
}
