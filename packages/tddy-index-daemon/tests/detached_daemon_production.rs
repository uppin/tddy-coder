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

/// This suite's own runtime directory, which stops whatever daemon the script started in it when
/// the test ends — passing or panicking.
///
/// A panic between a start and its stop would otherwise leak a detached daemon: it has a session
/// of its own, so nothing the test runner tears down reaches it. The directory is removed after
/// the stop, because the stop reads the pid file inside it.
struct ASuiteRuntime {
    root: PathBuf,
    directory: tempfile::TempDir,
}

impl ASuiteRuntime {
    fn new() -> Self {
        Self {
            root: repo_root(),
            directory: tempfile::tempdir().expect("a runtime directory of this suite's own"),
        }
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }
}

impl Drop for ASuiteRuntime {
    fn drop(&mut self) {
        stop_the_daemon(&self.root, self.directory.path());
    }
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
    let runtime = ASuiteRuntime::new();

    // Given the script run in a process group of its own, which is what makes the teardown below
    // a teardown of *its* group and not of this test runner's
    let started = Command::new("./run-index-daemon")
        // These are about the daemon's lifetime, not its index: warming this checkout would add
        // minutes of crate-graph load to each of them.
        .arg("--no-warm")
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
    assert!(
        ping_succeeds(&socket),
        "the daemon died with the process group of the shell that started it — it was not given \
         a session of its own"
    );
}

#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn status_refuses_a_socket_with_no_listener_behind_it() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a daemon that was started and then stopped, which leaves the pid file's claim stale
    let announced = Command::new("./run-index-daemon")
        // These are about the daemon's lifetime, not its index: warming this checkout would add
        // minutes of crate-graph load to each of them.
        .arg("--no-warm")
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
    let runtime = ASuiteRuntime::new();

    // Given a daemon the script started
    let announced = Command::new("./run-index-daemon")
        // These are about the daemon's lifetime, not its index: warming this checkout would add
        // minutes of crate-graph load to each of them.
        .arg("--no-warm")
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
///
/// **This cannot force the race it guards.** The bug showed only when the readiness loop's first
/// tick ran before the background job was scheduled, and nothing outside the script can arrange
/// that ordering — so with the bug back, this test still passes whenever the job happens to win.
/// A failure here is proof of the bug; a pass is evidence against it, not proof. The fix itself
/// (truncating before the launch, in `run-index-daemon`) is what removes the race, and this stays
/// as the regression check that catches it on the runs where the scheduler exposes it.
#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn a_restart_announces_the_daemon_it_started_not_the_previous_ones_log() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a daemon that was started and stopped, leaving its `listening on` line in the log
    let first = Command::new("./run-index-daemon")
        // These are about the daemon's lifetime, not its index: warming this checkout would add
        // minutes of crate-graph load to each of them.
        .arg("--no-warm")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(
        first.status.success(),
        "the first start did not announce its daemon: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    stop_the_daemon(&root, runtime.path());

    // When the script starts it again
    let second = Command::new("./run-index-daemon")
        // These are about the daemon's lifetime, not its index: warming this checkout would add
        // minutes of crate-graph load to each of them.
        .arg("--no-warm")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .stdin(Stdio::null())
        .output()
        .expect("restart the script");
    let socket = exported_socket(&String::from_utf8_lossy(&second.stdout));
    let answering = ping_succeeds(&socket);
    let pid = recorded_pid(runtime.path());

    // Then it announced a daemon that answers, and recorded that daemon's pid
    assert!(
        second.status.success(),
        "the restart did not announce its daemon: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(answering, "the announced daemon does not answer");
    assert!(!pid.is_empty(), "no pid was recorded for the daemon");
}

/// A directory holding this checkout's own daemon beside a stand-in `tddy-tools` that writes down
/// how it was invoked and exits with `exit_code`.
///
/// What is under test is the script's warm step — what it runs, where, against which socket, and
/// what it makes of the answer — not the client, which has its own suite. A stand-in keeps that
/// deterministic: the real client would load this whole checkout's crate graph, for minutes.
/// `TDDY_INDEX_DAEMON_BIN` pointing here is also how the script learns where its client is, since
/// it uses the one beside the daemon.
struct APrebuiltDaemonWithAStandInClient {
    directory: tempfile::TempDir,
}

impl APrebuiltDaemonWithAStandInClient {
    fn exiting_with(exit_code: i32) -> Self {
        let directory = tempfile::tempdir().expect("a directory for the prebuilt binaries");
        std::fs::copy(
            the_index_daemon(),
            directory.path().join("tddy-index-daemon"),
        )
        .expect("copy the daemon");
        let client = directory.path().join("tddy-tools");
        std::fs::write(
            &client,
            format!(
                "#!/bin/sh\necho \"$* in $(pwd -P) socket=$TDDY_INDEX_SOCKET\" >> \"$0.invoked\"\nexit {exit_code}\n"
            ),
        )
        .expect("write the stand-in client");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755))
            .expect("make the stand-in client executable");
        Self { directory }
    }

    fn daemon(&self) -> PathBuf {
        self.directory.path().join("tddy-index-daemon")
    }

    /// What the stand-in client was invoked with, one line per invocation; empty if it never ran.
    fn invocations(&self) -> String {
        std::fs::read_to_string(self.directory.path().join("tddy-tools.invoked"))
            .unwrap_or_default()
    }
}

