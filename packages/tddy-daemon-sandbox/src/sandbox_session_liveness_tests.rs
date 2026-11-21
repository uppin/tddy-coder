//! [`SandboxSessionManager`]: a sandboxed session is live while its main jail's runner runs, and a
//! main jail whose runner exits on its own leaves the registry and is announced
//! (`#tab-hardening` 3/4).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-jail-death-watch.md

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_task::ChildExit;
use tddy_testing_commons::wait::eventually_awaiting;
use tokio::sync::broadcast;

use crate::jail_exit_watch::JailEnded;
use crate::jail_test_support::{a_jail_running, the_runner_is_killed_by, EXITS_3, SLEEPS};
use crate::sandbox_session::SandboxSessionManager;

/// A safety net for a runner's exit to be observed.
const OBSERVE: Duration = Duration::from_secs(10);

/// The next main jail announced ended, which must come.
async fn the_next_jail_ended(ended: &mut broadcast::Receiver<JailEnded>) -> JailEnded {
    tokio::time::timeout(OBSERVE, ended.recv())
        .await
        .expect("a main jail is announced ended")
        .expect("the death watch's channel stays open")
}

#[tokio::test]
async fn a_sandboxed_session_whose_runner_runs_is_live() {
    // Given
    let manager = SandboxSessionManager::new();
    let (state, _dir) = a_jail_running(SLEEPS);
    manager.insert("jailed-running".to_string(), state).await;

    // When
    let live = manager.live_sessions().await;

    // Then
    assert!(live.contains("jailed-running"));
}

#[tokio::test]
async fn a_sandboxed_session_whose_runner_exited_is_not_live() {
    // Given a registered session whose runner exits by itself
    let manager = SandboxSessionManager::new();
    let (state, _dir) = a_jail_running(&["true"]);
    manager.insert("jailed-exited".to_string(), state).await;

    // When / Then it is not live once the runner has exited
    eventually_awaiting("the exited runner to stop being live", OBSERVE, || async {
        (!manager.live_sessions().await.contains("jailed-exited"))
            .then_some(())
            .ok_or_else(|| "still live".to_string())
    })
    .await;
}

#[tokio::test]
async fn a_main_jail_that_exits_is_announced_ended_with_its_status() {
    // Given a subscriber to the death watch
    let manager = SandboxSessionManager::new();
    let mut ended = manager.subscribe_ended();

    // When a main jail whose runner exits 3 on its own is registered
    let (state, _dir) = a_jail_running(EXITS_3);
    manager.insert("jailed-exits-3".to_string(), state).await;

    // Then its end is announced, naming its session and its status
    assert_eq!(
        the_next_jail_ended(&mut ended).await,
        JailEnded {
            session_id: "jailed-exits-3".to_string(),
            exit: ChildExit::with_code(3),
        }
    );
}

#[tokio::test]
async fn a_main_jail_stopped_by_request_is_not_announced() {
    // Given a subscriber to the death watch, and a registered main jail that runs until stopped
    let manager = SandboxSessionManager::new();
    let mut ended = manager.subscribe_ended();
    let (stopped, _stopped_dir) = a_jail_running(SLEEPS);
    manager
        .insert("jailed-stopped".to_string(), Arc::clone(&stopped))
        .await;

    // When it is stopped as a delete stops it — while still registered — and forgotten, and then
    // another main jail dies on its own
    stopped.stop();
    manager.remove("jailed-stopped").await;
    let (dying, _dying_dir) = a_jail_running(SLEEPS);
    manager
        .insert("jailed-dying".to_string(), Arc::clone(&dying))
        .await;
    the_runner_is_killed_by(&dying, libc::SIGKILL);

    // Then the only end announced is the one that was not asked for
    let mut announced = vec![the_next_jail_ended(&mut ended).await.session_id];
    while let Ok(more) = ended.try_recv() {
        announced.push(more.session_id);
    }
    assert_eq!(announced, vec!["jailed-dying".to_string()]);
}

#[tokio::test]
async fn a_main_jail_that_exits_leaves_the_registry() {
    // Given
    let manager = SandboxSessionManager::new();
    let (state, _dir) = a_jail_running(EXITS_3);

    // When a main jail whose runner exits on its own is registered
    manager.insert("jailed-gone".to_string(), state).await;

    // Then it is forgotten once its runner has exited
    eventually_awaiting(
        "the exited main jail to leave the registry",
        OBSERVE,
        || async {
            manager
                .get("jailed-gone")
                .await
                .is_none()
                .then_some(())
                .ok_or_else(|| "still registered".to_string())
        },
    )
    .await;
}
