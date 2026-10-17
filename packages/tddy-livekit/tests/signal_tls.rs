//! Integration test: the LiveKit signal client can open a `wss://` connection.
//!
//! A LiveKit server published behind TLS (`wss://host:port`) is the production shape. Without a TLS
//! backend compiled into the signal client's websocket, every such connect fails before a byte of
//! TLS is written — `URL error: TLS support not compiled in` — and the daemon retries forever, so
//! the room is never joined and no peer daemon is ever discovered.
//!
//! The server here accepts the TCP connection and closes it, so the connect still fails. What this
//! pins is *how*: a handshake that was attempted, never one that was refused for want of TLS.
//!
//! The test runs with `ring` as rustls' provider, which is the only one the shipped binaries
//! compile in. A workspace-wide test build links `aws-lc-rs` too, and rustls panics rather than pick
//! between two, so the provider is installed explicitly instead of left to crate features.
//!
//! Run: `cargo test -p tddy-livekit --test signal_tls` (no LiveKit server needed).

use anyhow::Result;
use livekit::{Room, RoomOptions};
use std::time::Duration;
use tokio::net::TcpListener;

/// Generous: the SDK retries the signal connect three times with backoff before it gives up.
const CONNECT_DEADLINE: Duration = Duration::from_secs(60);

/// What the websocket layer reports when the URL is `wss://` but no TLS backend was compiled in.
const TLS_NOT_COMPILED_IN: [&str; 2] = ["TLS support not compiled in", "TlsFeatureNotEnabled"];

/// Select `ring`, as the shipped binaries' feature set selects it implicitly.
fn with_the_shipped_crypto_provider() {
    // `Err` only means a provider is already installed for this process, which is the goal.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// A server that completes the TCP handshake and hangs up straight away.
struct HangingUpServer {
    wss_url: String,
    _accepting: tokio::task::JoinHandle<()>,
}

async fn a_server_that_hangs_up() -> Result<HangingUpServer> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let wss_url = format!("wss://{}", listener.local_addr()?);
    let accepting = tokio::spawn(async move {
        while let Ok((connection, _)) = listener.accept().await {
            drop(connection);
        }
    });
    Ok(HangingUpServer {
        wss_url,
        _accepting: accepting,
    })
}

#[tokio::test]
async fn a_wss_connect_attempts_the_tls_handshake() -> Result<()> {
    with_the_shipped_crypto_provider();
    let server = a_server_that_hangs_up().await?;

    let outcome = tokio::time::timeout(
        CONNECT_DEADLINE,
        Room::connect(&server.wss_url, "not-a-real-token", RoomOptions::default()),
    )
    .await
    .expect("the connect gives up within the deadline");

    let error = match outcome {
        Ok(_) => panic!("a server that hangs up cannot admit anyone to a room"),
        Err(error) => format!("{error} / {error:?}"),
    };
    for refusal in TLS_NOT_COMPILED_IN {
        assert!(
            !error.contains(refusal),
            "the wss:// connect was refused for want of TLS support: {error}"
        );
    }
    Ok(())
}
