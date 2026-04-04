//! Ctrl+C handling shared by the local TUI (`event_loop`) and VirtualTui (RPC/web byte 0x03).
//!
//! Must run before `key_event_to_intent`: interrupt kills the tracked **agent/backend child**
//! (e.g. Cursor CLI) via [`tddy_core::kill_child_process`]. It does **not** set the session
//! [`std::sync::atomic::AtomicBool`] passed into the event loop / VirtualTui — that flag is for
//! tearing down the whole runner (SIGINT from [`ctrlc`] handler, daemon shutdown). Stop pane /
//! `UserIntent::Interrupt` use this path so the TUI keeps running after canceling the agent.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tddy_core::kill_child_process;

/// `true` when the key is Ctrl+C press (including when parsed from ETX over VirtualTui).
pub fn key_is_ctrl_c_press(key: &KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('c')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Kill the process registered with [`tddy_core::set_child_pid`], if any (agent/backend only).
///
/// Does not set the workflow `shutdown` flag — the presenter keeps polling and the TUI event loop
/// continues; the backend invoke task observes the child exit and surfaces an error as usual.
pub fn ctrl_c_interrupt_session() {
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

/// Session interrupt targets the tracked backend child only; the tddy process keeps running.
#[cfg(all(test, unix))]
mod interrupt_session_contract_tests {
    use super::ctrl_c_interrupt_session;
    use serial_test::serial;
    use std::sync::Mutex;
    use tddy_core::{clear_child_pid, get_child_pid, set_child_pid};

    static CHILD_PID_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_clear_child_pid() -> std::sync::MutexGuard<'static, ()> {
        let guard = CHILD_PID_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        clear_child_pid();
        guard
    }

    #[test]
    #[serial]
    fn ctrl_c_interrupt_session_is_no_op_when_no_tracked_child() {
        let _guard = lock_and_clear_child_pid();
        ctrl_c_interrupt_session();
        assert_eq!(get_child_pid(), 0);
    }

    #[test]
    #[serial]
    fn ctrl_c_interrupt_session_kills_tracked_agent_child_and_does_not_end_test_process() {
        let _guard = lock_and_clear_child_pid();
        let tddy_pid = std::process::id();

        let mut child = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep stand-in for agent/backend child");
        set_child_pid(child.id());

        ctrl_c_interrupt_session();

        assert_eq!(get_child_pid(), 0, "child pid slot cleared after kill");

        assert_eq!(
            std::process::id(),
            tddy_pid,
            "tddy process must not exit — only the tracked child is killed"
        );

        let status = child.wait().expect("reap child");
        assert!(
            !status.success(),
            "child should have been signalled, not clean exit"
        );
    }
}
