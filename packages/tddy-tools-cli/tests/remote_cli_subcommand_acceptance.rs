//! Acceptance tests: `tddy-tools remote` subcommand implementations (Gap C).
//!
//! AC: `remote start-session` POSTs to the StartSession Connect RPC and prints JSON containing
//!     `session_id` on stdout.
//! AC: `remote connect-session` POSTs to ConnectSession and prints JSON with livekit fields.
//! AC: `remote sync-context --dest <dir>` fetches context files via ExecuteTool Read/Glob and
//!     writes them to the dest directory.
//! AC: `remote list-tools` uses the ListExecTools Connect RPC (not a plain GET). A Connect
//!     `ListExecToolsResponse` JSON is parsed and tool names printed, one per line.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use prost::Message as _;
use tddy_service::proto::exec_tools::{ExecuteToolResponse, ListExecToolsResponse, ToolDef};
use tddy_service::proto::session::{
    ConnectSessionResponse, StartSessionRequest, StartSessionResponse,
};

fn tddy_tools_bin() -> Command {
    cargo_bin_cmd!("tddy-tools")
}

/// Write a `daemon.json` discovery file pointing to the given port.
fn write_discovery(dir: &std::path::Path, port: u16) {
    std::fs::write(
        dir.join("daemon.json"),
        serde_json::json!({ "port": port, "pid": 0, "started_at": 0 }).to_string(),
    )
    .unwrap();
}

/// Spawn a minimal stub HTTP server on a real OS thread (blocking I/O) that replies with
/// `response_body` to any incoming request.
///
/// Using a real OS thread (not a tokio task) ensures the stub can handle connections even
/// when the tokio current_thread runtime is blocked in `Command::output()`.
///
/// Returns the port the stub listens on.
fn spawn_stub_thread(response_body: Vec<u8>) -> u16 {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf);
            // Only serve POST requests — all four subcommands use Connect-protocol POST.
            // Reject GETs with 405 so tests fail until the transport is correct.
            let is_post = buf.starts_with(b"POST ");
            if is_post {
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/proto\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    response_body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&response_body);
            } else {
                let body = b"Method Not Allowed";
                let resp = format!(
                    "HTTP/1.1 405 Method Not Allowed\r\ncontent-length: {}\r\nconnection: close\r\n\r\nMethod Not Allowed",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });

    port
}

// ── start-session ────────────────────────────────────────────────────────────────────────────────

/// AC: `remote start-session` prints JSON containing the `session_id` returned by the daemon.
///
/// Currently `run_start_session` is `bail!("start-session: not yet implemented")`.
#[test]
fn start_session_prints_session_id_from_daemon_response() {
    // Given
    let response = StartSessionResponse {
        session_id: "sess-stub-abc".to_string(),
        ..Default::default()
    }
    .encode_to_vec();
    let port = spawn_stub_thread(response);
    let relay_dir = tempfile::tempdir().unwrap();
    write_discovery(relay_dir.path(), port);

    // When
    let output = tddy_tools_bin()
        .args([
            "remote",
            "start-session",
            "--base-dir",
            relay_dir.path().to_str().unwrap(),
            "--session-token",
            "test-token",
        ])
        .output()
        .expect("remote start-session must not panic");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "remote start-session must exit 0 when relay responds; stderr: {}",
        stderr
    );
    assert!(
        stdout.contains("sess-stub-abc") || stdout.contains("session_id"),
        "remote start-session must print session_id on stdout; got: {}",
        stdout
    );
}

/// AC: `remote start-session --help` lists `--session-token` as an accepted option.
#[test]
fn start_session_help_lists_session_token_flag() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "start-session", "--help"])
        .output()
        .expect("remote start-session --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("session-token"),
        "start-session --help must list --session-token; got: {}",
        stdout
    );
}

// ── connect-session ──────────────────────────────────────────────────────────────────────────────

/// AC: `remote connect-session` prints JSON containing livekit fields from the daemon response.
///
/// Currently `run_connect_session` is `bail!("connect-session: not yet implemented")`.
#[test]
fn connect_session_prints_livekit_info_from_daemon_response() {
    // Given
    let response = ConnectSessionResponse {
        livekit_room: "room-stub".to_string(),
        livekit_url: "ws://stub:7880".to_string(),
        livekit_server_identity: "srv-identity".to_string(),
    }
    .encode_to_vec();
    let port = spawn_stub_thread(response);
    let relay_dir = tempfile::tempdir().unwrap();
    write_discovery(relay_dir.path(), port);

    // When
    let output = tddy_tools_bin()
        .args([
            "remote",
            "connect-session",
            "--base-dir",
            relay_dir.path().to_str().unwrap(),
            "--session-id",
            "sess-existing-123",
            "--session-token",
            "test-token",
        ])
        .output()
        .expect("remote connect-session must not panic");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "remote connect-session must exit 0 when relay responds; stderr: {}",
        stderr
    );
    assert!(
        stdout.contains("room-stub") || stdout.contains("livekit"),
        "remote connect-session must print livekit fields on stdout; got: {}",
        stdout
    );
}

/// AC: `remote connect-session --help` lists `--session-token` as an accepted option.
///
/// Currently `ConnectSessionArgs` only has `base_dir` and `session_id`.
#[test]
fn connect_session_help_lists_session_token_flag() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "connect-session", "--help"])
        .output()
        .expect("remote connect-session --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("session-token"),
        "connect-session --help must list --session-token; got: {}",
        stdout
    );
}

// ── sync-context ─────────────────────────────────────────────────────────────────────────────────

