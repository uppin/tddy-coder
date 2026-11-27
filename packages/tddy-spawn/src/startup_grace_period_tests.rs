//! startup grace period tests over real fixtures on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use super::{spawn_as_user, LiveKitCreds, SpawnOptions, StartupWatch};
use std::time::Duration;

/// Long enough that a loaded machine's fork/exec/exit never looks like a healthy start.
/// Costs a crashing child nothing: `try_wait` breaks out the moment it is gone, so this is a
/// ceiling on how long we would wait, not on how long we do.
const LONG_ENOUGH_TO_SEE_A_CRASH: StartupWatch = StartupWatch {
    grace: Duration::from_secs(10),
    poll: Duration::from_millis(25),
};

/// A watch a healthy spawn pays in full — kept short, because what is asserted is that a live
/// child is reported as started, not how long it was watched.
const A_BRIEF_WATCH: StartupWatch = StartupWatch {
    grace: Duration::from_millis(100),
    poll: Duration::from_millis(25),
};

fn current_username() -> String {
    std::env::var("USER").expect("USER env var must be set to run this test")
}

fn a_livekit_creds() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

/// Returns the path to the `false` binary — `/usr/bin/false` on macOS, `/bin/false` on Linux.
fn false_bin() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/bin/false"
    } else {
        "/bin/false"
    }
}

#[test]
fn spawning_a_binary_that_exits_immediately_returns_an_error_with_its_stderr() {
    // Given — `false` ignores every argument (including the --daemon/--grpc/... flags
    // spawn_as_user appends) and exits 1 immediately, simulating a child that crashes on
    // startup (e.g. an unknown --recipe value) rather than becoming a running session
    let tmp = tempfile::tempdir().unwrap();
    let tddy_data_dir = tempfile::tempdir().unwrap();
    let os_user = current_username();

    // When
    let result = spawn_as_user(
        &os_user,
        false_bin(),
        tddy_data_dir.path(),
        tmp.path(),
        &a_livekit_creds(),
        SpawnOptions::default(),
        "info",
        super::CHILD_LOG_FORMAT_FALLBACK,
        None,
        LONG_ENOUGH_TO_SEE_A_CRASH,
    );

    // Then — a startup crash is reported as an error, not a fake-success SpawnResult
    let err = result.expect_err("a child that exits immediately must not report success");
    assert!(
        err.to_string().contains("exited immediately"),
        "expected the error to explain the child exited during startup, got: {err}"
    );
}

/// A script that ignores every argument (spawn_as_user appends `--daemon --grpc ...`) and
/// just sleeps, standing in for a `tddy-coder` process that starts up successfully.
fn a_script_that_outlives_the_grace_period(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-long-running-tddy-coder.sh");
    std::fs::write(&path, "#!/bin/sh\nsleep 2\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn spawning_a_process_that_outlives_the_grace_period_returns_ok() {
    // Given — a process that stays alive well past the startup grace period
    let tmp = tempfile::tempdir().unwrap();
    let tddy_data_dir = tempfile::tempdir().unwrap();
    let os_user = current_username();
    let script = a_script_that_outlives_the_grace_period(tmp.path());

    // When
    let result = spawn_as_user(
        &os_user,
        script.to_str().unwrap(),
        tddy_data_dir.path(),
        tmp.path(),
        &a_livekit_creds(),
        SpawnOptions::default(),
        "info",
        super::CHILD_LOG_FORMAT_FALLBACK,
        None,
        A_BRIEF_WATCH,
    );

    // Then — spawn_as_user does not wait for the child to exit; it only guards against an
    // immediate crash, so a long-running child is reported as a successful spawn
    let spawned = result.expect("a long-running child must be reported as a successful spawn");
    assert!(spawned.pid > 0);
}