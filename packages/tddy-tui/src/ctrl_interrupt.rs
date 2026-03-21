//! Ctrl+C handling shared by the local TUI (`event_loop`) and VirtualTui (RPC/web byte 0x03).
//!
//! Must run before `key_event_to_intent`: interrupt kills the tracked backend child and stops
//! the session; it is not the same as `UserIntent::Quit` alone.

use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::kill_child_process;

/// `true` when the key is Ctrl+C press (including when parsed from ETX over VirtualTui).
pub fn key_is_ctrl_c_press(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Signal shutdown and kill the process registered with `set_child_pid`, if any.
pub fn ctrl_c_interrupt_session(shutdown: &AtomicBool) {
    shutdown.store(true, Ordering::Relaxed);
    kill_child_process();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    #[test]
    fn key_is_ctrl_c_press_true_for_control_c() {
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert!(key_is_ctrl_c_press(&key));
    }

    #[test]
    fn key_is_ctrl_c_press_false_for_plain_c() {
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::empty(),
            KeyEventKind::Press,
        );
        assert!(!key_is_ctrl_c_press(&key));
    }
}
