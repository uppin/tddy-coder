//! LiveKit path acceptance for remote sandbox exec (PRD: livekit_exec_smoke).
//!
//! Requires LiveKit testkit (reuse `LIVEKIT_TESTKIT_WS_URL` or start a container).

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use livekit::prelude::*;
use serial_test::serial;
use tddy_livekit::{LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{MultiRpcService, ServiceEntry};
use tddy_service::{
    RemoteSandboxServiceImpl, RemoteSandboxServiceServer, TokenProvider, TokenServiceImpl,
    TokenServiceServer,
};

const SERVER_IDENTITY: &str = "remote-sandbox-livekit-server";
const CLIENT_IDENTITY: &str = "remote-sandbox-livekit-client";
const ROOM: &str = "remote-sandbox-livekit-smoke";
const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";

struct DevTokenProvider;

impl TokenProvider for DevTokenProvider {
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
        let gen = tddy_livekit::TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            room.to_string(),
            identity.to_string(),
            Duration::from_secs(3600),
        );
        gen.generate_for(room, identity).map_err(|e| e.to_string())
    }

    fn ttl_seconds(&self) -> u64 {
        3600
    }
}

async fn wait_for_server(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) {
    let target: ParticipantIdentity = SERVER_IDENTITY.to_string().into();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if room.remote_participants().contains_key(&target) {
            return;
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), events.recv()).await;
    }
    panic!("timed out waiting for {SERVER_IDENTITY} to join");
}

#[tokio::test]
#[serial]
async fn livekit_exec_smoke() -> Result<()> {
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();

    let server_token = livekit.generate_token(ROOM, SERVER_IDENTITY)?;
    let client_token = livekit.generate_token(ROOM, CLIENT_IDENTITY)?;

    // TokenService keeps the room healthy; RemoteSandboxService is registered so LiveKit unary hits daemon-shaped stubs.
    let token_service = TokenServiceImpl::new(DevTokenProvider);
    let token_server = TokenServiceServer::new(token_service);
    let remote_sandbox_server =
        RemoteSandboxServiceServer::new(RemoteSandboxServiceImpl::default());
    let multi = MultiRpcService::new(vec![
        ServiceEntry {
            name: "token.TokenService",
            service: Arc::new(token_server) as Arc<dyn tddy_rpc::RpcService>,
        },
        ServiceEntry {
            name: RemoteSandboxServiceServer::<RemoteSandboxServiceImpl>::NAME,
            service: Arc::new(remote_sandbox_server) as Arc<dyn tddy_rpc::RpcService>,
        },
    ]);

    let server = LiveKitParticipant::connect(&url, &server_token, multi, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("server connect: {e}"))?;
    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {e}"))?;

    wait_for_server(&client_room, &mut client_events).await;

    let rpc_events = client_room.subscribe();
    let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

    // Unary RPC: run fixed payload through sandbox, return exit code + SHA-256 of stdout (proto TBD).
    let resp_bytes = rpc_client
        .call_unary(
            "remote_sandbox.v1.RemoteSandboxService",
            "ExecChecksum",
            vec![],
        )
        .await
        .map_err(|e| anyhow::anyhow!("LiveKit unary ExecChecksum: {e}"))?;

    server_handle.abort();

    assert!(
        !resp_bytes.is_empty(),
        "ExecChecksum response body must be non-empty protobuf"
    );

    Ok(())
}
