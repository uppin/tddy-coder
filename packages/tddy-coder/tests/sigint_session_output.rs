//! Integration tests: SIGINT handling and session info output.
//!
//! Verifies that when tddy-coder (stub backend) receives SIGINT (Ctrl+C), it prints session info
//! to stderr before exiting (Session: <id> and Plan dir: <path>).

mod common;

use std::io::Read;
use std::process::Stdio;

#[allow(deprecated)]
fn tddy_coder_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("tddy-coder")
}

/// When tddy-coder receives SIGINT, stderr contains "Session:" (plan dir or fallback).
#[test]
#[cfg(unix)]
fn tddy_demo_sigint_prints_session_info_to_stderr() {
    let mut child = std::process::Command::new(tddy_coder_bin())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "Build auth SKIP_QUESTIONS",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tddy-coder");

    // Wait for the process to fully initialize (register ctrlc handler).
    // The debug binary can take several hundred milliseconds to load in CI/test
    // contexts due to dynamic library loading and framework initialization.
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let pid = child.id() as i32;
    let _ = unsafe { libc::kill(pid, libc::SIGINT) };

    // Close stdin so the blocked read_line returns EOF, allowing the process to
    // reach its exit path and print session info.
    drop(child.stdin.take());

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
