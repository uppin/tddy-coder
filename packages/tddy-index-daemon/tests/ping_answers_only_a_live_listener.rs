//! `--ping` proves a listener, which is the one thing a pid cannot.
//!
//! `run-index-daemon --status` used to answer on `kill -0` plus the socket file existing, and both
//! can be true of a daemon that is not serving: a socket file outlives the process that bound it,
//! and a pid proves only that something with that number is alive. The gap had teeth — several
//! starts announced `listening on …`, were reported healthy, and refused the next connection.
//!
//! The binary is spawned rather than called into, because what is under test is what a process
//! exits with. Neither suite reaches rust-analyzer: `Workspaces` answers from the registry the
//! daemon already holds, so a ping costs a connect and a round trip rather than a crate graph.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Long enough for a connect and one round trip on a loaded machine, short enough that a hang
/// fails the suite rather than stalling it.
const A_PING_SHOULD_ANSWER_WITHIN: Duration = Duration::from_secs(20);

/// The binary under test, resolved the way this repo resolves a sibling binary: the cargo-provided
/// path first, then a sibling of the test executable, with the `deps/` hop an integration test
/// needs.
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

/// A socket path inside a temporary directory, short enough for the `sun_path` limit.
fn a_socket_path(home: &tempfile::TempDir) -> PathBuf {
    home.path().join("s.sock")
}

fn ping(socket: &std::path::Path) -> std::process::Output {
    Command::new(the_index_daemon())
        .arg("--ping")
        .arg(socket)
        .stdin(Stdio::null())
        .output()
        .expect("run the ping")
}

#[test]
fn refuses_a_socket_path_that_nothing_is_listening_on() {
    // Given a socket file a previous daemon left behind, with no process behind it
    let home = tempfile::tempdir().expect("a temporary home");
    let socket = a_socket_path(&home);
    std::fs::write(&socket, b"").expect("a leftover socket file");

    // When it is pinged
    let outcome = ping(&socket);

    // Then the run fails, naming the socket nothing answered on — not merely exiting non-zero,
    // which an unrecognised flag would also do
    assert_refused_to_reach(&outcome, &socket);
}

#[test]
fn refuses_a_socket_path_that_does_not_exist() {
    // Given a path no daemon has ever bound
    let home = tempfile::tempdir().expect("a temporary home");
    let socket = a_socket_path(&home);

    // When it is pinged
    let outcome = ping(&socket);

    // Then the run fails the same way, for the same reason
    assert_refused_to_reach(&outcome, &socket);
}

/// A refusal that reached the socket and found nothing, as distinct from a command line the binary
/// did not understand. Both exit non-zero, and only one of them is this suite's subject.
fn assert_refused_to_reach(outcome: &std::process::Output, socket: &std::path::Path) {
    let said = String::from_utf8_lossy(&outcome.stderr);

    assert!(
        !outcome.status.success(),
        "a socket with no listener was reported healthy: {said}"
    );
    assert!(
        said.contains(&socket.to_string_lossy().to_string()),
        "the refusal did not name the socket it could not reach: {said}"
    );
    assert!(
        said.contains("no index daemon answered"),
        "the refusal did not say a ping went unanswered, so it may be the command line \
         the binary rejected rather than the socket: {said}"
    );
}

#[test]
fn answers_a_daemon_that_is_serving_the_socket() {
    // Given a daemon serving on an AF_UNIX socket
    let home = tempfile::tempdir().expect("a temporary home");
    let socket = a_socket_path(&home);
    let mut daemon = Command::new(the_index_daemon())
        .arg("--grpc-uds")
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the daemon");

    let bound = Instant::now();
    while !socket.exists() && bound.elapsed() < A_PING_SHOULD_ANSWER_WITHIN {
        std::thread::sleep(Duration::from_millis(50));
    }

    // When it is pinged
    let outcome = ping(&socket);

    let _ = daemon.kill();
    let _ = daemon.wait();

    // Then the ping succeeds, and says which socket answered
    assert!(
        outcome.status.success(),
        "a serving daemon was reported unreachable: {}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    assert!(
        String::from_utf8_lossy(&outcome.stdout).contains(&socket.to_string_lossy().to_string()),
        "the answer named no socket: {}",
        String::from_utf8_lossy(&outcome.stdout)
    );
}
