//! Reproduces: Web/RPC terminal sends Ctrl+C as a single byte (ETX, 0x03). The local TUI
//! (`event_loop`) handles Ctrl+C by calling `tddy_core::kill_child_process()` so the tracked
//! backend child is interrupted. VirtualTui must do the same; otherwise Ctrl+C “has no effect”
//! on the underlying process even though the client logs `[terminal→server] Ctrl+C (1 bytes) [3]`.
//!
//! Acceptance: after sending 0x03 through the VirtualTui input path (same as RPC → `input_tx`),
//! the process registered via `set_child_pid` must be terminated, matching local TUI behavior.

#![cfg(unix)]

use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use tddy_core::presenter::PresenterEvent;
use tddy_core::{
    clear_child_pid, set_child_pid, ActivityEntry, ActivityKind, AppMode, PresenterState,
    UserIntent, ViewConnection,
};
use tddy_tui::run_virtual_tui;

fn running_presenter_state() -> PresenterState {
    PresenterState {
        agent: "test".to_string(),
        model: "test".to_string(),
        mode: AppMode::Running,
        current_goal: Some("test-goal".to_string()),
        current_state: None,
        workflow_session_id: None,
        goal_start_time: Instant::now(),
        activity_log: vec![ActivityEntry {
            text: "running".to_string(),
            kind: ActivityKind::Info,
        }],
        inbox: vec![],
        should_quit: false,
        exit_action: None,
        plan_refinement_pending: false,
        skills_project_root: None,
        active_worktree_display: None,
    }
}

#[test]
fn virtual_tui_rpc_ctrl_c_byte_kills_tracked_child_like_local_tui() {
    clear_child_pid();

    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep child for Ctrl+C kill test");
    let pid = child.id();
    set_child_pid(pid);

    let (event_tx, _) = tokio::sync::broadcast::channel::<PresenterEvent>(16);
    let (intent_tx, intent_rx) = std::sync::mpsc::channel::<UserIntent>();

    let conn = ViewConnection {
        state_snapshot: running_presenter_state(),
        event_rx: event_tx.subscribe(),
        intent_tx,
        critical_state: std::sync::Arc::new(std::sync::Mutex::new(
            tddy_core::CriticalPresenterState::default(),
        )),
    };

    let (output_tx, _output_rx) = mpsc::channel(64);
    let (input_tx, input_rx) = mpsc::channel(64);
    let shutdown = Arc::new(AtomicBool::new(false));

    run_virtual_tui(conn, output_tx, input_rx, shutdown.clone(), false);

    // Let the VirtualTui thread create the terminal and enter its poll loop.
    std::thread::sleep(Duration::from_millis(80));

    let rt = Runtime::new().expect("tokio runtime for input_tx.send");
    rt.block_on(async {
        input_tx
            .send(vec![0x03])
            .await
            .expect("send Ctrl+C (ETX) as RPC terminal does");
    });

    // Local TUI kills the child immediately on Ctrl+C; VirtualTui should match.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut exited = false;
    while Instant::now() < deadline {
        if child.try_wait().expect("try_wait").is_some() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    drop(intent_rx);
    clear_child_pid();

    if !exited {
        let _ = child.kill();
    }
    // Always reap the handle (after `try_wait` => Some, `wait` may return an error; ignore).
    let _ = child.wait();

    assert!(
        exited,
        "Ctrl+C byte (0x03) over VirtualTui input must kill the tracked child process (parity with local TUI kill_child_process); child pid {} still running",
        pid
    );
}
