//! E2E acceptance: LiveKit client receives `SessionRuntimeStatus` on `tddy.v1.TddyRemote` / `Stream`.
//!
//! Mirrors `livekit_terminal_rpc.rs` setup. Requires: `livekit` feature and LiveKit testkit.
//!
//! Run: `cargo test -p tddy-e2e --features livekit livekit_remote_observes_session_runtime_status`
//!
//! Without `--features livekit` this integration test target compiles but contains no tests.

#[cfg(feature = "livekit")]
mod livekit_tests {
    use std::time::Duration;

    use anyhow::Result;
    use livekit::prelude::*;
    use prost::Message;
    use serial_test::serial;
    use std::sync::Arc;

    use tddy_livekit::{LiveKitParticipant, RpcClient};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_rpc::{MultiRpcService, ServiceEntry};
    use tddy_service::gen::server_message;
    use tddy_service::gen::{ClientMessage, ServerMessage, SessionRuntimeStatus};
    use tddy_service::{TddyRemoteRpcServer, TerminalServiceServer, TerminalServiceVirtualTui};

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
    async fn livekit_remote_observes_session_runtime_status() -> Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = "remote-session-runtime-status";

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let (_presenter_handle, factory, shutdown, remote_handle) =
            tddy_e2e::spawn_presenter_with_view_connection_factory(Some("Build auth".to_string()));

        let terminal_service = TerminalServiceVirtualTui::new(factory, false);

        let rpc = MultiRpcService::new(vec![
            ServiceEntry {
                name: TerminalServiceServer::<TerminalServiceVirtualTui>::NAME,
                service: Arc::new(TerminalServiceServer::new(terminal_service)),
            },
            ServiceEntry {
                name: TddyRemoteRpcServer::NAME,
                service: Arc::new(TddyRemoteRpcServer::new(remote_handle)),
            },
        ]);

        let server =
            LiveKitParticipant::connect(&url, &server_token, rpc, RoomOptions::default()).await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();
        wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        let (mut sender, mut rx) = rpc_client
            .start_bidi_stream("tddy.v1.TddyRemote", "Stream")
            .map_err(|e| anyhow::anyhow!("start bidi TddyRemote/Stream: {}", e))?;

        let hello = ClientMessage {
            intent: Some(
                tddy_service::gen::client_message::Intent::SubmitFeatureInput(
                    tddy_service::gen::SubmitFeatureInput {
                        text: "Build auth".to_string(),
                    },
                ),
            ),
        };
        sender
            .send(hello.encode_to_vec(), false)
            .await
            .map_err(|e| anyhow::anyhow!("send ClientMessage: {}", e))?;

        let mut found: Option<SessionRuntimeStatus> = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);

        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    if let Ok(msg) = ServerMessage::decode(&bytes[..]) {
                        if let Some(server_message::Event::SessionRuntimeStatus(s)) = msg.event {
                            found = Some(s);
                            break;
                        }
                    }
                }
                Ok(Some(Err(status))) => {
                    return Err(anyhow::anyhow!("stream error: {:?}", status));
                }
                Ok(None) => break,
                Err(_) => {}
            }
        }

        shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        server_handle.abort();

        let status = found.expect(
            "expected at least one SessionRuntimeStatus from LiveKit TddyRemote/Stream \
             (register `tddy.v1.TddyRemote` on the LiveKit RPC bridge and forward ServerMessage chunks)",
        );

        assert!(
            !status.session_id.is_empty(),
            "SessionRuntimeStatus.session_id must be non-empty for multi-session isolation"
        );
        assert!(
            status.goal.contains("Build auth") || status.goal.contains("plan"),
            "expected goal substring from active session; got {:?}",
            status.goal
        );

        Ok(())
    }
}
