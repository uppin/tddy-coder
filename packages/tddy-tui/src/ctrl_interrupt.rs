//! Ctrl+C handling shared by the local TUI (`event_loop`) and VirtualTui (RPC/web byte 0x03).
//!
//! [`ctrl_c_interrupt_session`] kills the tracked **agent/backend child** via
//! [`tddy_core::kill_running_turns`] only. The local TUI and VirtualTui call it **and** set the
//! shared `shutdown` flag: in raw mode, Ctrl+C is usually a key event (not SIGINT), so without
//! that the process would never exit when no child is registered (e.g. stub demo). Stop pane /
//! `UserIntent::Interrupt` still only kill the child so the session can continue.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::kill_running_turns;

/// `true` when the key is Ctrl+C press (including when parsed from ETX over VirtualTui).
pub fn key_is_ctrl_c_press(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Kill the process group of every running agent/backend turn, if any (agent/backend only).
///
/// Does not set the workflow `shutdown` flag — the presenter keeps polling and the TUI event loop
/// continues; the backend invoke task observes the child exit and surfaces an error as usual.
pub fn ctrl_c_interrupt_session() {
    kill_running_turns();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    #[test]
    fn key_is_ctrl_c_press_true_for_control_c() {
        // Given
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );

        // When / Then
        assert!(key_is_ctrl_c_press(&key));
    }

    #[test]
    fn key_is_ctrl_c_press_false_for_plain_c() {
        // Given
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );

        // When / Then
        assert!(!key_is_ctrl_c_press(&key));
    }
}

/// Session interrupt targets the tracked backend child only; the tddy process keeps running.
#[cfg(all(test, unix))]
mod interrupt_session_contract_tests {
    use super::ctrl_c_interrupt_session;
    use serial_test::serial;
    use std::os::unix::process::CommandExt as _;
    use tddy_core::RunningGroup;

    /// A stand-in for an agent turn: `sleep` leading a process group of its own.
    fn a_running_turn() -> std::process::Child {
        std::process::Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .expect("spawn sleep stand-in for agent/backend child")
    }

    #[test]
    #[serial]
    fn ctrl_c_interrupt_session_is_no_op_when_no_running_turn() {
        // When / Then
        ctrl_c_interrupt_session();
    }

    #[test]
    #[serial]
    fn ctrl_c_interrupt_session_kills_every_running_turn_and_does_not_end_test_process() {
        // Given
        let tddy_pid = std::process::id();
        let mut first = a_running_turn();
        let mut second = a_running_turn();
        let _first_group = RunningGroup::new(first.id());
        let _second_group = RunningGroup::new(second.id());

        // When
        ctrl_c_interrupt_session();

        // Then
        assert_eq!(
            std::process::id(),
            tddy_pid,
            "tddy process must not exit — only the running turns are killed"
        );
        let signalled: Vec<bool> = [&mut first, &mut second]
            .into_iter()
            .map(|child| !child.wait().expect("reap child").success())
            .collect();
        assert_eq!(
            signalled,
            vec![true, true],
            "every turn should have been signalled, not exited cleanly"
        );
    }
}