/// AC: `remote sync-context --dest <dir>` writes context files to the dest directory.
///
/// Currently `run_sync_context` is `bail!("sync-context: not yet implemented")`.
/// Also, `SyncContextArgs` doesn't yet have a `--dest` flag.
#[test]
fn sync_context_writes_context_files_to_dest_directory() {
    // Given — ExecuteTool Read/Glob response: returns project docs content.
    let response = ExecuteToolResponse {
        result_json: r#"{"content":"Project docs"}"#.to_string(),
        ..Default::default()
    }
    .encode_to_vec();
    let port = spawn_stub_thread(response);
    let relay_dir = tempfile::tempdir().unwrap();
    write_discovery(relay_dir.path(), port);
    let dest_dir = tempfile::tempdir().unwrap();

    // When
    let output = tddy_tools_bin()
        .args([
            "remote",
            "sync-context",
            "--base-dir",
            relay_dir.path().to_str().unwrap(),
            "--dest",
            dest_dir.path().to_str().unwrap(),
            "--session-token",
            "test-token",
        ])
        .output()
        .expect("remote sync-context must not panic");

    // Then
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "remote sync-context must exit 0 when relay responds; stderr: {}",
        stderr
    );

    // At minimum, CLAUDE.md (or AGENTS.md) must be written to dest.
    let entries: Vec<_> = std::fs::read_dir(dest_dir.path())
        .expect("dest dir must be readable after sync-context")
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !entries.is_empty(),
        "remote sync-context must write at least one file to --dest; dest dir is empty"
    );
}

/// AC: `remote sync-context --help` lists `--dest` and `--session-token` as accepted options.
///
/// Currently `SyncContextArgs` has only `base_dir` — neither flag exists.
#[test]
fn sync_context_help_lists_dest_and_session_token_flags() {
    // When
    let output = tddy_tools_bin()
        .args(["remote", "sync-context", "--help"])
        .output()
        .expect("remote sync-context --help must not crash");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dest"),
        "sync-context --help must list --dest; got: {}",
        stdout
    );
    assert!(
        stdout.contains("session-token"),
        "sync-context --help must list --session-token; got: {}",
        stdout
    );
}

// ── list-tools (rework to Connect RPC) ───────────────────────────────────────────────────────────

/// AC: `remote list-tools` parses a Connect-protocol `ListExecToolsResponse` JSON object
/// (not a raw array).  The response `{"tools":[{"name":"Read",...},...]}` must produce
/// `Read` and `Write` on stdout.
///
/// Currently `list-tools` does a plain GET and parses the response as a raw JSON array `["Read",
/// "Write"]`. Once reworked to use the Connect `ListExecTools` RPC, it must POST and parse the
/// object shape. This test serves a Connect response — it will fail until the transport is changed.
#[test]
fn list_tools_parses_connect_list_exec_tools_response() {
    // Given — Connect-protocol ListExecToolsResponse, protobuf-encoded.
    let response = ListExecToolsResponse {
        tools: ["Read", "Write", "Shell"]
            .into_iter()
            .map(|name| ToolDef {
                name: name.to_string(),
                ..Default::default()
            })
            .collect(),
    }
    .encode_to_vec();
    let port = spawn_stub_thread(response);
    let relay_dir = tempfile::tempdir().unwrap();
    write_discovery(relay_dir.path(), port);

    // When
    let output = tddy_tools_bin()
        .args([
            "remote",
            "list-tools",
            "--base-dir",
            relay_dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("remote list-tools must not panic");

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "remote list-tools must exit 0 when relay responds with Connect JSON; stderr: {}",
        stderr
    );

    // Tool names must appear one per line.
    assert!(
        stdout.contains("Read"),
        "remote list-tools must print 'Read' from the Connect ListExecToolsResponse; got: {}",
        stdout
    );
    assert!(
        stdout.contains("Write"),
        "remote list-tools must print 'Write'; got: {}",
        stdout
    );
    assert!(
        stdout.contains("Shell"),
        "remote list-tools must print 'Shell'; got: {}",
        stdout
    );
}

/// A relay stub that answers every POST with an empty protobuf message and hands back each
/// request's body: the bytes after the header block.
fn spawn_recording_stub() -> (u16, std::sync::mpsc::Receiver<Vec<u8>>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = vec![0u8; 16384];
            let n = stream.read(&mut buf).unwrap_or(0);
            let body_start = buf[..n]
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map_or(n, |at| at + 4);
            let _ = tx.send(buf[body_start..n].to_vec());
            let resp = "HTTP/1.1 200 OK\r\ncontent-type: application/proto\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (port, rx)
}

#[test]
fn remote_start_session_passes_each_agent() {
    // Given
    let (port, bodies) = spawn_recording_stub();
    let dir = tempfile::tempdir().unwrap();
    write_discovery(dir.path(), port);

    // When
    tddy_tools_bin()
        .args(["remote", "start-session", "--base-dir"])
        .arg(dir.path())
        .args([
            "--agent",
            "codex-fast@codebase-host",
            "--agent",
            "claude-coder@codebase-host",
        ])
        .assert()
        .success();

    // Then
    let body = bodies.recv().expect("a request");
    let request = StartSessionRequest::decode(body.as_slice()).expect("a protobuf body");
    assert_eq!(request.session_type, "workspace");
    assert_eq!(
        request.specialized_agents,
        vec![
            "codex-fast@codebase-host".to_string(),
            "claude-coder@codebase-host".to_string(),
        ]
    );
}
