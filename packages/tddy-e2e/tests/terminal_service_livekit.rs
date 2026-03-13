//! E2E acceptance test: TerminalService StreamTerminalIO over LiveKit.
//!
//! Run with: cargo test -p tddy-e2e --features livekit terminal_stream_io
//! Requires: LiveKit testkit (testcontainers or LIVEKIT_TESTKIT_WS_URL)
//!
//! Uses #[serial] to avoid parallel execution with other LiveKit tests.
//! The livekit feature gates tddy-livekit (webrtc-sys); omit for webrtc-free CI.

#[cfg(not(feature = "livekit"))]
#[tokio::test]
async fn terminal_service_livekit_skipped() {
    // Built without livekit feature; test passes as no-op.
}

#[cfg(feature = "livekit")]
mod livekit_tests {
    use anyhow::Result;
    use livekit::prelude::*;
    use prost::Message;
    use serial_test::serial;
    use std::time::Duration;
    use tddy_livekit::{LiveKitParticipant, RpcClient};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
    use tddy_service::{TerminalServiceImpl, TerminalServiceServer};

    const SERVER_IDENTITY: &str = "server";
    const CLIENT_IDENTITY: &str = "client";
    const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

    async fn wait_for_participant(
        room: &Room,
        events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
        identity: &str,
    ) -> Result<()> {
        let target: ParticipantIdentity = identity.to_string().into();
        if room.remote_participants().contains_key(&target) {
            return Ok(());
        }
        tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::ParticipantConnected(p) = event {
                    if p.identity() == target {
                        return;
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Timed out waiting for participant '{}'", identity))?;
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn terminal_stream_io_streams_bytes_to_client() -> Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = "terminal-stream-test";

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let (tx_output, _rx_output) = tokio::sync::broadcast::channel::<Vec<u8>>(16);
        let (tx_input, _rx_input) = tokio::sync::mpsc::channel::<Vec<u8>>(16);

        let terminal_service = TerminalServiceImpl::new(tx_output.clone(), tx_input);

        let server = LiveKitParticipant::connect(
            &url,
            &server_token,
            TerminalServiceServer::new(terminal_service),
            RoomOptions::default(),
        )
        .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();
        wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        let request = TerminalInput { data: vec![] };
        let request_bytes = request.encode_to_vec();

        let mut rx = rpc_client
            .call_server_stream(
                "terminal.TerminalService",
                "StreamTerminalIO",
                request_bytes,
            )
            .await
            .map_err(|e| anyhow::anyhow!("StreamTerminalIO: {}", e))?;

        let test_bytes = b"Hello terminal!";
        tokio::time::sleep(Duration::from_millis(500)).await;
        for _ in 0..30 {
            let _ = tx_output.send(test_bytes.to_vec());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let mut received = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(chunk)) =
                tokio::time::timeout(Duration::from_millis(500), rx.recv()).await
            {
                let bytes = chunk.map_err(|e| anyhow::anyhow!("chunk error: {}", e))?;
                let output = TerminalOutput::decode(&bytes[..])?;
                received.extend_from_slice(&output.data);
                if received.contains(&b'!') {
                    break;
                }
            }
        }

        server_handle.abort();

        assert!(
            received.windows(test_bytes.len()).any(|w| w == test_bytes),
            "Expected to receive terminal bytes containing {:?}, got {} bytes",
            std::str::from_utf8(test_bytes).unwrap(),
            received.len()
        );

        Ok(())
    }
} // mod livekit_tests
