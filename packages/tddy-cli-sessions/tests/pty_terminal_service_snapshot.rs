//! Acceptance: a room-carried terminal (`terminal.TerminalService/StreamTerminalIO`) opens with a
//! snapshot of the current screen and answers a snapshot request at any point — with no redraw
//! signal sent to the application.
//!
//! The PTY is real: a stub agent draws a full-screen TUI, then idles, printing `WINCHED` if it ever
//! receives SIGWINCH. The exposure is in memory, so the terminal's `RpcService` is driven directly.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-room.md

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::session_participant_metadata::SessionParticipantMetadata;
use tddy_daemon_extensions::{ExposedTerminal, TerminalAddress, TerminalExposure};
use tddy_rpc::{RequestMetadata, RequestTransport, ResponseBody, RpcMessage, RpcService, Status};
use tddy_service::proto::terminal::{TerminalInput, TerminalOutput};
use tokio::sync::mpsc;

const SESSION: &str = "1780828020298-room-snapshot";
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// A stub agent that draws a full-screen TUI and then idles, announcing any SIGWINCH it receives.
fn a_tui_agent(dir: &Path) -> String {
    let path = dir.join("agent.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         trap 'printf WINCHED' WINCH\n\
         printf '\\033[?1049h\\033[2J\\033[H\\033[1mROOM-TUI\\033[0m\\033[5;3Hstatus: idle'\n\
         while :; do sleep 0.1; done\n",
    )
    .expect("write the stub agent");
    tddy_testing_commons::stub_scripts::make_executable(&path);
    path.display().to_string()
}

/// Keeps the terminal service each exposure was handed, so the test can open a stream on it.
#[derive(Default)]
struct CapturingExposure {
    terminal: Mutex<Option<Arc<dyn RpcService>>>,
}

#[async_trait::async_trait]
impl TerminalExposure for CapturingExposure {
    async fn expose(
        &self,
        _at: &TerminalAddress,
        terminal: Arc<dyn RpcService>,
        _metadata_json: Option<String>,
    ) -> Result<ExposedTerminal, Status> {
        *self.terminal.lock().unwrap() = Some(terminal);
        Ok(ExposedTerminal::new(tokio::spawn(
            std::future::pending::<()>(),
        )))
    }
}

/// The session's room terminal, served over a real PTY running the TUI agent.
async fn a_room_terminal(dir: &Path) -> (Arc<CliSessionManager>, Arc<dyn RpcService>) {
    let manager = Arc::new(CliSessionManager::new());
    manager
        .start(
            SESSION,
            dir.to_path_buf(),
            "claude-opus-4-8",
            &a_tui_agent(dir),
            None,
            None,
        )
        .await
        .expect("the stub agent starts under a PTY");
    manager
        .expose_terminal_to_livekit(
            SESSION,
            SessionParticipantMetadata {
                agent: "claude".to_string(),
                session_id: SESSION.to_string(),
                ..Default::default()
            },
        )
        .await;
    let exposure = CapturingExposure::default();
    let at = TerminalAddress {
        room: "tddy-lobby".to_string(),
        identity: format!("daemon-host-{SESSION}"),
    };
    manager
        .ensure_exposed_terminal(SESSION, &at, &exposure)
        .await
        .expect("the terminal is exposed");
    let terminal = exposure
        .terminal
        .lock()
        .unwrap()
        .clone()
        .expect("a terminal service");
    // Let the agent draw before anyone attaches: poll with throwaway opens until a snapshot shows it.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let mut probe = RoomStream::open(&terminal, asking_for_a_snapshot()).await;
        let (_, snapshot) = probe.until_snapshot().await;
        if screen_after(&[&snapshot.data]).contains("status: idle") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the agent never drew its TUI"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    (manager, terminal)
}

/// One open `StreamTerminalIO`: the input half and the decoded output half.
struct RoomStream {
    input: mpsc::Sender<RpcMessage>,
    output: mpsc::Receiver<Result<Vec<u8>, Status>>,
}

impl RoomStream {
    async fn open(terminal: &Arc<dyn RpcService>, first: TerminalInput) -> Self {
        let (input, input_rx) = mpsc::channel(8);
        input.send(encoded(&first)).await.unwrap();
        let started = terminal
            .start_bidi_stream(
                "terminal.TerminalService",
                "StreamTerminalIO",
                RequestMetadata::over(RequestTransport::Direct),
                input_rx,
            )
            .await
            .expect("StreamTerminalIO opened");
        let ResponseBody::Streaming(output) = started.output else {
            panic!("StreamTerminalIO answered without a live stream");
        };
        RoomStream { input, output }
    }

