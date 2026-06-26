//! Acceptance tests: `tddy-tools remote` subcommand group (Phase 4 follow-up).
//!
//! AC: the `remote` subcommand must expose `start-session`, `connect-session`,
//! `resume-session`, and `sync-context` as named subcommands. Each must appear in
//! `tddy-tools remote --help` output.
//!
//! AC: `remote list-tools` must contact the relay daemon via HTTP (reading the discovery
//! file for the port), not just read tool names from the discovery JSON file directly.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

fn tddy_tools_bin() -> Command {
    cargo_bin_cmd!("tddy-tools")
}

/// AC: `tddy-tools remote --help` lists `start-session` as a subcommand.
#[test]
fn remote_start_session_subcommand_exists_in_help() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "--help"])
        .output()
        .expect("tddy-tools remote --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("start-session"),
        "remote --help must list start-session; got: {}",
        stdout
    );
}

/// AC: `tddy-tools remote --help` lists `connect-session` as a subcommand.
#[test]
fn remote_connect_session_subcommand_exists_in_help() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "--help"])
        .output()
        .expect("tddy-tools remote --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("connect-session"),
        "remote --help must list connect-session; got: {}",
        stdout
    );
}

/// AC: `tddy-tools remote --help` lists `sync-context` as a subcommand.
#[test]
fn remote_sync_context_subcommand_exists_in_help() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "--help"])
        .output()
        .expect("tddy-tools remote --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("sync-context"),
        "remote --help must list sync-context; got: {}",
        stdout
    );
}

/// AC: `remote list-tools` reads the relay port from `daemon.json` and contacts the daemon via
/// HTTP to fetch the tool catalog — it must NOT return an empty list when the relay has tools.
///
/// Seed a `daemon.json` with a port pointing to a minimal HTTP server that serves a
/// `ListExecToolsResponse`-compatible JSON. Verify the output contains the tool names.
// Multi-threaded: the mock relay runs as a spawned task while the test thread blocks
// in assert_cmd's synchronous `.output()`. A current-thread runtime would starve the
// server task (the subprocess would connect but never get a response) and hang.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_list_tools_fetches_catalog_from_relay_daemon_not_from_discovery_file() {
    // Given — start a minimal HTTP server that responds to the ListExecTools endpoint.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            // Drain the incoming request first: closing a socket that still has an unread
            // request RSTs the connection and surfaces in the client as a send error.
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            // ListExecToolsResponse-shaped body — `run_list_tools` reads `tools[].name`.
            let body =
                r#"{"tools":[{"name":"Read"},{"name":"Write"},{"name":"Grep"},{"name":"Shell"}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
        }
    });

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let relay_dir = tempfile::tempdir().unwrap();
    // Write daemon.json with just the port (no 'tools' key — tools must come from HTTP).
    let discovery = relay_dir.path().join("daemon.json");
    std::fs::write(
        &discovery,
        serde_json::json!({ "port": port, "pid": 0, "started_at": 0 }).to_string(),
    )
    .unwrap();

    // When
    let output = tddy_tools_bin()
        .args(["remote", "list-tools", "--base-dir", relay_dir.path().to_str().unwrap()])
        .output()
        .expect("tddy-tools remote list-tools must not panic");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "remote list-tools must exit 0 when relay is reachable; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Read"),
        "remote list-tools output must contain tool names from relay; got: {}",
        stdout
    );
}
