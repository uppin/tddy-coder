//! Standalone echo server participant for Cypress/TypeScript tests.
//!
//! Connects to a LiveKit room and serves EchoService over the data channel.
//! Prints READY to stdout when connected so test infrastructure can detect readiness.
//!
//! Usage:
//!   cargo run --example echo_server -- --url ws://127.0.0.1:7880 --token <JWT> [--room <room>]
//!
//! Requires LIVEKIT_TESTKIT_WS_URL to be set or --url provided. Token must be generated
//! externally (e.g. by livekit-server-sdk in Node.js).

use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_service::{EchoServiceImpl, EchoServiceServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::new()
        .parse_default_env()
        .try_init()
        .ok();

    let mut args = std::env::args().skip(1);
    let mut url = std::env::var("LIVEKIT_TESTKIT_WS_URL").ok();
    let mut token = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => url = args.next(),
            "--token" => token = args.next(),
            "--room" => {
                args.next();
            }
            _ => {}
        }
    }

    let url = url.ok_or("Missing --url or LIVEKIT_TESTKIT_WS_URL")?;
    let token = token.ok_or("Missing --token")?;

    log::info!("[echo_server] connecting to {}", url);

    let participant = LiveKitParticipant::connect(
        &url,
        &token,
        EchoServiceServer::new(EchoServiceImpl),
        RoomOptions::default(),
    )
    .await
    .map_err(|e| format!("Connect failed: {}", e))?;

    log::info!("[echo_server] connected, identity=server, event loop starting");
    log::info!("READY");

    participant.run().await;
    Ok(())
}
