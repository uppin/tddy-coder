//! CLI integration tests for `tddy-tools session-hook`.
//!
//! These tests validate the subcommand surface, fail-quiet contract, and argument
//! parsing. No real daemon is required — failure or unreachability must always exit 0.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

/// Build a `tddy-tools` command with TDDY_SOCKET cleared so it never
/// accidentally hits a live session relay in the test environment.
fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

/// `session-hook` (or its kebab alias) must appear in the top-level `--help` output so
/// operators can discover it.
#[test]
fn session_hook_appears_in_help() {
    // When / Then
    tddy_tools_bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("session-hook"));
}

/// `session-hook --help` must describe all required and optional flags.
#[test]
fn session_hook_help_lists_required_flags() {
    // When
    let output = tddy_tools_bin()
        .args(["session-hook", "--help"])
        .output()
        .expect("failed to run tddy-tools session-hook --help");

    // Then
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        combined.contains("--session"),
        "help must mention --session: {combined}"
    );
    assert!(
        combined.contains("--daemon"),
        "help must mention --daemon: {combined}"
    );
    assert!(
        combined.contains("--os-user"),
        "help must mention --os-user: {combined}"
    );
    assert!(
        combined.contains("--hook-token"),
        "help must mention --hook-token: {combined}"
    );
    assert!(
        combined.contains("--event"),
        "help must mention --event: {combined}"
    );
}

/// Running `session-hook` without `--session` must exit with clap error code 2.
#[test]
fn session_hook_requires_session_flag() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--daemon",
            "http://127.0.0.1:8899",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-abc",
            "--event",
            "Stop",
        ])
        .write_stdin(r#"{"hook_event_name":"Stop"}"#)
        .assert()
        .code(2);
}

/// When stdin carries an unknown/no-op event (`PreToolUse`) and the daemon URL is
/// unroutable, the process must still exit 0 — the hook must never block Claude.
///
/// This exercises the short-circuit path: the event maps to `None`, so no network
/// call is attempted at all.
#[test]
fn session_hook_noop_event_exits_zero_without_daemon() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "test-session-noop-1",
            "--daemon",
            "http://127.0.0.1:1", // unroutable port
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-noop",
            "--event",
            "PreToolUse",
        ])
        .write_stdin(r#"{"hook_event_name":"PreToolUse","session_id":"test-session-noop-1"}"#)
        .assert()
        .success(); // exit 0 — fail-quiet contract
}

/// When stdin carries Cursor `sessionStart` (maps to `Started`) but the daemon is unreachable,
/// the process must still exit 0 — fail-quiet contract.
#[test]
fn session_hook_cursor_session_start_unreachable_daemon_exits_zero() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-start-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-start",
        ])
        .write_stdin(r#"{"hook_event_name":"sessionStart","session_id":"cursor-session-start-1"}"#)
        .assert()
        .success();
}

/// Cursor `beforeSubmitPrompt` maps to `Running`; fail-quiet even when daemon is down.
#[test]
fn session_hook_cursor_before_submit_prompt_unreachable_daemon_exits_zero() {
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-running-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-running",
        ])
        .write_stdin(
            r#"{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-session-running-1"}"#,
        )
        .assert()
        .success();
}

/// Cursor `stop` maps to `Done`; stdin `hook_event_name` is used without `--event`.
#[test]
fn session_hook_cursor_stop_unreachable_daemon_exits_zero() {
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "cursor-session-stop-1",
            "--daemon",
            "http://127.0.0.1:1",
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-cursor-stop",
        ])
        .write_stdin(r#"{"hook_event_name":"stop","session_id":"cursor-session-stop-1"}"#)
        .assert()
        .success();
}

/// When stdin carries a `SessionStart` event (maps to `Started`) but the daemon is
/// unreachable, the process must still exit 0.
///
/// This exercises the fail-quiet contract on the network-call path: the RPC fails but
/// the hook must never propagate the error upward (it would block Claude Code).
#[test]
fn session_hook_unreachable_daemon_exits_zero() {
    // When / Then
    tddy_tools_bin()
        .args([
            "session-hook",
            "--session",
            "test-session-unreachable-1",
            "--daemon",
            "http://127.0.0.1:1", // port 1 is always closed
            "--os-user",
            "testuser",
            "--hook-token",
            "tok-unreachable",
            "--event",
            "SessionStart",
        ])
        .write_stdin(
            r#"{"hook_event_name":"SessionStart","session_id":"test-session-unreachable-1"}"#,
        )
        .assert()
        .success(); // exit 0 — fail-quiet contract even on connection error
}
