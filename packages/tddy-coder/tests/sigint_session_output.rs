//! Integration tests: SIGINT handling and session info output.
//!
//! Verifies that when tddy-coder (stub backend) receives SIGINT (signal), it prints session info
//! to stderr before exiting (Session: <id> and Session dir: <path>).
//!
//! **Keyboard Ctrl+C** in the full-screen TUI: `tddy-coder` and `tddy-demo` share
//! [`tddy_tui::run_event_loop`]. In raw terminal mode, Ctrl+C is usually a key event (not SIGINT);
//! the event loop sets the same `shutdown` flag there so the process exits without hanging. Rebuild
//! the binary after `tddy-tui` changes: `cargo build -p tddy-coder`.

mod common;

use std::io::Read;
use std::process::Stdio;

#[allow(deprecated)]
fn tddy_coder_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("tddy-coder")
}

/// When tddy-coder receives SIGINT, stderr contains "Session:" (session dir or fallback).
///
/// Without SKIP_QUESTIONS the stub backend returns clarification questions,
/// causing the process to print "Clarification needed:" to stdout and block
/// on stdin. That stdout output proves the ctrlc handler is registered
/// (it happens after registration, inside the workflow loop). We wait for
/// it, then send SIGINT.
#[test]
#[cfg(unix)]
fn tddy_demo_sigint_prints_session_info_to_stderr() {
    let mut child = std::process::Command::new(tddy_coder_bin())
        .args([
            "--agent",
            "stub",
            "--recipe",
            "tdd",
            "--goal",
            "plan",
            "--prompt",
            "Build auth",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tddy-coder");

    // Wait for stdout output. When the stub returns clarification
    // questions, the process prints "Clarification needed:" to stdout and
    // blocks on stdin — this happens AFTER the ctrlc handler is registered.
    let mut stdout = child.stdout.take().expect("stdout");
    let mut buf = [0u8; 1];
    stdout
        .read_exact(&mut buf)
        .expect("read first byte of stdout");

    let pid = child.id() as i32;
    let _ = unsafe { libc::kill(pid, libc::SIGINT) };

    // Close stdin so the blocked read_line returns EOF, allowing the process to
    // reach its exit path and print session info.
    drop(child.stdin.take());

    // Drain remaining stdout so the child doesn't get a broken pipe.
    let _ = std::io::copy(&mut stdout, &mut std::io::sink());
    let _ = child.wait();

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");

    assert!(
        stderr.contains("Session:"),
        "stderr should contain 'Session:' on SIGINT, got: {}",
        stderr
    );
}
