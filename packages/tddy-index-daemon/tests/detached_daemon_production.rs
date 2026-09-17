//! `run-index-daemon` leaves a daemon that outlives the shell that started it.
//!
//! The property only exists between processes, so nothing smaller than the real script can show
//! it. A background job started with `nohup` alone stays in the process group of the shell that
//! launched it, and an agent harness, a CI step or a job-control terminal tearing that group down
//! takes the daemon with it — which is how several starts announced `listening on …` and then
//! refused the connection seconds later, with the socket file still on disk. `setsid` is the fix,
//! and this is what proves it.
//!
//! `#[ignore]`d, by [`docs/dev/guides/testing.md`] § Production Tests: the script runs
//! `nix develop` and `cargo build`, so this needs a toolchain and minutes rather than
//! milliseconds.
//!
//! ```bash
//! ./dev cargo test -p tddy-index-daemon --test detached_daemon_production -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` for the reason the warm suite states: these start real daemons.
//!
//! The suite gives itself its own `TDDY_INDEX_RUNTIME_DIR`, so the socket and pid file are not the
//! ones a developer's own `./run-index-daemon` is using. Without it, the teardown here would stop
//! the daemon they were working against.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The repo root: this crate is `packages/tddy-index-daemon`, so the root is two levels up.
fn repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    root
}

/// The binary the ping runs through, resolved the way this repo resolves a sibling binary.
fn the_index_daemon() -> PathBuf {
    if let Some(from_cargo) = std::env::var_os("CARGO_BIN_EXE_tddy-index-daemon") {
        return PathBuf::from(from_cargo);
    }
    let mut candidate = std::env::current_exe().expect("the test executable's own path");
    candidate.pop();
    if candidate.ends_with("deps") {
        candidate.pop();
    }
    candidate.join("tddy-index-daemon")
}

/// The socket the script exported, read from the one line it puts on stdout.
fn exported_socket(stdout: &str) -> PathBuf {
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix("export TDDY_INDEX_SOCKET="))
        .expect("the script exports a socket on stdout");
    PathBuf::from(line.trim())
}

fn ping_succeeds(socket: &Path) -> bool {
    Command::new(the_index_daemon())
        .arg("--ping")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run the ping")
        .success()
}

fn stop_the_daemon(root: &Path, runtime: &Path) {
    let _ = Command::new("./run-index-daemon")
        .arg("--stop")
        .current_dir(root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn the_daemon_outlives_the_shell_that_started_it() {
    use std::os::unix::process::CommandExt;

    let root = repo_root();
    let runtime = tempfile::tempdir().expect("a runtime directory of this suite's own");

    // Given the script run in a process group of its own, which is what makes the teardown below
    // a teardown of *its* group and not of this test runner's
    let started = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("start the script");
    let group = started.id();

    let announced = started
        .wait_with_output()
        .expect("the script announces and exits");
    assert!(
        announced.status.success(),
        "the script did not start a daemon: {}",
        String::from_utf8_lossy(&announced.stderr)
    );
    let socket = exported_socket(&String::from_utf8_lossy(&announced.stdout));

    // When everything in the starting shell's process group is torn down, which is what a harness
    // does when the command it ran has finished
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(format!("-{group}"))
        .stderr(Stdio::null())
        .status();

    // Then the daemon is still answering, because it never belonged to that group
    let answering = ping_succeeds(&socket);
    stop_the_daemon(&root, runtime.path());

    assert!(
        answering,
        "the daemon died with the process group of the shell that started it — it was not given \
         a session of its own"
    );
}

#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn status_refuses_a_socket_with_no_listener_behind_it() {
    let root = repo_root();
    let runtime = tempfile::tempdir().expect("a runtime directory of this suite's own");

    // Given a daemon that was started and then stopped, which leaves the pid file's claim stale
    let announced = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(announced.status.success());
    let socket = exported_socket(&String::from_utf8_lossy(&announced.stdout));
    stop_the_daemon(&root, runtime.path());

    // And a socket file left where the daemon had bound one
    std::fs::write(&socket, b"").expect("leave a socket file behind");

    // When the status is asked
    let status = Command::new("./run-index-daemon")
        .arg("--status")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("ask the status");

    // Then it reports a daemon that is not there as not there, rather than trusting the file
    assert!(
        !status.status.success(),
        "a socket with no listener was reported healthy: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}
