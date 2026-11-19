//! [`CliSessionManager::register_process_tab`]: a tool session's `tddy-coder` as a PTY-less tab.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_task::{exit_channel, ChildExit, ExitSender};
use tddy_testing_commons::wait::eventually_awaiting;

use super::process_tab::{ProcessTab, ProcessTabHolder, SignalRefused, TDDY_CODER_KIND};
use super::{CliSessionManager, MAIN_TERMINAL_ID};

/// A safety net for an exit to be observed.
const OBSERVE: Duration = Duration::from_secs(10);

const SESSION: &str = "tool-session";

/// A pid no process tab here ever signals: every holder is a [`RecordingHolder`].
const HELD_PID: u32 = 4242;

/// What a holder was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Stop,
    Signal(i32),
}

/// A holder that records what it is asked and touches no process.
#[derive(Default)]
struct RecordingHolder {
    asked: Mutex<Vec<Asked>>,
}

impl RecordingHolder {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ProcessTabHolder for RecordingHolder {
    async fn stop(&self) -> anyhow::Result<()> {
        self.asked.lock().unwrap().push(Asked::Stop);
        Ok(())
    }

    async fn signal(&self, signal: i32) -> anyhow::Result<()> {
        self.asked.lock().unwrap().push(Asked::Signal(signal));
        Ok(())
    }
}

/// A manager holding `SESSION`'s `tddy-coder` as a process tab; the returned sender reports its end.
async fn a_manager_holding_a_tddy_coder() -> (CliSessionManager, ExitSender, Arc<RecordingHolder>) {
    let manager = CliSessionManager::new();
    let holder = Arc::new(RecordingHolder::default());
    let (exit_tx, exit_rx) = exit_channel();
    manager
        .register_process_tab(SESSION, TDDY_CODER_KIND, HELD_PID, exit_rx, holder.clone())
        .await
        .expect("the process tab is registered");
    (manager, exit_tx, holder)
}

#[tokio::test]
async fn a_process_tab_is_held_as_the_sessions_main_tab_with_its_kind_and_pid() {
    // Given
    let (manager, _exit, _holder) = a_manager_holding_a_tddy_coder().await;

    // When
    let tab = manager.process_tab(SESSION).await;

    // Then
    assert_eq!(
        tab,
        Some(ProcessTab {
            terminal_id: MAIN_TERMINAL_ID.to_string(),
            kind: TDDY_CODER_KIND.to_string(),
            pid: HELD_PID,
        })
    );
}

#[tokio::test]
async fn a_process_tab_is_not_listed_as_a_terminal() {
    // Given
    let (manager, _exit, _holder) = a_manager_holding_a_tddy_coder().await;

    // When
    let terminals = manager.list_terminals(SESSION).await;

    // Then
    assert_eq!(
        terminals.len(),
        0,
        "a process tab has no terminal to stream"
    );
}

/// Blocked on `#session-liveness` 1/4: `live_agent_sessions` is that node's.
#[tokio::test]
async fn a_process_tab_makes_its_session_live() {
    // Given
    let (manager, _exit, _holder) = a_manager_holding_a_tddy_coder().await;

    // When
    let live = manager.live_agent_sessions().await;

    // Then
    assert!(live.contains(SESSION));
}

#[tokio::test]
async fn a_process_tab_is_gone_once_its_exit_is_reported() {
    // Given
    let (manager, exit, _holder) = a_manager_holding_a_tddy_coder().await;

    // When
    exit.send(ChildExit::with_code(0));

    // Then
    eventually_awaiting("the process tab to leave the registry", OBSERVE, || async {
        match manager.process_tab(SESSION).await {
            None => Ok(()),
            Some(tab) => Err(format!("{tab:?} is still held")),
        }
    })
    .await;
}

#[tokio::test]
async fn signalling_a_process_tab_reaches_its_holder() {
    // Given
    let (manager, _exit, holder) = a_manager_holding_a_tddy_coder().await;

    // When
    manager
        .signal_process_tab(SESSION, libc::SIGINT)
        .await
        .expect("a held process tab is signalled");

    // Then
    assert_eq!(holder.asked(), vec![Asked::Signal(libc::SIGINT)]);
}

#[tokio::test]
async fn signalling_a_session_without_a_process_tab_is_refused_as_not_held() {
    // Given
    let manager = CliSessionManager::new();

    // When
    let refusal = manager.signal_process_tab(SESSION, libc::SIGTERM).await;

    // Then
    assert!(
        matches!(refusal, Err(SignalRefused::NotHeld)),
        "{refusal:?}"
    );
}

#[tokio::test]
async fn stopping_a_session_stops_its_process_tab_through_its_holder() {
    // Given
    let (manager, _exit, holder) = a_manager_holding_a_tddy_coder().await;

    // When
    manager.stop_session(SESSION).await;

    // Then
    assert_eq!(holder.asked(), vec![Asked::Stop]);
}
