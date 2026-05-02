//! Minimal echo terminal for Cypress E2E: prints greeting, echoes each line.
//!
//! Connects to LiveKit, serves TerminalService. Prints "Hello! Type a line and press Enter."
//! then echoes each line the client types.
//!
//! Usage:
//!   cargo run -p tddy-livekit --example echo_terminal -- --url ws://127.0.0.1:7880 --token <JWT> [--room <room>]
//!
//! Requires LIVEKIT_TESTKIT_WS_URL or --url. Token from livekit-server-sdk.

use async_trait::async_trait;
use std::io::Write;
use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput, TerminalService};
use tddy_service::TerminalServiceServer;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const GREETING: &[u8] = b"Hello! Type a line and press Enter.\r\n> ";

/// Echo terminal service: sends greeting, then echoes each line the client types.
/// Each connection gets its own echo loop.
struct EchoTerminalService;

#[async_trait]
impl TerminalService for EchoTerminalService {
    type StreamTerminalIoStream = ReceiverStream<Result<TerminalOutput, Status>>;

    async fn stream_terminal_io(
        &self,
        request: Request<Streaming<TerminalInput>>,
    ) -> Result<Response<Self::StreamTerminalIoStream>, Status> {
        let stream = request.into_inner();
        let (tx, rx) = mpsc::channel(64);

        let _ = tx
            .send(Ok(TerminalOutput {
                data: GREETING.to_vec(),
            }))
            .await;

        tokio::spawn(async move {
            let mut buf = Vec::new();
            futures_util::pin_mut!(stream);
            while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
                if let Ok(input) = item {
                    buf.extend_from_slice(&input.data);
                    while let Some(pos) = buf.iter().position(|&b| b == b'\r' || b == b'\n') {
                        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                        let line = String::from_utf8_lossy(
                            &line_bytes[..line_bytes.len().saturating_sub(1)],
                        )
                        .trim()
                        .to_string();
                        if !line.is_empty() {
                            let echo = format!("{}\r\n> ", line);
                            if tx
                                .send(Ok(TerminalOutput {
                                    data: echo.into_bytes(),
                                }))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
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

    let participant = LiveKitParticipant::connect(
        &url,
        &token,
        TerminalServiceServer::new(EchoTerminalService),
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .map_err(|e| format!("Connect failed: {}", e))?;

    log::info!("[echo_terminal] connected, identity=server");
    let _ = std::io::stderr().write_all(b"READY\n");
    let _ = std::io::stderr().flush();

    participant.run().await;
    Ok(())
}
