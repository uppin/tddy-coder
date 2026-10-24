//! A managed service is told the supervisor's privileged socket, so `tddy-daemon` knows it was
//! started by a supervisor without a `supervisor:` block in its own config — the config drift that
//! left `mantas-udoo-ros` cloning through an unprivileged worker on 2026-10-24.
//!
//! Every test here needs a service or session the supervisor actually started, so the whole file is
//! Linux-only — see the note in `support`'s header.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-supervisor-handoff.md
#![cfg(target_os = "linux")]

mod support;

use support::{a_service, a_supervisor, a_tool_that_records_its_parent, current_username};
use tddy_supervisor::spawn_broker::SUPERVISOR_SOCKET_ENV;
use tddy_supervisor::SpawnSessionRequest;

#[tokio::test]
async fn a_managed_service_is_told_the_supervisor_socket() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;

    // When
    let report = supervisor.await_handoff_report("tddy-daemon").await;

    // Then
    assert_eq!(SUPERVISOR_SOCKET_ENV, "TDDY_SUPERVISOR_SOCKET");
    assert_eq!(
        report.get("supervisor_socket").map(String::as_str),
        Some(supervisor.socket_path().to_str().unwrap()),
        "the service is told where the supervisor listens: {report:?}"
    );
}

#[tokio::test]
async fn a_service_env_entry_cannot_override_the_supervisor_socket() {
    // Given — supervisor.yaml tries to point the daemon somewhere else
    let supervisor = a_supervisor()
        .managing(
            a_service("tddy-daemon")
                .with_a_listening_socket()
                .with_env("TDDY_SUPERVISOR_SOCKET", "/tmp/forged.sock"),
        )
        .start()
        .await;

    // When
    let report = supervisor.await_handoff_report("tddy-daemon").await;

    // Then — the supervisor's own value wins
    assert_eq!(
        report.get("supervisor_socket").map(String::as_str),
        Some(supervisor.socket_path().to_str().unwrap()),
        "a service's env: cannot forge the supervisor socket: {report:?}"
    );
}

#[tokio::test]
async fn a_session_spawn_is_not_told_the_supervisor_socket() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    supervisor
        .client()
        .await
        .spawn_session(SpawnSessionRequest {
            os_user: current_username(),
            tool_path: tool.path().to_path_buf(),
            args: Vec::new(),
            env: Default::default(),
            working_dir: None,
            scope: None,
        })
        .await
        .expect("spawn a session");

    // Then — only a managed service may reach the privileged socket by being told it
    assert_eq!(tool.await_recorded_supervisor_socket().await, "unset");
}
