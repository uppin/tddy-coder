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

/// The pid the script recorded for the daemon it started, from the only pid file in `runtime`.
fn recorded_pid(runtime: &Path) -> String {
    let pid_file = std::fs::read_dir(runtime)
        .expect("read the runtime directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| path.extension().is_some_and(|extension| extension == "pid"))
        .expect("the script wrote a pid file for the daemon it started");
    std::fs::read_to_string(pid_file)
        .expect("read the pid file")
        .trim()
        .to_string()
}

/// The `NAME=value` words of a running process's environment, as `ps` shows them.
fn environment_of(pid: &str) -> Vec<String> {
    let shown = Command::new("ps")
        .args(["eww", "-o", "command=", "-p", pid])
        .output()
        .expect("run ps");
    String::from_utf8_lossy(&shown.stdout)
        .split_whitespace()
        .filter(|word| word.contains('='))
        .map(str::to_string)
        .collect()
}

/// rust-analyzer runs every build script in the environment the daemon hands it. With the dev
/// shell's PATH alone, `webrtc-sys`'s build script and the `sqlx-macros` proc macro failed to link
/// inside it while the same `cargo check` passed in the shell, and the daemon served a degraded index
/// whose extract-methods came out as `req: _`.
#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn the_daemon_runs_with_the_dev_shells_whole_environment() {
    let root = repo_root();
    let runtime = tempfile::tempdir().expect("a runtime directory of this suite's own");

    // Given a daemon the script started
    let announced = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(
        announced.status.success(),
        "the script did not start a daemon: {}",
        String::from_utf8_lossy(&announced.stderr)
    );

    // When its environment is read
    let environment = environment_of(&recorded_pid(runtime.path()));
    stop_the_daemon(&root, runtime.path());

    // Then it is the dev shell's, with the temporary directory a `nix develop` deletes on exit
    // replaced — this suite itself runs under `./dev`, so the caller's TMPDIR is one of those too
    assert!(
        environment
            .iter()
            .any(|word| word.starts_with("IN_NIX_SHELL=")),
        "the daemon was not given the dev shell's environment: {environment:?}"
    );
    assert!(
        !environment
            .iter()
            .any(|word| word.starts_with("TMPDIR=") && word.contains("/nix-shell.")),
        "the daemon kept the TMPDIR `nix develop` deletes when it exits: {environment:?}"
    );
}

/// A restart reads a log the previous daemon left behind. Truncating it inside the background job
/// raced the readiness loop, which then found the old `listening on` line and announced a daemon
/// that had not started yet — with no pid file, so `--stop` had nothing to stop.
#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn a_restart_announces_the_daemon_it_started_not_the_previous_ones_log() {
    let root = repo_root();
    let runtime = tempfile::tempdir().expect("a runtime directory of this suite's own");

    // Given a daemon that was started and stopped, leaving its `listening on` line in the log
    let first = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(first.status.success());
    stop_the_daemon(&root, runtime.path());

    // When the script starts it again
    let second = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("restart the script");
    let socket = exported_socket(&String::from_utf8_lossy(&second.stdout));
    let answering = ping_succeeds(&socket);
    let pid = recorded_pid(runtime.path());
    stop_the_daemon(&root, runtime.path());

    // Then it announced a daemon that answers, and recorded that daemon's pid
    assert!(
        second.status.success(),
        "the restart did not announce its daemon: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(answering, "the announced daemon does not answer");
    assert!(!pid.is_empty(), "no pid was recorded for the daemon");
}
