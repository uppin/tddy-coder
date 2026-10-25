//! Acceptance: a terminal stream opened with `STREAM_REPLAY_MODE_SNAPSHOT` shows the client the
//! current screen, then only what changed after it.
//!
//! A reconnecting client is a fresh VT that saw none of the application's output. These tests give
//! it exactly what the stream sends and compare its screen with a reference VT that watched every
//! byte. The offsets on every frame are checked as the client would check them: the first delta
//! starts at the snapshot's sequence point and each one starts where the previous ended.

mod support;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use support::{a_snapshot_open, an_output_request, StubTerminal, TerminalServiceHost};
use tddy_terminal_rpc::proto::terminal_session::{SessionTerminalOutput, StreamReplayMode};

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// A full-screen TUI's startup: alternate screen, a scroll region, hidden cursor, mouse tracking.
fn tui_startup() -> Vec<u8> {
    b"\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[2J\x1b[H\x1b[1;35m header \x1b[0m\x1b[2;23r"
        .to_vec()
}

/// One frame's worth of diffs — cursor-addressed writes, never a full redraw.
fn tui_diff(frame: usize) -> Vec<u8> {
    format!(
        "\x1b[{};4H\x1b[36mstep {frame:05}\x1b[0m\x1b[24;1H\x1b[7m {frame:06} \x1b[0m",
        2 + frame % 21
    )
    .into_bytes()
}

fn diffs(frames: std::ops::Range<usize>) -> Vec<u8> {
    frames.flat_map(tui_diff).collect()
}

/// A terminal that has been running a TUI for a while, and the host serving it.
fn a_host_running_a_tui() -> (TerminalServiceHost, Arc<StubTerminal>, Vec<u8>) {
    let terminal = Arc::new(StubTerminal::sized(ROWS, COLS));
    let mut output = tui_startup();
    output.extend(diffs(0..400));
    terminal.write(&output);
    (
        TerminalServiceHost::sharing_terminal(Arc::clone(&terminal)),
        terminal,
        output,
    )
}

/// The cells and cursor a VT shows, as one comparable value.
fn screen_of(parser: &vt100::Parser) -> (String, (u16, u16), bool) {
    let screen = parser.screen();
    (
        String::from_utf8_lossy(&screen.contents_formatted()).into_owned(),
        screen.cursor_position(),
        screen.alternate_screen(),
    )
}

fn a_vt_that_watched(output: &[u8]) -> vt100::Parser {
    let mut vt = vt100::Parser::new(ROWS, COLS, 0);
    vt.process(output);
    vt
}

fn data_frames(frames: Vec<SessionTerminalOutput>) -> Vec<SessionTerminalOutput> {
    frames
        .into_iter()
        .filter(|frame| !frame.data.is_empty() || frame.snapshot)
        .collect()
}

#[tokio::test]
async fn a_snapshot_open_renders_the_servers_screen_then_only_later_deltas() {
    // Given a terminal that has been running a full-screen TUI for a while
    let (host, terminal, mut output) = a_host_running_a_tui();

    // When a client opens the bidi stream asking for a snapshot
    let mut stream = host.open_bidi(a_snapshot_open()).await;
    let snapshot = stream.frames(1).await.remove(0);

    // Then the first frame is a snapshot at the capture tip, serialized at the PTY's grid
    assert!(snapshot.snapshot, "the first frame is the snapshot");
    assert_eq!(
        (
            snapshot.end_offset,
            snapshot.snapshot_rows,
            snapshot.snapshot_cols
        ),
        (output.len() as u64, ROWS as u32, COLS as u32)
    );

    // And when the TUI goes on drawing, the client receives only those later bytes, each frame
    // starting exactly where the previous one ended
    let later = diffs(400..403);
    for frame in 400..403 {
        terminal.write(&tui_diff(frame));
    }
    output.extend_from_slice(&later);
    let deltas = data_frames(stream.frames(3).await);
    let mut expected_start = snapshot.end_offset;
    for delta in &deltas {
        assert_eq!(
            delta.start_offset, expected_start,
            "a delta starts where the last ended"
        );
        assert_eq!(
            delta.end_offset,
            delta.start_offset + delta.data.len() as u64
        );
        expected_start = delta.end_offset;
    }
    let delivered: Vec<u8> = deltas.iter().flat_map(|d| d.data.clone()).collect();
    assert_eq!(
        delivered, later,
        "exactly the bytes after the sequence point"
    );

    // And a client that saw only the snapshot and the deltas shows what a client that watched
    // every byte shows
    let mut reconnected = vt100::Parser::new(ROWS, COLS, 0);
    reconnected.process(&snapshot.data);
    reconnected.process(&delivered);
    assert_eq!(
        screen_of(&reconnected),
        screen_of(&a_vt_that_watched(&output))
    );
}

