//! E2E acceptance test: TokenService GenerateToken over LiveKit.
//!
//! Run with: cargo test -p tddy-e2e --features livekit token_service_generate
//! Requires: LiveKit testkit (testcontainers or LIVEKIT_TESTKIT_WS_URL)
//!
//! Uses #[serial] to avoid parallel execution with other LiveKit tests.

#[cfg(not(feature = "livekit"))]
#[tokio::test]
async fn token_service_livekit_skipped() {
    // Built without livekit feature; test passes as no-op.
}

#[cfg(feature = "livekit")]
mod livekit_tests {
    use anyhow::Result;
    use livekit::prelude::*;
    use prost::Message;
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::Duration;
    use tddy_livekit::{LiveKitParticipant, RpcClient};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_rpc::{MultiRpcService, ServiceEntry};
    use tddy_service::proto::token::{GenerateTokenRequest, GenerateTokenResponse};
    use tddy_service::{TokenProvider, TokenServiceImpl, TokenServiceServer};

    const SERVER_IDENTITY: &str = "token-server";
    const CLIENT_IDENTITY: &str = "token-client";
    const ROOM_NAME: &str = "token-service-test";
    const DEV_API_KEY: &str = "devkey";
    const DEV_API_SECRET: &str = "secret";

    /// TokenProvider that uses dev credentials (same as LiveKitTestkit).
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

    #[tokio::test]
    #[serial]
    async fn token_service_generate_returns_valid_jwt() -> Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();

        let server_token = livekit.generate_token(ROOM_NAME, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(ROOM_NAME, CLIENT_IDENTITY)?;

        let token_service = TokenServiceImpl::new(DevTokenProvider);
        let token_server = TokenServiceServer::new(token_service);
        let multi_service = MultiRpcService::new(vec![ServiceEntry {
            name: "token.TokenService",
            service: Arc::new(token_server) as Arc<dyn tddy_rpc::RpcService>,
        }]);

        let server =
            LiveKitParticipant::connect(&url, &server_token, multi_service, RoomOptions::default())
                .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();

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

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        let request = GenerateTokenRequest {
            room: ROOM_NAME.to_string(),
            identity: "new-participant".to_string(),
        };
        let request_bytes = request.encode_to_vec();

        let response_bytes = rpc_client
            .call_unary("token.TokenService", "GenerateToken", request_bytes)
            .await
            .map_err(|e| anyhow::anyhow!("GenerateToken: {}", e))?;

        server_handle.abort();

        let resp = GenerateTokenResponse::decode(&response_bytes[..])?;
        assert!(!resp.token.is_empty(), "token should not be empty");
        assert!(
            resp.token.matches('.').count() >= 2,
            "JWT should have 3 parts separated by dots"
        );
        assert_eq!(resp.ttl_seconds, 3600);

        Ok(())
    }
}
