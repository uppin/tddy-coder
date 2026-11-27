//! run capture as user tests over real fixtures on disk.
//
// Test-only: included via `mod` under `#[cfg(all(test, unix))]`, so the filesystem-port
// guard reads the whole file as test code and the real-disk fixtures below are the tests'
// fixtures, not production I/O.
#![cfg(test)]
use super::run_capture_as_user;
use serial_test::serial;
use std::path::{Path, PathBuf};

fn current_username() -> String {
    std::env::var("USER").expect("USER env var must be set to run this test")
}

/// Writes an executable script at `<dir>/target/debug/tddy-tools` that prints a fixed marker
/// to stdout and exits immediately — `run_capture_as_user` waits for full completion
/// (`Command::output`), unlike `spawn_as_user`'s liveness-checked long-running child.
fn a_fake_tddy_tools_probe_under(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script_dir = dir.join("target").join("debug");
    std::fs::create_dir_all(&script_dir).unwrap();
    let script_path = script_dir.join("tddy-tools");
    std::fs::write(&script_path, "#!/bin/sh\necho probe-output-marker\n").unwrap();
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// RAII guard: switches the test process's cwd for the duration of the test (simulating
/// "wherever `./web-dev` launched the daemon from") and restores the original cwd on drop —
/// including when the test panics. Duplicated from `daemon_toolchain_resolution_tests` above
/// rather than shared, since Rust module privacy would otherwise require widening visibility
/// just for this small test-only helper.
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
fn run_capture_as_user_locates_a_relative_program_path_against_the_daemons_own_cwd_not_the_target_users_home(
) {
    // Given — the daemon's own toolchain root (simulated by the process's cwd, matching
    // "wherever ./web-dev launched the daemon from") contains a tddy-tools build; the target
    // OS user's home directory (whatever it really is on this machine) contains no such
    // build at all, since `run_capture_as_user` internally `.current_dir()`s there
    let daemon_toolchain_root = tempfile::tempdir().unwrap();
    a_fake_tddy_tools_probe_under(daemon_toolchain_root.path());
    let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

    let os_user = current_username();

    // When — run_capture_as_user receives the *relative* program path exactly as
    // `DaemonSessionHost::resolve_tddy_tools_path` produces it ("target/debug/tddy-tools")
    let result = run_capture_as_user(&os_user, Path::new("target/debug/tddy-tools"), &[]);

    // Then — it locates and runs the daemon's own toolchain build via the daemon's own cwd,
    // even though it chdirs into the target user's home before exec-ing; a relative program
    // path must never be silently re-resolved against wherever this function happens to chdir
    let output = result.expect(
        "run_capture_as_user must locate a relative program path against the daemon's own \
         cwd, not against the target OS user's home directory",
    );
    assert_eq!(output.trim(), "probe-output-marker");
}