//! Integration tests: SIGINT handling and session info output.
//!
//! Verifies that when tddy-demo receives SIGINT (Ctrl+C), it prints session info
//! to stderr before exiting (Session: <id> and Plan dir: <path>).

mod common;

use std::io::{Read, Write};
use std::process::Stdio;

#[allow(deprecated)]
fn tddy_demo_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("tddy-demo")
}

/// When tddy-demo receives SIGINT, stderr contains "Session:" (plan dir or fallback).
#[test]
#[cfg(unix)]
fn tddy_demo_sigint_prints_session_info_to_stderr() {
    let mut child = std::process::Command::new(tddy_demo_bin())
        .args(["--goal", "plan", "--prompt", "Build auth SKIP_QUESTIONS"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tddy-demo");

    let _stdin_handle = std::thread::spawn({
        let mut stdin = child.stdin.take().expect("stdin");
        move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = stdin.write_all(b"a\n");
        }
    });

    // Send SIGINT during workflow (before StubBackend can complete).
    // Delays sized for slow CI; no conditional logic per testing practices.
    std::thread::sleep(std::time::Duration::from_millis(400));

    let pid = child.id() as i32;
    let _ = unsafe { libc::kill(pid, libc::SIGINT) };

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
