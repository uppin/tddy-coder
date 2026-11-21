//! Cursor and Muse take a new turn the way Claude does — pasted into their TUI and submitted — so an
//! orchestrator's `SendSessionMessage` reaches every interactive agent it starts (`#orchestra` 7/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Sending a session a turn.

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{embedded_spec, TurnInputSlot};

#[test]
fn cursor_takes_a_turn_as_a_terminal_paste_submitted_with_enter() {
    // When
    let cursor = embedded_spec("cursor").expect("cursor is built in");

    // Then
    assert_eq!(
        cursor.turn_input(),
        Some(&TurnInputSlot::TerminalPaste {
            submit_key: "Enter".to_string()
        })
    );
}

#[test]
fn muse_takes_a_turn_as_a_terminal_paste_submitted_with_enter() {
    // When
    let muse = embedded_spec("muse").expect("muse is built in");

    // Then
    assert_eq!(
        muse.turn_input(),
        Some(&TurnInputSlot::TerminalPaste {
            submit_key: "Enter".to_string()
        })
    );
}
