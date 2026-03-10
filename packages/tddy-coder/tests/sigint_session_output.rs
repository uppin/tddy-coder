//! Integration tests: SIGINT handling and session info output.
//!
//! Verifies that when tddy-demo receives SIGINT (Ctrl+C), it prints session info
//! to stderr before exiting (Session: <id> and Plan dir: <path>).

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn temp_output_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("tddy-sigint-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

/// When tddy-demo receives SIGINT, stderr contains "Session:" (plan dir or fallback).
#[test]
#[cfg(unix)]
fn tddy_demo_sigint_prints_session_info_to_stderr() {
    let output_dir = temp_output_dir();
    let output_dir_str = output_dir.to_str().expect("path");

    let mut child = Command::new(assert_cmd::cargo::cargo_bin("tddy-demo"))
        .args([
            "--goal",
            "plan",
            "--output-dir",
            output_dir_str,
            "--prompt",
            "Build auth SKIP_QUESTIONS",
        ])
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

    let _ = std::fs::remove_dir_all(&output_dir);
}
