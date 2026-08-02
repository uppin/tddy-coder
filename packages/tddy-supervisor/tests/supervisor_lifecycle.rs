//! The supervisor as a mini-init: it starts declared services, keeps them alive, gives up on
//! ones that will not stay up, and takes them down with it.

mod support;

use support::{
    a_service, a_supervisor, process_is_alive, ServiceListAssertions, ServiceStatusAssertions,
};
use tddy_supervisor::ServiceState;

#[tokio::test]
async fn starts_every_declared_service_and_reports_it_running() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .start()
        .await;

    // When
    let status = supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await;

    // Then
    status
        .assert_named("tddy-daemon")
        .assert_running_with_a_live_pid()
        .assert_restart_count(0);
}

#[tokio::test]
async fn reports_every_declared_service_in_declaration_order() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .managing(a_service("tddy-relay").that_stays_alive())
        .start()
        .await;

    // When
    let services = supervisor
        .client()
        .await
        .list_services()
        .await
        .expect("list declared services");

    // Then
    services.assert_names_in_order(&["tddy-daemon", "tddy-relay"]);
}

#[tokio::test]
async fn restarts_a_managed_service_that_exits_and_reports_a_new_pid() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .start()
        .await;
    let original = supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await
        .pid();

    // When
    // SAFETY: killing a pid the supervisor just reported as running; it is reaped by the
    // supervisor, not by us, so the pid cannot have been recycled before this call.
    unsafe { libc::kill(original as i32, libc::SIGKILL) };
    let restarted = supervisor
        .await_service_restart("tddy-daemon", original)
        .await;

    // Then
    restarted
        .assert_running_with_a_live_pid()
        .assert_restart_count(1);
    assert_ne!(
        restarted.pid(),
        original,
        "the restarted service reused the dead pid"
    );
}

#[tokio::test]
async fn stops_restarting_a_service_once_the_retry_ceiling_is_reached() {
    // Given
    let supervisor = a_supervisor()
        .managing(
            a_service("doomed")
                .that_exits_immediately()
                .with_max_retries(2)
                .with_initial_backoff_ms(10),
        )
        .start()
        .await;

    // When
    let status = supervisor
        .await_service_state("doomed", ServiceState::GaveUp)
        .await;

    // Then
    status.assert_has_no_pid().assert_restart_count(2);
    assert_eq!(
        supervisor.recorded_starts("doomed").len(),
        3,
        "expected the initial start plus exactly two retries"
    );
}

#[tokio::test]
async fn terminates_every_managed_service_when_the_supervisor_is_asked_to_shut_down() {
    // Given
    let mut supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .start()
        .await;
    let managed_pid = supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await
        .pid();

    // When
    supervisor.terminate().await;

    // Then
    assert!(
        !process_is_alive(managed_pid),
        "managed service {managed_pid} outlived the supervisor"
    );
}
