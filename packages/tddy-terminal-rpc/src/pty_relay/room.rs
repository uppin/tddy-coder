use crate::local_terminal::RawMode;
use crate::pty_relay::build_start_session_request;
use crate::pty_relay::daemon_http::connectrpc_post;
use crate::pty_relay::daemon_http::exchange_stub_session_token;
use crate::pty_relay::daemon_http::join_terminal_room;
use crate::pty_relay::daemon_http::open_session_room_over_http;
use crate::pty_relay::encode_resize;
use crate::pty_relay::run_grpc_terminal;
use crate::pty_relay::PtyRelayConfig;
use anyhow::Result;
use std::io::{Read, Write};
use std::sync::Arc;
use tddy_room_client::{RoomConnector, RoomSession};

/// Room-backed start/connect (--daemon-identity / --server-identity).
pub(super) async fn run_livekit_session(
    config: PtyRelayConfig,
    connector: &dyn RoomConnector,
) -> Result<()> {
    use prost::Message as _;
    use tddy_service::proto::session::StartSessionResponse;

    // Connect-only path: no StartSession, just connect to the given LiveKit identity.
    if let Some(server_identity) = config.server_identity.clone() {
        let livekit_url = config
            .livekit_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--server-identity requires --livekit-url"))?;
        return run_livekit_terminal(&config, livekit_url.to_string(), server_identity, connector)
            .await;
    }

    // Start-and-connect path.
    let session_token = match config.session_token.as_deref().filter(|s| !s.is_empty()) {
        Some(t) => t.to_string(),
        None => {
            log::info!(target: "tddy_terminal_rpc::pty_relay", "no --session-token; exchanging via {}", config.daemon_url);
            exchange_stub_session_token(&config.daemon_url)
                .await
                .map_err(|e| anyhow::anyhow!("auto-auth: {}", e))?
        }
    };

    let req = build_start_session_request(&config, &session_token);

    log::info!(target: "tddy_terminal_rpc::pty_relay", "calling StartSession via HTTP {}…", config.daemon_url);
    let resp_bytes = connectrpc_post(
        &reqwest::Client::new(),
        &config.daemon_url,
        "session.SessionService",
        "StartSession",
        req.encode_to_vec(),
    )
    .await?;

    let resp = StartSessionResponse::decode(resp_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("decode StartSessionResponse: {}", e))?;

    log::info!(
        target: "tddy_terminal_rpc::pty_relay",
        "session started: id={} server_identity={}",
        resp.session_id, resp.livekit_server_identity
    );

    if !resp.livekit_server_identity.is_empty() {
        if let Some(livekit_url) = config.livekit_url.as_deref() {
            // Ask the daemon to make the session reachable over LiveKit before joining the room to
            // look for it. Starting a session is local work and touches LiveKit nowhere, and
            // `ConnectSession` does not open a room for a caller that did not arrive over LiveKit,
            // so the participant this is about to wait for is put there by `OpenSessionRoom`.
            // Without it the wait below would expire against a room nobody joined.
            open_session_room_over_http(&config.daemon_url, &session_token, &resp.session_id)
                .await?;
            log::info!(target: "tddy_terminal_rpc::pty_relay", "connecting via LiveKit");
            return run_livekit_terminal(
                &config,
                livekit_url.to_string(),
                resp.livekit_server_identity,
                connector,
            )
            .await;
        }
    }

    log::info!(target: "tddy_terminal_rpc::pty_relay", "connecting via gRPC");
    run_grpc_terminal(&config.daemon_url, &resp.session_id, &session_token).await
}

/// Connect to a running session's terminal over a room duplex.
async fn run_livekit_terminal(
    config: &PtyRelayConfig,
    livekit_url: String,
    server_identity: String,
    connector: &dyn RoomConnector,
) -> Result<()> {
    use prost::Message as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};

    log::info!(target: "tddy_terminal_rpc::pty_relay", "waiting for session server participant \"{}\"…", server_identity);
    let session = join_terminal_room(config, &livekit_url, &server_identity, connector).await?;
    // Holds the room for the whole session: dropping the last handle would leave it mid-stream.
    let _room = Arc::clone(&session);

    log::info!(target: "tddy_terminal_rpc::pty_relay", "server visible — starting terminal bidi stream");
    let tddy_room_client::RoomDuplex {
        outbound: terminal_input,
        inbound: mut terminal_output,
    } = session
        .open_duplex("terminal.TerminalService", "StreamTerminalIO")
        .map_err(|e| anyhow::anyhow!("start_bidi_stream: {e}"))?;

    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (output_tx, mut output_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_bidi = Arc::clone(&shutdown);

    tokio::spawn(async move {
        let _ = terminal_input
            .send(TerminalInput { data: vec![] }.encode_to_vec())
            .await;
        if let Some(resize) = encode_resize() {
            let _ = terminal_input
                .send(TerminalInput { data: resize }.encode_to_vec())
                .await;
        }
        loop {
            if shutdown_bidi.load(Ordering::Relaxed) {
                break;
            }
            tokio::select! {
                bytes = key_rx.recv() => {
                    match bytes {
                        Some(bytes) => {
                            if terminal_input.send(TerminalInput { data: bytes }.encode_to_vec()).await.is_err() { break; }
                        }
                        None => break,
                    }
                }
                chunk = terminal_output.recv() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            if let Ok(out) = TerminalOutput::decode(&bytes[..]) {
                                let _ = output_tx.send(out.data);
                            }
                        }
                        Some(Err(e)) => { log::warn!(target: "tddy_terminal_rpc::pty_relay", "recv: {}", e); break; }
                        None => break, // bidi stream closed (server side)
                    }
                }
            }
        }
        shutdown_bidi.store(true, Ordering::Relaxed);
    });

    let _raw = RawMode::enable();

    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            if shutdown_stdin.load(Ordering::Relaxed) {
                break;
            }
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    shutdown_stdin.store(true, Ordering::Relaxed);
                    break;
                }
                Ok(n) => {
                    let _ = key_tx.blocking_send(buf[..n].to_vec());
                }
            }
        }
    });

    let mut stdout = std::io::stdout();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match tokio::time::timeout(Duration::from_millis(50), output_rx.recv()).await {
            Ok(Some(bytes)) => {
                let _ = stdout.write_all(&bytes);
                let _ = stdout.flush();
            }
            Ok(None) => break,
            Err(_timeout) => {}
        }
    }

    Ok(())
}
