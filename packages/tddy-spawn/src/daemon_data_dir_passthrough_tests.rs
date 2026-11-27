//! daemon data dir passthrough tests over real fixtures on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use super::{spawn_as_user, LiveKitCreds, SpawnOptions, StartupWatch};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tddy_testing_commons::stub_scripts::{a_stub_agent_script, read_recorded_argv_value_after};
use tddy_testing_commons::wait::eventually_blocking;

/// These tests assert what argv the child received, not how long it was watched — so the
/// startup watch is kept to the minimum that still proves the child did not die on its
/// arguments.
const A_BRIEF_STARTUP_WATCH: StartupWatch = StartupWatch {
    grace: Duration::from_millis(100),
    poll: Duration::from_millis(25),
};

/// Safety net, not a prediction: a stub that has not recorded its argv within this window is
/// a stub that was never spawned. The wait costs nothing when the record is already there.
const A_STUB_RECORDS_ITS_ARGV_WITHIN: Duration = Duration::from_secs(10);

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

/// Writes an executable script at `<dir>/target/debug/tddy-coder` that records the argv it
/// was actually invoked with to `argv_file`, then sleeps rather than exiting immediately so it
/// survives `spawn_as_user`'s startup grace-period check — these tests are about what argv the
/// child *received*, not about post-spawn liveness.
fn a_fake_tddy_coder_build_that_records_its_argv(dir: &Path, argv_file: &Path) {
    let script_dir = dir.join("target").join("debug");
    std::fs::create_dir_all(&script_dir).unwrap();
    a_stub_agent_script(&script_dir, "tddy-coder")
        .recording_argv_to(argv_file)
        .then_sleeping_secs(2)
        .build();
}

/// The value the child received after `flag` (e.g. `--tddy-data-dir`).
///
/// `spawn_as_user` returns once the child has survived its startup grace period, which says
/// nothing about whether the child's *first* statement has run yet — under a loaded machine
/// the shell can still be starting up. Poll for the record rather than reading once.
fn recorded_argv_value_after(argv_file: &Path, flag: &str) -> String {
    eventually_blocking(
        &format!("the spawned child to record {flag} in its argv"),
        A_STUB_RECORDS_ITS_ARGV_WITHIN,
        || read_recorded_argv_value_after(argv_file, flag),
    )
}

/// RAII guard: switches the test process's cwd for the duration of the test (simulating
/// "wherever `./web-dev` launched the daemon from") and restores the original cwd on drop —
/// including when the test panics — so a failing assertion never corrupts the cwd for
/// whichever test `#[serial]` runs next.
struct CwdGuard(PathBuf);
impl CwdGuard {
    fn switch_to(dir: &Path) -> Self {
        let original = std::env::current_dir().expect("read current_dir");
        std::env::set_current_dir(dir).expect("switch current_dir for test");
        Self(original)
    }
}
impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
#[serial]
fn spawn_as_user_passes_the_daemons_tddy_data_dir_to_the_child_as_an_absolute_path_argument() {
    // Given — an absolute tddy_data_dir (an operator override, or the daemon's already-
    // resolved home) that is distinct from both the daemon's own toolchain root and the
    // target session's repo_path
    let daemon_toolchain_root = tempfile::tempdir().unwrap();
    let target_repo = tempfile::tempdir().unwrap();
    let tddy_data_dir = tempfile::tempdir().unwrap();
    let argv_file = target_repo.path().join("captured-argv.txt");
    a_fake_tddy_coder_build_that_records_its_argv(daemon_toolchain_root.path(), &argv_file);
    let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

    let os_user = current_username();

    // When
    let result = spawn_as_user(
        &os_user,
        "target/debug/tddy-coder",
        tddy_data_dir.path(),
        target_repo.path(),
        &a_livekit_creds(),
        SpawnOptions::default(),
        "info",
        super::CHILD_LOG_FORMAT_FALLBACK,
        None,
        A_BRIEF_STARTUP_WATCH,
    );
    result.expect("spawn_as_user must succeed");

    // Then — the child receives the daemon's own tddy_data_dir verbatim, regardless of
    // repo_path; it must never fall back to deriving session storage from its own cwd
    let received = recorded_argv_value_after(&argv_file, "--tddy-data-dir");
    assert_eq!(received, tddy_data_dir.path().to_str().unwrap());
}

#[test]
#[serial]
fn spawn_as_user_resolves_a_relative_tddy_data_dir_against_the_daemons_own_cwd_not_the_target_repo()
{
    // Given — the shape `default_tddy_data_dir()` actually returns for a debug build
    // ("tmp/.tddy", relative) when no operator override is configured; the target session's
    // repo_path is a completely unrelated directory the child's cwd gets set to
    let daemon_toolchain_root = tempfile::tempdir().unwrap();
    let target_repo = tempfile::tempdir().unwrap();
    let argv_file = target_repo.path().join("captured-argv.txt");
    a_fake_tddy_coder_build_that_records_its_argv(daemon_toolchain_root.path(), &argv_file);
    let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

    let os_user = current_username();

    // When
    let result = spawn_as_user(
        &os_user,
        "target/debug/tddy-coder",
        Path::new("tmp/.tddy"),
        target_repo.path(),
        &a_livekit_creds(),
        SpawnOptions::default(),
        "info",
        super::CHILD_LOG_FORMAT_FALLBACK,
        None,
        A_BRIEF_STARTUP_WATCH,
    );
    result.expect("spawn_as_user must succeed");

    // Then — resolved against the daemon's own cwd, never against repo_path (which would
    // silently split session storage between the daemon and the child it spawned).
    // Canonicalize the expected side too: production resolves the daemon's cwd via
    // `std::env::current_dir()`, which returns the OS-canonicalized path (e.g. macOS
    // resolves `/tmp` to `/private/tmp`), while `daemon_toolchain_root.path()` here is the
    // raw, uncanonicalized `tempfile::TempDir` path — both name the same directory.
    let received = recorded_argv_value_after(&argv_file, "--tddy-data-dir");
    let expected = daemon_toolchain_root
        .path()
        .canonicalize()
        .expect("canonicalize daemon toolchain root")
        .join("tmp/.tddy");
    assert_eq!(received, expected.to_str().unwrap());
}
