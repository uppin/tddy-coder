//! Handing a privileged listening socket to an unprivileged managed service.
//!
//! This is the job `tddy-daemon.socket` used to do: systemd bound `/run/tddy-daemon.sock` as root
//! and passed the fd, so the unprivileged daemon never had to write into `/run`. With the daemon
//! demoted to a supervisor child it is no longer a systemd service, and without this the daemon
//! self-binds and gets `EACCES`.
//!
//! The daemon already implements the receiving half — `resolve_socket_source` adopts
//! `SD_LISTEN_FDS_START` when `LISTEN_PID` names it — so the contract asserted here is deliberately
//! systemd's, not a new one.
//!
//! Every test here needs a service the supervisor actually started, so the whole file is Linux-only
//! — see the note in `support`'s header.
#![cfg(target_os = "linux")]

mod support;

use std::os::unix::fs::{FileTypeExt, PermissionsExt};

use support::{a_service, a_supervisor};
use tddy_supervisor::ServiceState;

#[tokio::test]
async fn binds_a_declared_service_socket_before_the_service_starts() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;

    // When
    supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await;

    // Then — the socket has to exist before the service does, or a client that races startup gets
    // ECONNREFUSED against a path that is about to work.
    let socket = supervisor.declared_socket_path("tddy-daemon");
    let metadata = std::fs::metadata(&socket)
        .unwrap_or_else(|error| panic!("stat {}: {error}", socket.display()));
    assert!(
        metadata.file_type().is_socket(),
        "expected {} to be a unix socket",
        socket.display()
    );
}

#[tokio::test]
async fn creates_the_declared_service_socket_with_the_configured_mode() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;
    supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await;

    // When
    let socket = supervisor.declared_socket_path("tddy-daemon");
    let mode = std::fs::metadata(&socket)
        .expect("stat the declared socket")
        .permissions()
        .mode()
        & 0o777;

    // Then — the mode is the entire access grant on a root-owned socket.
    assert_eq!(mode, 0o660, "socket mode of {}", socket.display());
}

#[tokio::test]
async fn passes_the_listener_to_the_service_as_file_descriptor_three() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;

    // When
    let report = supervisor.await_handoff_report("tddy-daemon").await;

    // Then — asked of the kernel rather than inferred from LISTEN_FDS.
    assert_eq!(report.get("fd3").map(String::as_str), Some("socket"));
    assert_eq!(report.get("listen_fds").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn tells_the_service_the_listener_belongs_to_it_and_not_to_its_parent() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;

    // When
    let report = supervisor.await_handoff_report("tddy-daemon").await;

    // Then — `LISTEN_PID` must be the service's own pid. The daemon checks it against its own pid
    // precisely because the variables are inherited, and a child that trusted a parent's
    // `LISTEN_PID` would adopt whatever its fd 3 happened to be.
    assert_eq!(
        report.get("listen_pid"),
        report.get("own_pid"),
        "LISTEN_PID must name the service itself, got {report:?}"
    );
}

#[tokio::test]
async fn hands_no_listener_to_a_service_that_declared_no_socket() {
    // Given a service with the reporting body but no socket declaration
    let supervisor = a_supervisor()
        .managing(
            a_service("tddy-relay")
                .with_a_listening_socket()
                .declaring_no_socket(),
        )
        .start()
        .await;

    // When
    let report = supervisor.await_handoff_report("tddy-relay").await;

    // Then — an undeclared socket is not a privilege the supervisor hands out by accident, and a
    // stale `LISTEN_FDS` inherited from the supervisor's own activation must not leak through.
    assert_eq!(report.get("fd3").map(String::as_str), Some("absent"));
    assert_eq!(report.get("listen_fds").map(String::as_str), Some("unset"));
}

#[tokio::test]
async fn rebinds_the_declared_socket_when_the_service_is_restarted() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").with_a_listening_socket())
        .start()
        .await;
    let original = supervisor
        .await_service_state("tddy-daemon", ServiceState::Running)
        .await
        .pid
        .expect("running service has a pid");

    supervisor.clear_handoff_report("tddy-daemon");

    // When the service is killed and the supervisor restarts it
    // SAFETY: the supervisor just reported this pid running and reaps it itself, so it cannot have
    // been recycled before this call.
    unsafe { libc::kill(original as i32, libc::SIGKILL) };
    supervisor
        .await_service_restart("tddy-daemon", original)
        .await;

    // Then the replacement gets a working listener too — a socket that only survives the first
    // start would make every restart a silent outage.
    let report = supervisor.await_handoff_report("tddy-daemon").await;
    assert_eq!(report.get("fd3").map(String::as_str), Some("socket"));
}
