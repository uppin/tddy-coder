//! Acceptance tests for `StreamSessionTerminalIO`, the one bidirectional method in the ninety-method
//! surface `#unbundle` is splitting up.
//!
//! Every test drives the stream the way a bidi transport does — `start_bidi_stream` at the
//! registered coordinate, encoded `SessionTerminalInput` messages pushed into the input channel,
//! encoded `SessionTerminalOutput` frames read back off the output half. Nothing here calls the
//! handler directly, because what makes this method different from the other eight is precisely
//! the wiring: an input stream and an output stream alive at the same time on one call.

mod support;

use bytes::Bytes;
use support::{
    a_snapshot_open, an_input, typing, StubTerminal, TerminalServiceHost, SCREEN_ID, SESSION_ID,
};
use tddy_terminal_rpc::proto::terminal_session::{
    SessionTerminalInput, SessionTerminalOutput, StreamReplayMode,
};

/// A second browser tab of the same user, which can steal terminal control from the first.
const OTHER_SCREEN_ID: &str = "screen-ada-phone";

/// The grid the driving screen set on the PTY.
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// A host whose main terminal's screen model knows the PTY's grid — the precondition for a
/// snapshot — with `output` already printed.
fn a_host_with_a_sized_terminal(output: &[u8]) -> TerminalServiceHost {
    let terminal = std::sync::Arc::new(StubTerminal::sized(ROWS, COLS));
    terminal.write(output);
    TerminalServiceHost::sharing_terminal(terminal)
}

/// A snapshot open whose opening message already carries keystrokes.
fn typing_into_a_snapshot_open(data: &[u8], input_offset: u64) -> SessionTerminalInput {
    SessionTerminalInput {
        mode: StreamReplayMode::Snapshot as i32,
        ..typing(data, input_offset)
    }
}

/// The refused chunk's cumulative offset and the reason, asserting the frame carries no output.
fn refusal_of(frame: &SessionTerminalOutput) -> (u64, &str) {
    assert!(frame.data.is_empty(), "a refusal frame carries no output");
    (frame.refused_input_offset, frame.input_refusal.as_str())
}

#[tokio::test]
async fn replays_the_current_last_frame_before_any_live_byte() {
    // Given a session whose terminal has already produced ten bytes
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");

    // When a bidi client opens the stream in TAIL mode
    let mut session = host.open_bidi(an_input()).await;

    // Then the first frame is the tail of the ring, anchored at its absolute offsets
    assert_eq!(
        session.frames(1).await,
        vec![SessionTerminalOutput {
            data: b"6789".to_vec(),
            acked_input_offset: 0,
            start_offset: 6,
            end_offset: 10,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
            snapshot: false,
            snapshot_rows: 0,
            snapshot_cols: 0,
            refused_input_offset: 0,
            input_refusal: String::new(),
        }]
    );
}

#[tokio::test]
async fn carries_the_open_frames_own_keystrokes_to_the_pty() {
    // Given a session with a live terminal
    let host = TerminalServiceHost::serving_main_terminal(b"ready");

    // When the client opens the stream with keystrokes already in the first message
    let mut session = host.open_bidi(typing(b"whoami\r", 7)).await;
    session.frames(1).await;

    // Then those keystrokes reached the PTY with their cumulative offset
    assert_eq!(
        host.typed_into_pty(),
        vec![(Bytes::from_static(b"whoami\r"), 7)]
    );
}

#[tokio::test]
async fn carries_every_subsequent_keystroke_chunk_in_order() {
    // Given an open bidi stream whose first message typed nothing
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let mut session = host.open_bidi(an_input()).await;
    session.frames(1).await;

    // When the client types two more chunks
    session.send(typing(b"ls", 2)).await;
    session.send(typing(b" -la\r", 7)).await;

    // Then both reached the PTY in order, each with the cumulative offset it advanced to. The
    // forwarder is a spawned task, so the live output frame the terminal produces afterwards is
    // what says it has caught up — no sleep.
    host.terminal().write(b"total 0\r\n");
    session.frames(1).await;
    assert_eq!(
        host.typed_into_pty(),
        vec![
            (Bytes::from_static(b"ls"), 2),
            (Bytes::from_static(b" -la\r"), 7),
        ]
    );
}

