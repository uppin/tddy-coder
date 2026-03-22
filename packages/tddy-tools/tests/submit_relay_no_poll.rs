//! `tddy-tools submit` must complete against the real Unix relay without the presenter calling
//! `poll_tool_calls()`. Exercises the CLI + socket path; `tddy-core` has a lower-level regression
//! (`toolcall_relay_presenter_stuck`) using a raw stream.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::json;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;

#[test]
#[cfg(unix)]
fn submit_exits_ok_when_presenter_never_polls() {
    let (socket_path, _hold_tool_rx) = start_toolcall_listener().expect("start listener");
    let sock = socket_path.clone();
    let bin = cargo_bin_cmd!("tddy-tools").get_program().to_owned();

    let data = json!({"goal": "plan", "prd": "# minimal"}).to_string();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let out = Command::new(bin)
            .env("TDDY_SOCKET", &sock)
            .args(["submit", "--goal", "plan", "--data", &data])
            .output();
        let _ = tx.send(out);
    });

    let deadline = Duration::from_secs(2);
    let child_result = rx.recv_timeout(deadline).unwrap_or_else(|_| {
        panic!(
            "tddy-tools submit must finish within {:?} when presenter never polls; process hung",
            deadline
        );
    });
    let child_out = child_result.expect("tddy-tools spawn");

    assert!(
        child_out.status.success(),
        "tddy-tools should exit 0; stderr={}",
        String::from_utf8_lossy(&child_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&child_out.stdout);
    assert!(
        stdout.contains("\"status\":\"ok\"") && stdout.contains("\"goal\":\"plan\""),
        "expected ok relay JSON on stdout; got: {stdout}"
    );
}
