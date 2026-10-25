//! Acceptance: a bidi terminal stream can be resynced at any point — on the client's request, or by
//! the server when the client fell too far behind to be sent every byte.
//!
//! As in `stream_snapshot_acceptance.rs`, the client is a reference VT fed exactly what the stream
//! sends, compared with a reference VT that watched every byte. The new point here is that a
//! snapshot can arrive in the *middle* of a stream, and the offsets after it must restart from its
//! sequence point.

mod support;

use std::sync::Arc;

use support::{
    a_snapshot_open, an_input, typing_and_requesting_a_snapshot, BidiSession, StubTerminal,
    TerminalServiceHost,
};
use tddy_terminal_rpc::proto::terminal_session::{SessionTerminalOutput, StreamReplayMode};

const ROWS: u16 = 24;
const COLS: u16 = 80;

fn tui_startup() -> Vec<u8> {
    b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H\x1b[1m header \x1b[0m\x1b[2;23r".to_vec()
}

fn tui_diff(frame: usize) -> Vec<u8> {
    format!(
        "\x1b[{};2H\x1b[33mrow {frame:05}\x1b[0m\x1b[24;1H {frame:06} ",
        2 + frame % 21
    )
    .into_bytes()
}

fn screen_of(parser: &vt100::Parser) -> (String, (u16, u16)) {
    let screen = parser.screen();
    (
        String::from_utf8_lossy(&screen.contents_formatted()).into_owned(),
        screen.cursor_position(),
    )
}

fn a_vt_that_watched(output: &[u8]) -> vt100::Parser {
    let mut vt = vt100::Parser::new(ROWS, COLS, 0);
    vt.process(output);
    vt
}

/// A client's VT, painting what a stream delivers and checking the offsets as it goes.
struct Client {
    vt: vt100::Parser,
    next_start: Option<u64>,
    snapshots_seen: usize,
}

impl Client {
    fn new() -> Self {
        Client {
            vt: vt100::Parser::new(ROWS, COLS, 0),
            next_start: None,
            snapshots_seen: 0,
        }
    }

    fn paint(&mut self, frame: &SessionTerminalOutput) {
        if frame.snapshot {
            self.snapshots_seen += 1;
            self.vt.process(&frame.data);
            self.next_start = Some(frame.end_offset);
        } else if !frame.data.is_empty() {
            let expected = self.next_start.expect("no delta arrives before a snapshot");
            assert_eq!(
                frame.start_offset, expected,
                "a delta starts where the last frame ended"
            );
            assert_eq!(
                frame.end_offset,
                frame.start_offset + frame.data.len() as u64
            );
            self.vt.process(&frame.data);
            self.next_start = Some(frame.end_offset);
        }
    }

    /// Paint frames off `stream` until a snapshot beyond the `already` seen ones has been painted.
    async fn paint_until_snapshot_number(&mut self, stream: &mut BidiSession, wanted: usize) {
        while self.snapshots_seen < wanted {
            let frame = stream
                .next_frame_or_close()
                .await
                .expect("the stream stays open until the snapshot arrives");
            self.paint(&frame);
        }
    }

    /// Paint frames off `stream` until the client has reached offset `tip`.
    async fn paint_until(&mut self, stream: &mut BidiSession, tip: u64) {
        while self.next_start != Some(tip) {
            let frame = stream
                .next_frame_or_close()
                .await
                .expect("the stream stays open until the tip is reached");
            self.paint(&frame);
        }
    }
}

#[tokio::test]
async fn a_mid_stream_request_yields_a_snapshot_that_renders_the_servers_screen_then_only_later_deltas(
) {
    // Given a client following a TUI over a snapshot-opened bidi stream
    let terminal = Arc::new(StubTerminal::sized(ROWS, COLS));
    let mut output = tui_startup();
    terminal.write(&output);
    let host = TerminalServiceHost::sharing_terminal(Arc::clone(&terminal));
    let mut stream = host.open_bidi(a_snapshot_open()).await;
    let mut client = Client::new();
    client.paint_until_snapshot_number(&mut stream, 1).await;
    for frame in 0..5 {
        let diff = tui_diff(frame);
        terminal.write(&diff);
        output.extend_from_slice(&diff);
    }
    client.paint_until(&mut stream, output.len() as u64).await;

    // When the client types a key and asks for a fresh snapshot on the same message
    stream.send(typing_and_requesting_a_snapshot(b"q", 1)).await;
    client.paint_until_snapshot_number(&mut stream, 2).await;

    // Then the keystroke reached the PTY, and the snapshot shows the screen at its sequence point
    assert_eq!(
        host.typed_into_pty(),
        vec![(bytes::Bytes::from_static(b"q"), 1)]
    );
    assert_eq!(
        screen_of(&client.vt),
        screen_of(&a_vt_that_watched(&output))
    );

    // And when the TUI keeps drawing, only the later deltas follow, contiguous from the snapshot
    for frame in 5..9 {
        let diff = tui_diff(frame);
        terminal.write(&diff);
        output.extend_from_slice(&diff);
    }
    client.paint_until(&mut stream, output.len() as u64).await;
    assert_eq!(
        screen_of(&client.vt),
        screen_of(&a_vt_that_watched(&output))
    );
}

#[tokio::test]
async fn a_lagged_subscriber_is_resynced_with_a_snapshot_not_a_hole() {
    // Given a snapshot-opened stream whose live broadcast holds a single chunk
    let terminal = Arc::new(StubTerminal::sized_with_broadcast_capacity(ROWS, COLS, 1));
    let mut output = tui_startup();
    terminal.write(&output);
    let host = TerminalServiceHost::sharing_terminal(Arc::clone(&terminal));
    let mut stream = host.open_bidi(a_snapshot_open()).await;
    let mut client = Client::new();
    client.paint_until_snapshot_number(&mut stream, 1).await;

    // When the TUI bursts far more output than the stream can hold while the client is not reading
    for frame in 0..400 {
        let diff = tui_diff(frame);
        terminal.write(&diff);
        output.extend_from_slice(&diff);
    }

    // Then the client is sent a fresh snapshot instead of a hole, and ends on the server's screen
    client.paint_until(&mut stream, output.len() as u64).await;
    assert!(
        client.snapshots_seen >= 2,
        "the lag was answered with a snapshot (saw {} snapshots)",
        client.snapshots_seen
    );
    assert_eq!(
        screen_of(&client.vt),
        screen_of(&a_vt_that_watched(&output))
    );
}

#[tokio::test]
async fn tail_streams_keep_their_lagged_behaviour() {
    // Given a TAIL-opened stream — a legacy client that reads bytes, not snapshots — on a terminal
    // whose broadcast holds a single chunk
    let terminal = Arc::new(StubTerminal::sized_with_broadcast_capacity(ROWS, COLS, 1));
    terminal.write(&tui_startup());
    let host = TerminalServiceHost::sharing_terminal(Arc::clone(&terminal));
    let mut stream = host
        .open_bidi(
            tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
                mode: StreamReplayMode::Tail as i32,
                ..an_input()
            },
        )
        .await;

    // When the TUI bursts past the broadcast's capacity
    for frame in 0..400 {
        terminal.write(&tui_diff(frame));
    }

    // Then that client is never handed a snapshot it does not know how to read
    let frames = stream.frames(500).await;
    assert!(!frames.is_empty(), "the stream delivered output");
    assert!(
        frames.iter().all(|frame| !frame.snapshot),
        "a TAIL stream carries no snapshot frames"
    );
}
