//! How the daemon decides where privileged work goes, and what happens when the supervisor it
//! was told about is not there.

use std::path::PathBuf;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::supervisor_client::{
    connect_supervisor, spawn_backend_choice, SpawnBackendChoice,
};

fn a_daemon_config(yaml: &str) -> DaemonConfig {
    serde_yaml::from_str(yaml).expect("parse daemon config")
}

#[test]
fn delegates_spawning_to_the_supervisor_when_the_config_declares_a_socket() {
    // Given
    let config = a_daemon_config(
        r#"
supervisor:
  socket_path: /run/tddy-supervisor.sock
"#,
    );

    // When
    let choice = spawn_backend_choice(&config);

    // Then
    assert_eq!(
        choice,
        SpawnBackendChoice::Supervisor {
            socket_path: PathBuf::from("/run/tddy-supervisor.sock")
        }
    );
}

#[test]
fn keeps_the_forked_spawn_worker_when_no_supervisor_is_declared() {
    // Given
    let config = a_daemon_config("users: []\n");

    // When
    let choice = spawn_backend_choice(&config);

    // Then
    assert_eq!(choice, SpawnBackendChoice::ForkedWorker);
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
