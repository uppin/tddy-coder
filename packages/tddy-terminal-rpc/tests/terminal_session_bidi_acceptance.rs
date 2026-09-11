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
use support::{an_input, typing, TerminalServiceHost, SCREEN_ID, SESSION_ID};
use tddy_terminal_rpc::proto::terminal_session::{SessionTerminalOutput, StreamReplayMode};

/// A second browser tab of the same user, which can steal terminal control from the first.
const OTHER_SCREEN_ID: &str = "screen-ada-phone";

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
        }]
    );
}

#[tokio::test]
async fn resumes_from_the_clients_offset_instead_of_replaying_the_tail() {
    // Given a session whose terminal has produced ten bytes, six of which the client already holds
    let host = TerminalServiceHost::serving_main_terminal(b"0123456789");
    let resuming = tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
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
async fn refuses_to_open_for_a_screen_that_does_not_hold_the_control_lease() {
    // Given a session whose terminal control is held by another screen
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    host.control_held_by(OTHER_SCREEN_ID).await;

    // When this screen opens the stream presenting no control token
    let status = host
        .refusal_on_bidi(
            tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
                control_token: String::new(),
                ..an_input()
            },
        )
        .await;

    // Then it is told the terminal is driven by someone else — not that the terminal is missing
    assert_eq!(status.code(), tddy_rpc::Code::FailedPrecondition);
}

#[tokio::test]
async fn stops_carrying_keystrokes_once_the_control_lease_is_lost_mid_stream() {
    // Given an open bidi stream held by this screen
    let host = TerminalServiceHost::serving_main_terminal(b"ready");
    let control_token = host.control_held_by(SCREEN_ID).await;
    let mut session = host
        .open_bidi(
            tddy_terminal_rpc::proto::terminal_session::SessionTerminalInput {
                control_token: control_token.clone(),
                ..an_input()
            },
        )
        .await;
    session.frames(1).await;

    // When another screen steals control and this one keeps typing under its stale token
    host.control_held_by(OTHER_SCREEN_ID).await;
    session.send(typing(b"rm -rf /\r", 9)).await;

    // Then nothing reaches the PTY. The live output frame below is the synchronisation point: it
    // travels the same runtime as the input forwarder, so by the time it arrives the stale chunk
    // has been seen and rejected.
    host.terminal().write(b"$ ");
    session.frames(1).await;
    assert_eq!(host.typed_into_pty(), Vec::new());
}
