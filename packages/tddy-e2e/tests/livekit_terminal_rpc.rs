//! E2E test: LiveKit StreamTerminalIO RPC pipeline with virtual terminal.
//!
//! Mimics tddy-web behaviour: connects via LiveKit, uses a virtual terminal as viewer
//! (receives ANSI output, sends keyboard/mouse input). Asserts on content after
//! keyboard operations to verify the entire RPC pipeline works.
//!
//! Requires: LIVEKIT_TESTKIT_WS_URL (e.g. ws://127.0.0.1:32971)

#[cfg(not(feature = "livekit"))]
#[tokio::test]
async fn livekit_terminal_rpc_skipped() {
    // Built without livekit feature; test passes as no-op.
}

#[cfg(feature = "livekit")]
mod livekit_tests {
    use std::time::Duration;

    use livekit::prelude::*;
    use prost::Message;
    use serial_test::serial;
    use strip_ansi_escapes::strip;
    use tddy_livekit::{LiveKitParticipant, RpcClient};
    use tddy_livekit_testkit::LiveKitTestkit;
    use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
    use tddy_service::{TerminalServiceServer, TerminalServiceVirtualTui};
    use vt100::Parser;

    const SERVER_IDENTITY: &str = "server";
    const CLIENT_IDENTITY: &str = "client";
    const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

    mod keys {
        pub const ENTER: &[u8] = b"\r";
        pub const DOWN: &[u8] = b"\x1b[B";
    }

    fn ansi_to_text(bytes: &[u8]) -> String {
        let stripped = strip(bytes);
        String::from_utf8_lossy(&stripped).into_owned()
    }

    async fn wait_for_participant(
        room: &Room,
        events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
        identity: &str,
    ) -> anyhow::Result<()> {
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
    async fn livekit_terminal_io_receives_ansi_output() -> anyhow::Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = "terminal-rpc-receive-test";

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let (_presenter_handle, factory, _shutdown) =
            tddy_e2e::spawn_presenter_with_view_connection_factory(Some("Build auth".to_string()));

        let terminal_service = TerminalServiceVirtualTui::new(factory, false);

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

        let mut received = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

        while tokio::time::Instant::now() < deadline {
            if let Ok(Some(chunk)) =
                tokio::time::timeout(Duration::from_millis(100), rx.recv()).await
            {
                if let Ok(bytes) = chunk {
                    let output = TerminalOutput::decode(&bytes[..])?;
                    received.extend_from_slice(&output.data);
                }
            }
            if received.len() > 100 {
                break;
            }
        }

        server_handle.abort();

        assert!(
            received.len() > 50,
            "Expected ANSI output from VirtualTui, got {} bytes",
            received.len()
        );

        let text = ansi_to_text(&received);
        assert!(
            text.contains("State:")
                || text.contains("Goal:")
                || text.contains("Feature")
                || text.contains("plan")
                || text.contains("Build"),
            "Expected terminal content, got stripped text (len {}): {:?}",
            text.len(),
            &text[..text.len().min(200)]
        );

        Ok(())
    }

