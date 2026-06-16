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

use std::sync::Arc;

use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_rpc::{MultiRpcService, ServiceEntry};
use tddy_service::{reflection_entry_from, EchoServiceImpl, EchoServiceServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::new()
        .parse_default_env()
        .try_init()
        .ok();

    let mut args = std::env::args().skip(1);
    let mut url = std::env::var("LIVEKIT_TESTKIT_WS_URL").ok();
    let mut token = None;
    let mut with_reflection = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => url = args.next(),
            "--token" => token = args.next(),
            "--reflection" => with_reflection = true,
            "--room" => {
                args.next();
            }
            _ => {}
        }
    }

    let url = url.ok_or("Missing --url or LIVEKIT_TESTKIT_WS_URL")?;
    let token = token.ok_or("Missing --token")?;

    log::info!(
        "[echo_server] connecting to {} (reflection={})",
        url,
        with_reflection
    );

    // Build a MultiRpcService with EchoService, optionally exposing ServerReflection so
    // dynamic clients (RPC Playground) can enumerate and invoke methods.
    let mut entries: Vec<ServiceEntry> = vec![ServiceEntry {
        name: "test.EchoService",
        service: Arc::new(EchoServiceServer::new(EchoServiceImpl)),
    }];
    if with_reflection {
        let names: Vec<&str> = entries.iter().map(|e| e.name).collect();
        entries.push(reflection_entry_from(&names));
    }
    let service = MultiRpcService::new(entries);

    let participant =
        LiveKitParticipant::connect(&url, &token, service, RoomOptions::default(), None, None)
            .await
            .map_err(|e| format!("Connect failed: {}", e))?;

    log::info!("[echo_server] connected, identity=server, event loop starting");
    // Print READY to stdout (not via log::info which goes to stderr)
    // so that test infrastructure (Cypress) can detect readiness on stdout.
    println!("READY");

    participant.run().await;
    Ok(())
}
