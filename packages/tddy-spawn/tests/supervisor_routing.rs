//! How the daemon decides where privileged work goes, and what happens when the supervisor it
//! was told about is not there.
//!
//! The decision follows how the daemon was started: `tddy-supervisor` tells a managed service its
//! socket in `TDDY_SUPERVISOR_SOCKET`; anything else — the desktop app, `./install --user`, a
//! daemon run by hand — forks its own spawn worker. There is no config switch to drift from that.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-10-24-start-progress-supervisor-handoff.md

use std::ffi::OsString;
use std::path::PathBuf;

use tddy_spawn::supervisor_client::{connect_supervisor, SpawnBackendChoice};

fn an_environment_with(
    name: &'static str,
    value: &'static str,
) -> impl Fn(&str) -> Option<OsString> {
    move |asked| (asked == name).then(|| OsString::from(value))
}

fn an_empty_environment(_: &str) -> Option<OsString> {
    None
}

#[test]
fn a_daemon_told_of_a_supervisor_spawns_through_it() {
    // When
    let choice = SpawnBackendChoice::from_environment(an_environment_with(
        "TDDY_SUPERVISOR_SOCKET",
        "/run/tddy-supervisor.sock",
    ));

    // Then
    assert_eq!(
        choice,
        SpawnBackendChoice::Supervisor {
            socket_path: PathBuf::from("/run/tddy-supervisor.sock")
        }
    );
}

#[test]
fn a_daemon_not_told_of_a_supervisor_forks_its_own_worker() {
    assert_eq!(
        SpawnBackendChoice::from_environment(an_empty_environment),
        SpawnBackendChoice::ForkedWorker
    );
}

#[test]
fn a_supervised_daemon_consumes_the_socket_variable_so_its_own_children_do_not_inherit_it() {
    // When
    let adopted = SpawnBackendChoice::adopt_from_environment(an_environment_with(
        "TDDY_SUPERVISOR_SOCKET",
        "/run/tddy-supervisor.sock",
    ));

    // Then
    assert_eq!(
        adopted.choice,
        SpawnBackendChoice::Supervisor {
            socket_path: PathBuf::from("/run/tddy-supervisor.sock")
        }
    );
    assert_eq!(adopted.consumed_env, vec!["TDDY_SUPERVISOR_SOCKET"]);
}

#[test]
fn an_unsupervised_daemon_consumes_nothing_from_its_environment() {
    let adopted = SpawnBackendChoice::adopt_from_environment(an_empty_environment);

    assert_eq!(adopted.choice, SpawnBackendChoice::ForkedWorker);
    assert!(adopted.consumed_env.is_empty());
}

#[tokio::test]
async fn fails_to_reach_a_declared_supervisor_whose_socket_is_absent() {
    // Given a host configured for a supervisor that is not running.
    let workspace = tempfile::tempdir().expect("create workspace");
    let missing_socket = workspace.path().join("tddy-supervisor.sock");

    // When
    let result = connect_supervisor(&missing_socket).await;

    // Then the daemon surfaces the outage. It must not silently fall back to spawning the
    // session itself, which would run it as the daemon user with no isolation.
    let error = result.expect_err("connecting to an absent supervisor should fail");
    assert!(
        error
            .to_string()
            .contains(&missing_socket.display().to_string()),
        "error should name the unreachable socket, got: {error}"
    );
}
