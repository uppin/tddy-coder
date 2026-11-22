//! [`watch_jail_exit`], the wait that never reaps, [`JailProcess`] and [`JailBridge`]: a jail's
//! exit is recorded once, whoever looks first, and a jail's stop stops what bridges it.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_task::ChildExit;
use tddy_testing_commons::wait::eventually_awaiting;
use tokio::sync::oneshot;

use super::{wait_without_reaping, watch_jail_exit, JailBridge, JailProcess};
use crate::jail_test_support::{
    a_jail_running, a_jail_state_in, the_runner_is_killed_by, EXITS_3, SLEEPS,
};
use crate::sandbox_session::SandboxSessionState;

/// A safety net for an exit, a stop or an abort to be observed.
const OBSERVE: Duration = Duration::from_secs(10);

/// A process that exits 3 on its own, with its stdio closed.
fn a_child_exiting_3() -> std::process::Child {
    Command::new("sh")
        .args(["-c", "exit 3"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the child starts")
}

/// What `watch` resolved to, which it must within [`OBSERVE`].
async fn watched(watch: tokio::task::JoinHandle<Option<ChildExit>>) -> Option<ChildExit> {
    tokio::time::timeout(OBSERVE, watch)
        .await
        .expect("the watch resolves")
        .expect("the watch does not panic")
}

/// Watch `state` on a task of its own, as the registry does.
fn watching(state: &Arc<SandboxSessionState>) -> tokio::task::JoinHandle<Option<ChildExit>> {
    tokio::spawn(watch_jail_exit(Arc::clone(state)))
}

/// A task that runs until it is aborted, and the receiver that resolves once it is: the task holds
/// the sender, so it is dropped exactly when the task is.
fn a_task_until_aborted() -> (tokio::task::JoinHandle<()>, oneshot::Receiver<()>) {
    let (held, dropped) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _held = held;
        std::future::pending::<()>().await;
    });
    (task, dropped)
}

/// A bridge of three tasks that run until aborted, and a receiver per task.
fn a_bridge_until_aborted() -> (JailBridge, Vec<oneshot::Receiver<()>>) {
    let (endpoint, endpoint_dropped) = a_task_until_aborted();
    let (relay, relay_dropped) = a_task_until_aborted();
    let (forwarder, forwarder_dropped) = a_task_until_aborted();
    (
        JailBridge::new(endpoint, relay, forwarder),
        vec![endpoint_dropped, relay_dropped, forwarder_dropped],
    )
}

/// How many of the bridge's tasks have been dropped, waiting up to [`OBSERVE`] for each.
async fn tasks_dropped(receivers: Vec<oneshot::Receiver<()>>) -> usize {
    let mut dropped = 0;
    for receiver in receivers {
        if let Ok(Err(_)) = tokio::time::timeout(OBSERVE, receiver).await {
            dropped += 1;
        }
    }
    dropped
}

// ---------------------------------------------------------------------------
// The wait that never reaps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn waiting_without_reaping_leaves_the_exit_for_its_owner_to_reap() {
    // Given a child of this process that exits 3
    let mut child = a_child_exiting_3();
    let pid = child.id();

    // When it is waited on without being reaped
    let waited = tokio::task::spawn_blocking(move || wait_without_reaping(pid))
        .await
        .expect("the wait does not panic")
        .expect("the child is waited on");

    // Then the wait saw its status, and its owner still reaps that same status
    let reaped = child
        .try_wait()
        .expect("the child is still its owner's to reap")
        .and_then(|status| status.code());
    assert_eq!((waited, reaped), (ChildExit::with_code(3), Some(3)));
}

#[tokio::test]
async fn a_jail_process_reads_its_exit_without_reaping_it() {
    // Given a jail process whose runner exits 3
    let mut process = JailProcess::Owned(a_child_exiting_3());

    // When its exit is read, and then it is reaped
    let read = eventually_awaiting("the runner's exit to be read", OBSERVE, || {
        let exit = process.try_exit();
        async move { exit.ok_or_else(|| "still running".to_string()) }
    })
    .await;
    let reaped = process.kill_and_reap();

    // Then reading left the status for the reap
    assert_eq!(
        (read, reaped),
        (ChildExit::with_code(3), Some(ChildExit::with_code(3)))
    );
}

// ---------------------------------------------------------------------------
// watch_jail_exit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_watch_records_the_status_of_a_runner_that_exits_3() {
    // Given a jail whose runner exits 3 on its own
    let (state, _dir) = a_jail_running(EXITS_3);

    // When it is watched
    let ended = watched(watching(&state)).await;

    // Then the watch reports the death, and the state records it
    assert_eq!(
        (ended, state.exit()),
        (Some(ChildExit::with_code(3)), Some(ChildExit::with_code(3)))
    );
}

#[tokio::test]
async fn the_watch_records_the_signal_that_killed_its_runner() {
    // Given a watched jail whose runner runs until something kills it
    let (state, _dir) = a_jail_running(SLEEPS);
    let watch = watching(&state);

    // When its runner is killed from outside the daemon
    the_runner_is_killed_by(&state, libc::SIGKILL);

    // Then the watch reports the signal, and the state records it
    assert_eq!(
        (watched(watch).await, state.exit()),
        (
            Some(ChildExit::by_signal(libc::SIGKILL)),
            Some(ChildExit::by_signal(libc::SIGKILL))
        )
    );
}

#[tokio::test]
async fn the_watch_keeps_the_status_when_liveness_was_polled_first() {
    // Given a jail whose runner exits 3, polled for liveness until it is seen gone — what a session
    // listing does before any watch looks
    let (state, _dir) = a_jail_running(EXITS_3);
    eventually_awaiting("the runner to be seen gone", OBSERVE, || async {
        (!state.is_running())
            .then_some(())
            .ok_or_else(|| "still running".to_string())
    })
    .await;

    // When it is watched
    let ended = watched(watching(&state)).await;

    // Then its status was not lost to the poll
    assert_eq!(ended, Some(ChildExit::with_code(3)));
}

#[tokio::test]
async fn the_watch_is_silent_for_a_runner_stopped_during_the_wait() {
    // Given a watched jail whose runner runs until stopped, its watch waiting
    let (state, _dir) = a_jail_running(SLEEPS);
    let watch = watching(&state);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // When it is stopped by request — which needs the lock the watch must not hold while it waits
    let stopping = Arc::clone(&state);
    tokio::time::timeout(
        OBSERVE,
        tokio::task::spawn_blocking(move || stopping.stop()),
    )
    .await
    .expect("the stop is not blocked by the waiting watch")
    .expect("the stop does not panic");

    // Then the watch reports no death
    assert_eq!(watched(watch).await, None);
}

// ---------------------------------------------------------------------------
// JailBridge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aborting_a_bridge_ends_every_task_of_it() {
    // Given a bridge of three running tasks
    let (bridge, receivers) = a_bridge_until_aborted();

    // When
    bridge.abort();

    // Then
    assert_eq!(tasks_dropped(receivers).await, 3);
}

#[tokio::test]
async fn a_jails_stop_aborts_the_bridge_it_holds() {
    // Given a jail holding a bridge of three running tasks
    let dir = tempfile::tempdir().unwrap();
    let (bridge, receivers) = a_bridge_until_aborted();
    let state = a_jail_state_in(dir.path(), SLEEPS).with_bridge(bridge);

    // When the jail is stopped
    state.stop();

    // Then every task of its bridge is gone
    assert_eq!(tasks_dropped(receivers).await, 3);
}
