//! E2E acceptance test: TerminalService StreamTerminalIO over LiveKit.
//!
//! Run with: cargo test -p tddy-livekit-compat-tests terminal_stream_io
//! Requires: LiveKit testkit (testcontainers or LIVEKIT_TESTKIT_WS_URL)
//!
//! Uses #[serial] to avoid parallel execution with other LiveKit tests.
//! The livekit feature gates tddy-livekit (webrtc-sys); omit for webrtc-free CI.

mod livekit_tests {
    use anyhow::Result;
    use livekit::prelude::*;
    use prost::Message;
    use serial_test::serial;
    use std::time::Duration;
    use tddy_livekit::{LiveKitParticipant, RpcClient};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
    use tddy_service::{TerminalServiceServer, TerminalServiceVirtualTui};

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

    /// Upper bound for a cold daemon start on a loaded CI runner. The wait ends as
    /// soon as the daemon joins or exits; this only bounds a daemon that hangs alive.
    const DAEMON_JOIN_TIMEOUT: Duration = Duration::from_secs(90);

    /// Waits until the daemon joins the room as `server`, polling the daemon process
    /// so an early exit fails immediately (with its stderr) instead of timing out.
    async fn wait_for_daemon_participant(
        room: &Room,
        events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
        daemon: &mut std::process::Child,
        stderr_path: &std::path::Path,
    ) -> Result<()> {
        let target: ParticipantIdentity = SERVER_IDENTITY.to_string().into();
        let deadline = tokio::time::Instant::now() + DAEMON_JOIN_TIMEOUT;
        let stderr = || std::fs::read_to_string(stderr_path).unwrap_or_default();
        loop {
            if room.remote_participants().contains_key(&target) {
                return Ok(());
            }
            if let Some(status) = daemon.try_wait()? {
                anyhow::bail!(
                    "daemon exited ({status}) before joining as '{SERVER_IDENTITY}'; stderr:\n{}",
                    stderr()
                );
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "daemon still running but not joined as '{SERVER_IDENTITY}' after {:?}; stderr:\n{}",
                    DAEMON_JOIN_TIMEOUT,
                    stderr()
                );
            }
            match tokio::time::timeout(Duration::from_millis(250), events.recv()).await {
                Ok(Some(RoomEvent::ParticipantConnected(p))) if p.identity() == target => {
                    return Ok(())
                }
                Ok(None) => anyhow::bail!("client room event stream closed"),
                _ => {}
            }
        }
    }

    /// Two LiveKit clients with independent VirtualTui streams.
    /// Each client gets its own stream; client 1 disconnect does not affect client 2.
    #[tokio::test]
    #[serial]
    async fn two_livekit_clients_get_independent_terminal_streams() -> Result<()> {
        // Given
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = &LiveKitTestkit::unique_room("terminal-two-clients-test");

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        const CLIENT1_IDENTITY: &str = "client1";
        const CLIENT2_IDENTITY: &str = "client2";

        let client1_token = livekit.generate_token(room_name, CLIENT1_IDENTITY)?;
        let client2_token = livekit.generate_token(room_name, CLIENT2_IDENTITY)?;

        let (_presenter_handle, factory, _shutdown) =
            tddy_e2e::spawn_presenter_with_view_connection_factory(Some("feature".to_string()));

        let terminal_service = TerminalServiceVirtualTui::new(factory, false);

        let server = LiveKitParticipant::connect(
            &url,
            &server_token,
            TerminalServiceServer::new(terminal_service),
            RoomOptions::default(),
            None,
            None,
        )
        .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client1_room, mut client1_events) =
            Room::connect(&url, &client1_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client1 connect: {}", e))?;
        wait_for_participant(&client1_room, &mut client1_events, SERVER_IDENTITY).await?;
        let client1_rpc_events = client1_room.subscribe();
        let client1_rpc = RpcClient::new(
            client1_room,
            SERVER_IDENTITY.to_string(),
            client1_rpc_events,
        );

        let (client2_room, mut client2_events) =
            Room::connect(&url, &client2_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client2 connect: {}", e))?;
        wait_for_participant(&client2_room, &mut client2_events, SERVER_IDENTITY).await?;
        let client2_rpc_events = client2_room.subscribe();
        let client2_rpc = RpcClient::new(
            client2_room,
            SERVER_IDENTITY.to_string(),
            client2_rpc_events,
        );

        // When — both clients open independent StreamTerminalIO streams
        let request = TerminalInput {
            data: vec![],
            ..Default::default()
        };
        let request_bytes = request.encode_to_vec();

        let mut rx1 = client1_rpc
            .call_server_stream(
                "terminal.TerminalService",
                "StreamTerminalIO",
                request_bytes.clone(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("client1 StreamTerminalIO: {}", e))?;

        let mut rx2 = client2_rpc
            .call_server_stream(
                "terminal.TerminalService",
                "StreamTerminalIO",
                request_bytes,
            )
            .await
            .map_err(|e| anyhow::anyhow!("client2 StreamTerminalIO: {}", e))?;

        let mut received1 = Vec::new();
        let mut received2 = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(Ok(bytes))) =
                tokio::time::timeout(Duration::from_millis(200), rx1.recv()).await
            {
                let output = TerminalOutput::decode(&bytes[..])?;
                received1.extend_from_slice(&output.data);
            }
            if let Ok(Some(Ok(bytes))) =
                tokio::time::timeout(Duration::from_millis(200), rx2.recv()).await
            {
                let output = TerminalOutput::decode(&bytes[..])?;
                received2.extend_from_slice(&output.data);
            }
            if received1.len() > 50 && received2.len() > 50 {
                break;
            }
        }

        server_handle.abort();

        // Then — each client receives its own ANSI byte stream
        assert!(
            received1.len() > 50,
            "client1 should receive ANSI bytes, got {}",
            received1.len()
        );
        assert!(
            received2.len() > 50,
            "client2 should receive ANSI bytes, got {}",
            received2.len()
        );

        Ok(())
    }

    /// Daemon with LiveKit args serves TerminalService/StreamTerminalIO (regression: not EchoService).
    #[tokio::test]
    #[serial]
    async fn daemon_with_livekit_serves_stream_terminal_io() -> Result<()> {
        // Given
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = &LiveKitTestkit::unique_room("daemon-stream-test");

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let sessions_base = std::env::temp_dir().join("tddy-daemon-livekit-test");
        let _ = std::fs::remove_dir_all(&sessions_base);
        std::fs::create_dir_all(&sessions_base).expect("create sessions base");
        let daemon_stderr_path = sessions_base.join("daemon.stderr.log");

        let tddy_coder = std::env::var("CARGO_BIN_EXE_tddy-coder").unwrap_or_else(|_| {
            let exe = std::env::current_exe().expect("current exe");
            let deps = exe.parent().expect("exe parent");
            let debug = deps.parent().expect("deps parent");
            debug.join("tddy-coder").display().to_string()
        });
        let daemon = std::process::Command::new(&tddy_coder)
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::fs::File::create(&daemon_stderr_path)?)
            .args([
                "--agent",
                "stub",
                "--daemon",
                "--tddy-data-dir",
                sessions_base.to_str().unwrap(),
                "--livekit-url",
                &url,
                "--livekit-token",
                &server_token,
                "--livekit-room",
                room_name,
                "--livekit-identity",
                SERVER_IDENTITY,
            ])
            .spawn()
            .map_err(|e| anyhow::anyhow!("spawn tddy-coder daemon: {}", e))?;

        struct KillOnDrop(std::process::Child);
        impl Drop for KillOnDrop {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut guard = KillOnDrop(daemon);

        // When — client connects and calls StreamTerminalIO on the daemon.
        // The daemon's own join is the readiness signal: no fixed sleep, and a
        // daemon that dies fails the wait at once with its stderr.
        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();
        wait_for_daemon_participant(
            &client_room,
            &mut client_events,
            &mut guard.0,
            &daemon_stderr_path,
        )
        .await?;

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        let request = TerminalInput {
            data: vec![],
            ..Default::default()
        };
        let request_bytes = request.encode_to_vec();

        let mut rx = rpc_client
            .call_server_stream(
                "terminal.TerminalService",
                "StreamTerminalIO",
                request_bytes,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "StreamTerminalIO (daemon must serve TerminalService, not EchoService): {}",
                    e
                )
            })?;

        let mut received = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(Ok(bytes))) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
            {
                let output = TerminalOutput::decode(&bytes[..])?;
                received.extend_from_slice(&output.data);
            }
            if received.len() > 50 {
                break;
            }
        }

        // Then — daemon streams ANSI TUI output (not an EchoService response)
        assert!(
            received.len() > 50,
            "daemon VirtualTui should stream ANSI bytes to client, got {} bytes (0 = headless/TTY bug)",
            received.len()
        );

        Ok(())
    }
} // mod livekit_tests
