//! Acceptance tests: `ensure_relay_daemon` lifecycle (Phase 4 follow-up).
//!
//! AC20/AC21: `ensure_relay_daemon` returns a `RelayEndpoint` when the relay is reachable;
//! when the relay is down, it attempts to start it (or fails gracefully); concurrent calls
//! use file-locking to avoid spawning two daemons.

use std::path::PathBuf;
use tddy_tools::relay::{RelayEndpoint, ensure_relay_daemon, RelayConfig};

/// AC20: `RelayEndpoint` has a `port` field and a `base_url()` helper.
#[test]
fn relay_endpoint_has_port_and_base_url() {
    let ep = RelayEndpoint { port: 9321 };
    assert_eq!(ep.port, 9321);
    assert_eq!(
        ep.base_url(),
        "http://127.0.0.1:9321",
        "base_url() must return localhost URL with the relay port"
    );
}

/// AC20: when no relay is running and no tddy-daemon binary is on PATH (test environment),
/// `ensure_relay_daemon` returns an Err — not panics, not hangs.
#[test]
fn ensure_relay_daemon_fails_gracefully_when_no_binary() {
    let relay_dir = tempfile::tempdir().unwrap();
    let cfg = RelayConfig {
        base_dir: relay_dir.path().to_path_buf(),
        idle_timeout_secs: 60,
        // Use a PATH that definitely has no tddy-daemon binary.
        daemon_binary: PathBuf::from("/nonexistent/path/tddy-daemon"),
    };

    let result = ensure_relay_daemon(&cfg);
    assert!(
        result.is_err(),
        "ensure_relay_daemon must return Err when daemon binary is not found; got Ok"
    );
    let err_str = result.unwrap_err().to_string();
    assert!(
        !err_str.contains("panicked at"),
        "error must not be a panic; got: {}",
        err_str
    );
}

/// AC21: when a valid discovery file exists pointing to a running HTTP server, `ensure_relay_daemon`
/// reads the file and returns the endpoint without spawning a new process.
///
/// We simulate a running relay by starting a minimal HTTP server on a random port and writing
/// its port to the discovery file.
#[tokio::test]
async fn ensure_relay_daemon_reuses_running_relay_from_discovery_file() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let relay_dir = tempfile::tempdir().unwrap();

    // Start a minimal HTTP server that responds 200 to GET /api/config.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let served = Arc::new(AtomicBool::new(false));
    let served_clone = served.clone();
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            served_clone.store(true, Ordering::SeqCst);
            // Write a minimal HTTP 200 response and close.
            use tokio::io::AsyncWriteExt;
            let _ = stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                )
                .await;
        }
    });

    // Write the discovery file.
    let discovery = relay_dir.path().join("daemon.json");
    let pid = std::process::id();
    std::fs::write(
        &discovery,
        serde_json::json!({ "port": port, "pid": pid, "started_at": 0 }).to_string(),
    )
    .unwrap();

    let cfg = RelayConfig {
        base_dir: relay_dir.path().to_path_buf(),
        idle_timeout_secs: 60,
        daemon_binary: PathBuf::from("tddy-daemon"),
    };

    // Give tokio a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let result = ensure_relay_daemon(&cfg);
    assert!(
        result.is_ok(),
        "ensure_relay_daemon must return Ok when relay is reachable; got: {:?}",
        result.err()
    );
    let ep = result.unwrap();
    assert_eq!(ep.port, port, "returned endpoint must match the seeded port");
}