#[test]
#[ignore = "runs the real script — nix develop, so minutes and a toolchain; needs the daemon built"]
fn warms_the_checkout_on_the_daemon_it_started_and_still_exports_when_the_warm_fails() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a prebuilt daemon beside a client whose warm fails
    let prebuilt = APrebuiltDaemonWithAStandInClient::exiting_with(1);

    // When the script starts the daemon
    let started = Command::new("./run-index-daemon")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .env("TDDY_INDEX_DAEMON_BIN", prebuilt.daemon())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    let stdout = String::from_utf8_lossy(&started.stdout);
    let stderr = String::from_utf8_lossy(&started.stderr);
    let socket = exported_socket(&stdout);

    // Then the client was asked to warm this checkout against that daemon's socket
    let expected = format!(
        "restructure warm in {} socket={}",
        root.canonicalize().expect("the repo root").display(),
        socket.display()
    );
    assert_eq!(prebuilt.invocations().trim(), expected);

    // And the failed warm changed nothing the caller reads: the daemon is up, the one line on stdout
    // is the export, and the script succeeded — with the failure said on stderr
    assert!(started.status.success(), "the script failed: {stderr}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "stdout was not one line: {stdout}"
    );
    assert!(ping_succeeds(&socket), "the daemon does not answer");
    assert!(
        stderr.contains("The warm failed"),
        "the failed warm was not reported on stderr: {stderr}"
    );
}

#[test]
#[ignore = "runs the real script — nix develop, so minutes and a toolchain; needs the daemon built"]
fn leaves_the_crate_graph_unloaded_when_told_not_to_warm() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a prebuilt daemon beside a client that would record being run
    let prebuilt = APrebuiltDaemonWithAStandInClient::exiting_with(0);

    // When the script starts the daemon with `--no-warm`
    let started = Command::new("./run-index-daemon")
        .arg("--no-warm")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .env("TDDY_INDEX_DAEMON_BIN", prebuilt.daemon())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");

    // Then the daemon is up and the client never ran
    assert!(
        started.status.success(),
        "the script failed: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert_eq!(prebuilt.invocations(), "", "`--no-warm` still warmed");
}