#[tokio::test]
async fn delivers_output_the_terminal_produces_after_the_stream_opened() {
    // Given an open bidi stream on a terminal whose ring has been replayed
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let mut session = host.open_bidi(an_input()).await;
    session.frames(1).await;

    // When the terminal produces fresh output
    host.terminal().write(b"$ ");

    // Then it arrives as a live data frame — no offsets, since it is contiguous with the stream
    assert_eq!(
        session.frames(1).await,
        vec![SessionTerminalOutput {
            data: b"$ ".to_vec(),
            acked_input_offset: 0,
            start_offset: 0,
            end_offset: 0,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
            snapshot: false,
            snapshot_rows: 0,
            snapshot_cols: 0,
            refused_input_offset: 0,
            input_refusal: String::new(),
        }]
    );
}

#[tokio::test]
async fn acknowledges_the_applied_input_offset_on_the_output_half() {
    // Given an open bidi stream on a terminal that has applied no input yet
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let mut session = host.open_bidi(an_input()).await;
    session.frames(1).await;

    // When the PTY applies six bytes of input
    host.terminal().set_acked(6);

    // Then the client is sent an ACK frame: empty data carrying the applied offset, so its
    // un-acknowledged-input overlay can collapse
    assert_eq!(
        session.frames(1).await,
        vec![SessionTerminalOutput {
            data: Vec::new(),
            acked_input_offset: 6,
            start_offset: 0,
            end_offset: 0,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
            snapshot: false,
            snapshot_rows: 0,
            snapshot_cols: 0,
            refused_input_offset: 0,
            input_refusal: String::new(),
        }]
    );
}

#[tokio::test]
async fn resumes_from_the_clients_offset_instead_of_replaying_the_tail() {
    // Given a session whose terminal has produced ten bytes, six of which the client already holds
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");
    let resuming = SessionTerminalInput {
        mode: StreamReplayMode::FromOffset as i32,
        from_offset: 6,
        ..an_input()
    };

    // When it reopens the stream resuming from offset six
    let mut session = host.open_bidi(resuming).await;

    // Then it is sent only the gap it missed, with no duplicate of what it already has
    assert_eq!(
        session.frames(1).await,
        vec![SessionTerminalOutput {
            data: b"6789".to_vec(),
            acked_input_offset: 0,
            start_offset: 6,
            end_offset: 10,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: "main".to_string(),
            snapshot: false,
            snapshot_rows: 0,
            snapshot_cols: 0,
            refused_input_offset: 0,
            input_refusal: String::new(),
        }]
    );
}

#[tokio::test]
async fn ends_the_output_half_when_the_child_exits() {
    // Given an open bidi stream that has replayed the ring
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let mut session = host.open_bidi(an_input()).await;
    session.frames(1).await;

    // When the child process exits
    host.terminal().end();

    // Then the output half closes rather than hanging open on a dead terminal — the end of the
    // stream itself, not the absence of a frame, which a stream left open would also produce
    assert_eq!(session.next_frame_or_close().await, None);
}