#[tokio::test]
async fn a_snapshot_open_sends_no_redraw_signal_and_no_prologue_frame() {
    // Given a terminal running a TUI that enabled mouse tracking long ago
    let (host, terminal, _) = a_host_running_a_tui();

    // When a client opens with a snapshot at the PTY's own grid
    let mut stream = host
        .open_bidi(
            tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
                initial_rows: ROWS as u32,
                initial_cols: COLS as u32,
                ..a_snapshot_open()
            },
        )
        .await;
    let first = stream.frames(1).await.remove(0);

    // Then the very first frame is the snapshot itself, and the application was never nudged
    assert!(first.snapshot, "no prologue frame precedes the snapshot");
    assert_eq!(terminal.redraws.load(Ordering::SeqCst), 0, "no SIGWINCH");
    assert!(
        terminal.resizes.lock().unwrap().is_empty(),
        "an unchanged grid is not resized"
    );
}

#[tokio::test]
async fn a_snapshot_open_at_a_new_grid_resizes_first_and_snapshots_at_it() {
    // Given a terminal nobody holds the control lease of
    let (host, terminal, _) = a_host_running_a_tui();

    // When a client opens with a snapshot at a different grid
    let mut stream = host
        .open_bidi(
            tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
                initial_rows: 40,
                initial_cols: 120,
                ..a_snapshot_open()
            },
        )
        .await;
    let first = stream.frames(1).await.remove(0);

    // Then the PTY was resized before the snapshot was taken, and the snapshot is at the new grid
    assert_eq!(*terminal.resizes.lock().unwrap(), vec![(40, 120)]);
    assert_eq!((first.snapshot_rows, first.snapshot_cols), (40, 120));
    assert_eq!(
        terminal.redraws.load(Ordering::SeqCst),
        0,
        "no second SIGWINCH"
    );
}

#[tokio::test]
async fn stream_terminal_output_honours_snapshot_mode_too() {
    // Given a terminal running a TUI
    let (host, _terminal, output) = a_host_running_a_tui();

    // When a non-bidi client opens the output stream asking for a snapshot
    let frames: Vec<SessionTerminalOutput> = host
        .stream_frames(
            "StreamTerminalOutput",
            tddy_terminal_rpc::proto::terminal_session::StreamTerminalOutputRequest {
                mode: StreamReplayMode::Snapshot as i32,
                ..an_output_request()
            },
            1,
        )
        .await;

    // Then it is answered with the same snapshot
    assert!(frames[0].snapshot);
    assert_eq!(frames[0].end_offset, output.len() as u64);
}

#[tokio::test]
async fn a_terminal_whose_size_is_unknown_refuses_a_snapshot() {
    // Given a terminal whose screen model was never told the PTY's size
    let terminal = Arc::new(StubTerminal::new());
    terminal.write(b"hello");
    let host = TerminalServiceHost::sharing_terminal(terminal);

    // When a client asks for a snapshot
    let status = host.refusal_on_bidi(a_snapshot_open()).await;

    // Then it is refused rather than shown a screen serialized at a guessed size
    assert_eq!(status.code(), tddy_rpc::Code::FailedPrecondition);
}
