//! `TerminalKind::from_request` for a tmux terminal on a new tmux session.
//!
//! PRD: docs/ft/web/session-terminal-tabs.md (`#session-tabs 7/7`)

use tddy_rpc::Code;
use tddy_terminal_rpc::TerminalKind;

#[test]
fn a_tmux_new_kind_with_a_name_is_a_new_tmux_session_terminal() {
    // When
    let kind = TerminalKind::from_request("tmux-new", "work").expect("a new-session terminal");

    // Then
    assert_eq!(
        kind,
        TerminalKind::TmuxNew {
            session: "work".to_string()
        }
    );
}

#[test]
fn a_tmux_new_kind_without_a_name_is_refused_as_invalid_argument() {
    // When
    let refusal = TerminalKind::from_request("tmux-new", "").unwrap_err();

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}