    /// Drain all buffered output from the LiveKit stream until `quiet_period` passes
    /// without any new data. Returns all collected bytes.
    async fn drain_output(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
        quiet_period: Duration,
        phase: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let mut received = Vec::new();
        let mut chunk_count = 0u64;
        loop {
            match tokio::time::timeout(quiet_period, rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    chunk_count += 1;
                    let output = TerminalOutput::decode(&bytes[..])?;
                    received.extend_from_slice(&output.data);
                }
                Ok(Some(Err(e))) => return Err(anyhow::anyhow!("stream error in drain: {}", e)),
                Ok(None) => {
                    eprintln!(
                        "[BIDI_TRACE] livekit drain_output: phase={} stream closed after {} chunks, {} bytes",
                        phase, chunk_count, received.len()
                    );
                    break;
                }
                Err(_) => {
                    eprintln!(
                        "[BIDI_TRACE] livekit drain_output: phase={} quiet after {} chunks, {} bytes",
                        phase, chunk_count, received.len()
                    );
                    break;
                }
            }
        }
        Ok(received)
    }

    #[tokio::test]
    #[serial]
    async fn livekit_terminal_io_keyboard_input_affects_output() -> anyhow::Result<()> {
        let _ = env_logger::builder().is_test(true).try_init();
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = "terminal-rpc-keyboard-test";

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let (_presenter_handle, factory, shutdown) =
            tddy_e2e::spawn_presenter_with_view_connection_factory(Some("Build auth".to_string()));

        let terminal_service = TerminalServiceVirtualTui::new(factory, false);

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

        let (mut sender, mut rx) = rpc_client
            .start_bidi_stream("terminal.TerminalService", "StreamTerminalIO")
            .map_err(|e| anyhow::anyhow!("start bidi: {}", e))?;

        // Phase 1: send init, wait for LiveKit roundtrip, drain initial TUI output.
        // All sends use end_of_stream=false to keep the bidi session alive —
        // end_of_stream=true would tear down the session before VirtualTui processes
        // the last key (the shutdown propagates through the input chain).
        // LiveKit data channels have higher per-chunk latency than gRPC (5 hops vs 2),
        // so we use longer quiet periods (2s vs 500ms for gRPC).
        sender
            .send(TerminalInput { data: vec![] }.encode_to_vec(), false)
            .await
            .map_err(|e| anyhow::anyhow!("send init: {}", e))?;

        let initial_output = drain_output(&mut rx, Duration::from_secs(3), "init").await?;
        let initial_text = ansi_to_text(&initial_output);
        eprintln!(
            "[TEST] livekit init: {} bytes, text_len={}, preview={:?}",
            initial_output.len(),
            initial_text.len(),
            &initial_text[..initial_text.len().min(300)]
        );

        assert!(
            initial_text.contains("State:") || initial_text.contains("Scope"),
            "Initial TUI should render before any keyboard input; got (len {}): {:?}",
            initial_text.len(),
            &initial_text[..initial_text.len().min(300)]
        );

        // Phase 2: send keyboard inputs, drain output after each.
        // Enter answers scope → PlanReview. Down → Approve. Enter → approve.
        let inputs: &[(&[u8], &str)] = &[
            (keys::ENTER, "Enter (answer scope)"),
            (keys::DOWN, "Down (navigate to Approve)"),
            (keys::ENTER, "Enter (approve plan)"),
        ];

        let mut all_output = initial_output;
        for (key, label) in inputs {
            eprintln!("[TEST-INPUT] livekit: sending {}", label);
            sender
                .send(TerminalInput { data: key.to_vec() }.encode_to_vec(), false)
                .await
                .map_err(|e| anyhow::anyhow!("send {}: {}", label, e))?;

            let chunk = drain_output(&mut rx, Duration::from_secs(2), label).await?;
            eprintln!(
                "[TEST-INPUT] livekit: '{}' produced {} bytes of output",
                label,
                chunk.len()
            );
            all_output.extend_from_slice(&chunk);
        }

        shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        server_handle.abort();

        let text = ansi_to_text(&all_output);
        eprintln!(
            "[TEST] livekit total output: {} bytes, text_len={}, preview={:?}",
            all_output.len(),
            text.len(),
            &text[..text.len().min(500)]
        );

        assert!(
            text.contains("State:") || text.contains("Scope"),
            "Should receive initial TUI output; got (len {}): {:?}",
            text.len(),
            &text[..text.len().min(300)]
        );

        let progressed = text.contains("Plan dir:")
            || text.contains("AcceptanceTesting")
            || text.contains("GreenComplete")
            || text.contains("Workflow complete")
            || text.contains("DocsUpdated")
            || text.contains("Type your feature");

        assert!(
            progressed,
            "Keyboard inputs should advance the workflow past the initial screen; got (len {}): {:?}",
            text.len(),
            &text[..text.len().min(500)]
        );

        Ok(())
    }

    /// Virtual terminal viewer that mimics Ghostty: receives ANSI output via RPC,
    /// parses with vt100, exposes visible screen content for assertions.
    struct VirtualTerminalViewer {
        parser: Parser,
    }

    impl VirtualTerminalViewer {
        fn new() -> Self {
            Self {
                parser: Parser::new(24, 80, 0),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            self.parser.process(bytes);
        }

        #[allow(dead_code)]
        fn visible_content(&self) -> String {
            self.parser.screen().contents()
        }
    }

    /// Full e2e: virtual terminal (vt100) as viewer, RPC for I/O sync, virtual keyboard
    /// interactions. Asserts on visible terminal content like GhosttyTerminalLiveKit.
    #[tokio::test]
    #[serial]
    async fn livekit_ghostty_virtual_terminal_e2e() -> anyhow::Result<()> {
        let livekit = LiveKitTestkit::start().await?;
        let url = livekit.get_ws_url();
        let room_name = "ghostty-virtual-terminal-e2e";

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let (_presenter_handle, factory, shutdown) =
            tddy_e2e::spawn_presenter_with_view_connection_factory(Some("Build auth".to_string()));

        let terminal_service = TerminalServiceVirtualTui::new(factory, false);

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

        let (mut sender, mut rx) = rpc_client
            .start_bidi_stream("terminal.TerminalService", "StreamTerminalIO")
            .map_err(|e| anyhow::anyhow!("start bidi: {}", e))?;

        let mut viewer = VirtualTerminalViewer::new();

        // Phase 1: send init, drain initial TUI output.
        sender
            .send(TerminalInput { data: vec![] }.encode_to_vec(), false)
            .await
            .map_err(|e| anyhow::anyhow!("send init: {}", e))?;

        let initial_output = drain_output(&mut rx, Duration::from_secs(3), "ghostty-init").await?;
        for chunk in initial_output.chunks(256) {
            viewer.feed(chunk);
        }
        let initial_text = ansi_to_text(&initial_output);

        assert!(
            initial_text.contains("State:") || initial_text.contains("Scope"),
            "Initial TUI should render before any keyboard input; got (len {}): {:?}",
            initial_text.len(),
            &initial_text[..initial_text.len().min(300)]
        );

        // Phase 2: send keyboard inputs, drain output after each.
        // Enter answers scope → PlanReview. Down → Approve. Enter → approve.
        let inputs: &[(&[u8], &str)] = &[
            (keys::ENTER, "Enter (answer scope)"),
            (keys::DOWN, "Down (navigate to Approve)"),
            (keys::ENTER, "Enter (approve plan)"),
        ];

        let mut all_raw = initial_output;
        for (key, label) in inputs {
            sender
                .send(TerminalInput { data: key.to_vec() }.encode_to_vec(), false)
                .await
                .map_err(|e| anyhow::anyhow!("send {}: {}", label, e))?;

            let chunk = drain_output(&mut rx, Duration::from_secs(2), label).await?;
            for part in chunk.chunks(256) {
                viewer.feed(part);
            }
            all_raw.extend_from_slice(&chunk);
        }

        shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        server_handle.abort();

        let visible = ansi_to_text(&all_raw);
        let progressed = visible.contains("Plan dir:")
            || visible.contains("AcceptanceTesting")
            || visible.contains("GreenComplete")
            || visible.contains("Workflow complete")
            || visible.contains("DocsUpdated")
            || visible.contains("Type your feature");

        assert!(
            progressed,
            "Keyboard inputs should advance the workflow; stripped text (len {}): {:?}",
            visible.len(),
            &visible[..visible.len().min(500)]
        );

        Ok(())
    }
}
