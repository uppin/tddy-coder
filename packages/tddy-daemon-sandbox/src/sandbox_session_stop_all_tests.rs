//! `SandboxSessionManager::stop_all`: a daemon shutting down stops every session's **main** jail,
//! which it used to leave running
//! (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`, part 1).

use std::sync::Arc;

use pretty_assertions::assert_eq;

use crate::jail_test_support::{a_jail_running, SLEEPS};
use crate::sandbox_session::SandboxSessionManager;

#[tokio::test]
async fn stop_all_stops_every_main_jail_and_forgets_it() {
    // Given two sessions' main jails
    let manager = SandboxSessionManager::new();
    let (first, _first_dir) = a_jail_running(SLEEPS);
    let (second, _second_dir) = a_jail_running(SLEEPS);
    manager
        .insert("session-b".to_string(), Arc::clone(&second))
        .await;
    manager
        .insert("session-a".to_string(), Arc::clone(&first))
        .await;

    // When
    let stopped = manager.stop_all().await;

    // Then both runners are stopped, named in order, and neither session is registered any more
    assert_eq!(
        stopped,
        vec!["session-a".to_string(), "session-b".to_string()]
    );
    assert_eq!((first.is_running(), second.is_running()), (false, false));
    assert!(manager.get("session-a").await.is_none());
    assert!(manager.get("session-b").await.is_none());
}

#[tokio::test]
async fn stop_all_with_no_main_jail_stops_nothing() {
    // Given
    let manager = SandboxSessionManager::new();

    // When
    let stopped = manager.stop_all().await;

    // Then
    assert_eq!(stopped, Vec::<String>::new());
}