    async fn send(&self, message: TerminalInput) {
        self.input.send(encoded(&message)).await.unwrap();
    }

    async fn next_frame(&mut self) -> TerminalOutput {
        match tokio::time::timeout(FRAME_TIMEOUT, self.output.recv()).await {
            Ok(Some(Ok(bytes))) => TerminalOutput::decode(&bytes[..]).expect("a decodable frame"),
            other => panic!("expected a terminal frame, got {other:?}"),
        }
    }

    /// Frames until one is a snapshot; returns the deltas before it and the snapshot.
    async fn until_snapshot(&mut self) -> (Vec<TerminalOutput>, TerminalOutput) {
        let mut before = Vec::new();
        loop {
            let frame = self.next_frame().await;
            if frame.snapshot {
                return (before, frame);
            }
            before.push(frame);
        }
    }
}

fn encoded<M: Message>(message: &M) -> RpcMessage {
    RpcMessage::new(
        message.encode_to_vec(),
        RequestMetadata::over(RequestTransport::Direct),
    )
}

fn asking_for_a_snapshot() -> TerminalInput {
    TerminalInput {
        request_snapshot: true,
        ..Default::default()
    }
}

/// What a VT shows after `bytes`, as text.
fn screen_after(chunks: &[&[u8]]) -> String {
    let mut vt = vt100::Parser::new(24, 80, 0);
    for chunk in chunks {
        vt.process(chunk);
    }
    vt.screen().contents()
}

#[tokio::test]
async fn a_room_open_renders_the_ptys_screen_then_only_later_deltas_without_a_redraw() {
    // Given a session whose agent has drawn a full-screen TUI over a real PTY
    let dir = tempfile::tempdir().unwrap();
    let (manager, terminal) = a_room_terminal(dir.path()).await;

    // When a client opens the room terminal asking for a snapshot
    let mut stream = RoomStream::open(&terminal, asking_for_a_snapshot()).await;
    let (before, snapshot) = stream.until_snapshot().await;

    // Then the very first frame is the snapshot, and it shows the TUI
    assert!(
        before.is_empty(),
        "no raw replay precedes the snapshot: {before:?}"
    );
    assert!(
        snapshot.data.starts_with(b"\x1bc"),
        "a snapshot begins with RIS"
    );
    let shown = screen_after(&[&snapshot.data]);
    assert!(
        shown.contains("ROOM-TUI"),
        "the snapshot shows the TUI: {shown:?}"
    );
    assert!(shown.contains("status: idle"));

    // And when the client types, the echo arrives as deltas contiguous from the sequence point
    stream
        .send(TerminalInput {
            data: b"hi".to_vec(),
            ..Default::default()
        })
        .await;
    let delta = stream.next_frame().await;
    assert_eq!(
        delta.start_offset, snapshot.end_offset,
        "the first delta starts at the snapshot"
    );
    assert_eq!(
        delta.end_offset,
        delta.start_offset + delta.data.len() as u64
    );

    // And the application was never sent a redraw signal
    tokio::time::sleep(Duration::from_millis(500)).await;
    stream.send(asking_for_a_snapshot()).await;
    let (_, later) = stream.until_snapshot().await;
    assert!(
        !screen_after(&[&later.data]).contains("WINCHED"),
        "the agent never received SIGWINCH"
    );
    manager.kill_all().await;
}

#[tokio::test]
async fn a_mid_stream_request_snapshot_resyncs_at_a_new_sequence_point() {
    // Given a client that opened the room terminal and typed into it
    let dir = tempfile::tempdir().unwrap();
    let (manager, terminal) = a_room_terminal(dir.path()).await;
    let mut stream = RoomStream::open(&terminal, asking_for_a_snapshot()).await;
    let (_, first) = stream.until_snapshot().await;
    stream
        .send(TerminalInput {
            data: b"abc".to_vec(),
            ..Default::default()
        })
        .await;

    // When it asks for a fresh snapshot mid-stream
    stream.send(asking_for_a_snapshot()).await;
    let (deltas, second) = stream.until_snapshot().await;

    // Then the new snapshot sits at a later sequence point, and shows what the first snapshot plus
    // the deltas between them show
    assert!(second.end_offset >= first.end_offset);
    let mut painted: Vec<&[u8]> = vec![&first.data];
    painted.extend(deltas.iter().map(|d| d.data.as_slice()));
    assert_eq!(screen_after(&[&second.data]), screen_after(&painted));
    manager.kill_all().await;
}
