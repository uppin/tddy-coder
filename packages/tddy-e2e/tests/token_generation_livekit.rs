//! E2E acceptance test: LiveKit connection via TokenGenerator (API key/secret).
//!
//! Verifies that a server participant can connect using TokenGenerator instead of
//! a pre-generated token, and that run_with_reconnect establishes the connection.
//!
//! Run with: cargo test -p tddy-e2e --features livekit token_generation
//! Requires: LiveKit testkit (testcontainers or LIVEKIT_TESTKIT_WS_URL)

#[cfg(not(feature = "livekit"))]
#[tokio::test]
async fn token_generation_livekit_skipped() {
    // Built without livekit feature; test passes as no-op.
}

#[cfg(feature = "livekit")]
mod livekit_tests {
    use anyhow::Result;
    use livekit::prelude::*;
    use serial_test::serial;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Duration;
    use tddy_livekit::{LiveKitParticipant, TokenGenerator};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_service::{EchoServiceImpl, EchoServiceServer};

    const SERVER_IDENTITY: &str = "token-gen-server";
    const ROOM_NAME: &str = "token-generation-test";
    const DEV_API_KEY: &str = "devkey";
    const DEV_API_SECRET: &str = "secret";

    #[tokio::test]
    #[serial]
    async fn server_connects_via_token_generator() -> Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();

        let token_generator = TokenGenerator::new(
            DEV_API_KEY.to_string(),
            DEV_API_SECRET.to_string(),
            ROOM_NAME.to_string(),
            SERVER_IDENTITY.to_string(),
            Duration::from_secs(120),
        );

        let shutdown = Arc::new(AtomicBool::new(false));
        let server_handle = tokio::spawn({
            let url = url.clone();
            let shutdown = shutdown.clone();
            async move {
                LiveKitParticipant::run_with_reconnect(
                    &url,
                    &token_generator,
                    EchoServiceServer::new(EchoServiceImpl),
                    RoomOptions::default(),
                    shutdown,
                    None,
                    None,
                )
                .await
            }
        });

        let client_token = livekit.generate_token(ROOM_NAME, "client")?;
        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let target: ParticipantIdentity = SERVER_IDENTITY.to_string().into();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            if client_room.remote_participants().contains_key(&target) {
                break;
            }
            if client_events.recv().await.is_none() {
                break;
            }
        }

        server_handle.abort();

        assert!(
            client_room.remote_participants().contains_key(&target),
            "Server participant should be visible when connecting via TokenGenerator"
        );

        Ok(())
    }
}
