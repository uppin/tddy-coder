//! Minimal echo terminal for Cypress E2E: prints greeting, echoes each line.
//!
//! Connects to LiveKit, serves TerminalService. Prints "Hello! Type a line and press Enter."
//! then echoes each line the client types.
//!
//! Usage:
//!   cargo run -p tddy-livekit --example echo_terminal -- --url ws://127.0.0.1:7880 --token <JWT> [--room <room>]
//!
//! Requires LIVEKIT_TESTKIT_WS_URL or --url. Token from livekit-server-sdk.

use std::io::Write;
use tokio::sync::{broadcast, mpsc};
use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};
use tddy_service::TerminalServiceServer;
use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status, Streaming};
use tokio_stream::wrappers::ReceiverStream;

const GREETING: &[u8] = b"Hello! Type a line and press Enter.\r\n> ";

/// Echo terminal service: sends greeting when first client connects, then echoes lines.
struct EchoTerminalService {
    output_tx: broadcast::Sender<Vec<u8>>,
    input_tx: mpsc::Sender<Vec<u8>>,
}

#[async_trait]
impl TerminalService for EchoTerminalService {
    type StreamTerminalIoStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIoStream>, Status> {
        log::info!("[echo_terminal] stream_terminal_io: first client connected, sending greeting");
        let inner = tddy_service::TerminalServiceImpl::new(
            self.output_tx.clone(),
            self.input_tx.clone(),
        );
        let response = inner.stream_terminal_io(request).await?;
        let n = self.output_tx.send(GREETING.to_vec());
        log::info!(
            "[echo_terminal] stream_terminal_io: greeting sent (receivers={:?}) len={}",
            n,
            GREETING.len()
        );
        Ok(response)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::new()
        .parse_default_env()
        .try_init()
        .ok();

    let mut args = std::env::args().skip(1);
    let mut url = std::env::var("LIVEKIT_TESTKIT_WS_URL").ok();
    let mut token = None;
    let mut _room = "terminal-e2e".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => url = args.next(),
            "--token" => token = args.next(),
            "--room" => _room = args.next().unwrap_or_else(|| "terminal-e2e".into()),
            _ => {}
        }
    }

    let url = url.ok_or("Missing --url or LIVEKIT_TESTKIT_WS_URL")?;
    let token = token.ok_or("Missing --token")?;

    log::info!("[echo_terminal] connecting to {}", url);

    let (output_tx, _) = broadcast::channel::<Vec<u8>>(64);
    let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(64);

    let out = output_tx.clone();
    tokio::spawn(async move {
        let mut buf = Vec::new();
        while let Some(bytes) = input_rx.recv().await {
            buf.extend_from_slice(&bytes);
            while let Some(pos) = buf.iter().position(|&b| b == b'\r' || b == b'\n') {
                let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len().saturating_sub(1)])
                    .trim()
                    .to_string();
                if !line.is_empty() {
                    let echo = format!("{}\r\n> ", line);
                    let _ = out.send(echo.into_bytes());
                }
            }
        }
    });

    let terminal_service = EchoTerminalService {
        output_tx,
        input_tx,
    };

    let participant = LiveKitParticipant::connect(
        &url,
        &token,
        TerminalServiceServer::new(terminal_service),
        RoomOptions::default(),
    )
    .await
    .map_err(|e| format!("Connect failed: {}", e))?;

    log::info!("[echo_terminal] connected, identity=server");
    let _ = std::io::stderr().write_all(b"READY\n");
    let _ = std::io::stderr().flush();

    participant.run().await;
    Ok(())
}