#[tokio::test]
async fn a_screen_without_the_lease_watches_the_terminal_but_cannot_type_into_it() {
    // Given a session whose terminal control is held by another screen
    let host = a_host_with_a_sized_terminal(b"$ top\r\n");
    host.control_held_by(OTHER_SCREEN_ID).await;

    // When this screen opens the stream asking for a snapshot at its own grid, with keystrokes
    // already in the opening message and no control token
    let mut watcher = host
        .open_bidi(SessionTerminalInput {
            initial_rows: 40,
            initial_cols: 120,
            ..typing_into_a_snapshot_open(b"q", 1)
        })
        .await;

    // Then it is shown the screen, at the grid the driving screen set
    let snapshot = watcher.frames(1).await.remove(0);
    assert_eq!(
        (
            snapshot.snapshot,
            snapshot.snapshot_rows,
            snapshot.snapshot_cols
        ),
        (true, ROWS as u32, COLS as u32)
    );
    assert_eq!(host.terminal().resizes.lock().unwrap().clone(), Vec::new());

    // And its keystrokes are refused on the output half — told which chunk and why — and never typed
    let refusal = watcher.frames(1).await.remove(0);
    assert_eq!(
        refusal_of(&refusal),
        (1, "terminal controlled by another screen")
    );
    watcher.send(typing(b"k", 2)).await;
    let refusal = watcher.frames(1).await.remove(0);
    assert_eq!(
        refusal_of(&refusal),
        (2, "terminal controlled by another screen")
    );
    assert_eq!(host.typed_into_pty(), Vec::new());

    // And the stream stays open, carrying what the terminal goes on to print
    host.terminal().write(b"load average: 0.42\r\n");
    let live = watcher.frames(1).await.remove(0);
    assert_eq!(live.data, b"load average: 0.42\r\n".to_vec());
}

#[tokio::test]
async fn a_screen_whose_lease_was_taken_reopens_after_a_blip_and_keeps_watching() {
    // Given a screen that drove the terminal until another screen took control
    let host = a_host_with_a_sized_terminal(b"$ ");
    let stale_token = host.control_held_by(SCREEN_ID).await;
    host.control_held_by(OTHER_SCREEN_ID).await;

    // When its transport blips and it re-opens presenting the token it held
    let mut reopened = host
        .open_bidi(SessionTerminalInput {
            control_token: stale_token.clone(),
            ..a_snapshot_open()
        })
        .await;

    // Then the re-open is served the current screen rather than refused
    let snapshot = reopened.frames(1).await.remove(0);
    assert!(snapshot.snapshot, "the re-open is shown the screen");

    // And what it types under the stale token is refused, not typed
    reopened
        .send(SessionTerminalInput {
            control_token: stale_token,
            ..typing(b"exit\r", 5)
        })
        .await;
    let refusal = reopened.frames(1).await.remove(0);
    assert_eq!(
        refusal_of(&refusal),
        (5, "terminal controlled by another screen")
    );
    assert_eq!(host.typed_into_pty(), Vec::new());

    // And output keeps flowing to it
    host.terminal().write(b"hello\r\n");
    assert_eq!(
        reopened.frames(1).await.remove(0).data,
        b"hello\r\n".to_vec()
    );
}

#[tokio::test]
async fn refuses_keystrokes_under_a_stale_token_and_resumes_once_control_is_reclaimed() {
    // Given an open bidi stream held by this screen
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let control_token = host.control_held_by(SCREEN_ID).await;
    let mut session = host
        .open_bidi(SessionTerminalInput {
            control_token: control_token.clone(),
            ..an_input()
        })
        .await;
    session.frames(1).await;

    // When another screen steals control and this one keeps typing under its stale token
    host.control_held_by(OTHER_SCREEN_ID).await;
    session.send(typing(b"rm -rf /\r", 9)).await;

    // Then the chunk is refused on the output half — told which chunk and why — and never typed
    let refusal = session.frames(1).await.remove(0);
    assert_eq!(
        (
            refusal.data.is_empty(),
            refusal.refused_input_offset,
            refusal.input_refusal.as_str()
        ),
        (true, 9, "terminal controlled by another screen")
    );
    assert_eq!(host.typed_into_pty(), Vec::new());

    // And when this screen re-claims control, the same stream carries its next chunk to the PTY and
    // the terminal acks it — no reconnect
    let reclaimed = host.control_held_by(SCREEN_ID).await;
    session
        .send(SessionTerminalInput {
            control_token: reclaimed,
            ..typing(b"ls\r", 12)
        })
        .await;
    host.terminal().write(b"$ ");
    session.frames(1).await;
    assert_eq!(
        host.typed_into_pty(),
        vec![(Bytes::from_static(b"ls\r"), 12)]
    );
    host.terminal().set_acked(12);
    let ack = session.frames(1).await.remove(0);
    assert_eq!(ack.acked_input_offset, 12);
}
