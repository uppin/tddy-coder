//! daemon toolchain resolution tests over real fixtures on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use super::{spawn_as_user, LiveKitCreds, SpawnOptions, StartupWatch};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// These tests assert what argv the child received, not how long it was watched — so the
/// startup watch is kept to the minimum that still proves the child did not die on its
/// arguments.
const A_BRIEF_STARTUP_WATCH: StartupWatch = StartupWatch {
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

/// Writes an executable script at `<dir>/target/debug/tddy-coder`, mirroring the relative
/// layout every real `dev.daemon.yaml` `allowed_tools` entry points at. Sleeps rather than
/// exiting immediately so it survives `spawn_as_user`'s startup grace-period check — this
/// test is about *locating* the binary, not about post-spawn liveness.
fn a_fake_tddy_coder_build_under(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script_dir = dir.join("target").join("debug");
    std::fs::create_dir_all(&script_dir).unwrap();
    let script_path = script_dir.join("tddy-coder");
    std::fs::write(&script_path, "#!/bin/sh\nsleep 2\n").unwrap();
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
fn spawn_as_user_locates_a_relative_tool_path_against_the_daemons_own_cwd_not_the_target_repo()
{
    // Given — the daemon's own toolchain root (simulated here by the process's cwd, matching
    // "wherever ./web-dev launched the daemon from") contains a tddy-coder build; the target
    // session's repo_path is a *completely unrelated* directory that contains no such build
    // at all — like the TODO-app project this recipe is meant to work against
    let daemon_toolchain_root = tempfile::tempdir().unwrap();
    a_fake_tddy_coder_build_under(daemon_toolchain_root.path());
    let target_repo = tempfile::tempdir().unwrap();
    let tddy_data_dir = tempfile::tempdir().unwrap();
    let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

    let os_user = current_username();

    // When — spawn_as_user receives the *relative* tool_path every real `allowed_tools`
    // entry actually uses ("target/debug/tddy-coder"), and a repo_path pointing at the
    // unrelated target project
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

    // Then — it finds and runs the daemon's own toolchain build, even though target_repo
    // has no tddy-coder build of its own; project selection must never change which binary
    // gets executed
    result.expect(
        "spawn_as_user must locate a relative tool_path against the daemon's own toolchain \
         root, not against the target session's repo_path",
    );
}