/// The record the script and the daemon share, the only `*.spawns.jsonl` in `runtime`.
fn the_spawn_record_in(runtime: &Path) -> PathBuf {
    std::fs::read_dir(runtime)
        .expect("read the runtime directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| path.to_string_lossy().ends_with(".spawns.jsonl"))
        .expect("the script gave the daemon a spawn record in its runtime directory")
}

/// Every line of the record, parsed.
fn the_lines_of(record: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(record)
        .expect("read the spawn record")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of the record is JSON"))
        .collect()
}

/// The `end` line of the daemon's own entry, once the watcher has written it, or `None` if it does
/// not appear within `within`.
fn the_daemons_end_within(record: &Path, within: std::time::Duration) -> Option<serde_json::Value> {
    let deadline = std::time::Instant::now() + within;
    loop {
        let lines = the_lines_of(record);
        let daemon_id = lines
            .iter()
            .find(|line| line["event"] == "start" && line["purpose"] == "index-daemon")
            .map(|line| line["id"].clone());
        let end = daemon_id.and_then(|id| {
            lines
                .into_iter()
                .find(|line| line["event"] == "end" && line["id"] == id)
        });
        if end.is_some() || std::time::Instant::now() >= deadline {
            return end;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// A daemon killed with `SIGKILL` can run no handler, so the only thing that can say how it died
/// is the process that started it.
#[test]
#[ignore = "runs the real script — nix develop, so minutes and a toolchain; needs the daemon built"]
fn a_daemon_killed_with_sigkill_leaves_an_exit_line_naming_the_signal() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a daemon the script started
    let prebuilt = APrebuiltDaemonWithAStandInClient::exiting_with(0);
    let started = Command::new("./run-index-daemon")
        .arg("--no-warm")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .env("TDDY_INDEX_DAEMON_BIN", prebuilt.daemon())
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(
        started.status.success(),
        "the script did not start a daemon: {}",
        String::from_utf8_lossy(&started.stderr)
    );

    // When the daemon is killed with SIGKILL, by the pid the script recorded for it
    let killed = Command::new("kill")
        .args(["-9", &recorded_pid(runtime.path())])
        .status()
        .expect("run kill");
    assert!(killed.success(), "the daemon could not be killed");

    // Then within five seconds the record ends the daemon's entry with the signal that killed it
    let end = the_daemons_end_within(
        &the_spawn_record_in(runtime.path()),
        std::time::Duration::from_secs(5),
    )
    .expect("no end line for the daemon within five seconds of its death");
    assert_eq!(end["outcome"]["signal"], 9, "{end}");
}

#[test]
#[ignore = "runs the real script — nix develop and a cargo build, so minutes and a toolchain"]
fn an_orderly_stop_leaves_an_exit_line_with_status_zero_and_the_script_side_spawns_are_recorded() {
    let root = repo_root();
    let runtime = ASuiteRuntime::new();

    // Given a daemon the script started, building it first since no prebuilt one is named
    let started = Command::new("./run-index-daemon")
        .arg("--no-warm")
        .current_dir(&root)
        .env("TDDY_INDEX_RUNTIME_DIR", runtime.path())
        .env_remove("TDDY_INDEX_DAEMON_BIN")
        .stdin(Stdio::null())
        .output()
        .expect("start the script");
    assert!(
        started.status.success(),
        "the script did not start a daemon: {}",
        String::from_utf8_lossy(&started.stderr)
    );

    // When it is stopped in an orderly way
    stop_the_daemon(&root, runtime.path());

    // Then the record ends the daemon's entry with exit status zero
    let record = the_spawn_record_in(runtime.path());
    let end = the_daemons_end_within(&record, std::time::Duration::from_secs(5))
        .expect("no end line for the daemon within five seconds of its stop");
    assert_eq!(end["outcome"]["exit"], 0, "{end}");
    // And the script-side spawns are in the record: the build, the dev-shell capture, the launch
    let purposes: Vec<String> = the_lines_of(&record)
        .iter()
        .filter(|line| line["event"] == "start")
        .map(|line| line["purpose"].as_str().unwrap_or_default().to_string())
        .collect();
    for expected in ["build-daemon", "capture-dev-shell-env", "index-daemon"] {
        assert!(
            purposes.iter().any(|purpose| purpose == expected),
            "no `{expected}` start in {purposes:?}"
        );
    }
}
