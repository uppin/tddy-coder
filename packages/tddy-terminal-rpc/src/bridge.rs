//! Transport-agnostic streaming bridge for [`TerminalSessionService`].
//!
//! These functions consolidate the resize / capture-replay / broadcast-subscribe / ACK-framing
//! logic that was previously duplicated between `tddy-daemon`'s gRPC `ConnectionService` and
//! `tddy-coder`'s LiveKit `SessionConnectionServiceRpc`. Both backends now delegate here behind a
//! [`TerminalSessionStore`] impl.
//!
//! Replay model: [`serve_stream_terminal_output`] sends the mode prologue and the current last
//! frame (a tail chunk of the capture ring, tagged with its absolute byte offsets) as the first
//! frames, then bridges live broadcast output until the child exits. Older history is fetched on
//! demand via [`serve_get_terminal_history`] as the user scrolls up.

use std::future::Future;
use std::sync::Arc;

use bytes::Bytes;
use tddy_rpc::Status;
use tddy_task::CaptureChunk;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use crate::proto::terminal_session::{
    GetTerminalHistoryRequest, SessionTerminalInput, SessionTerminalOutput, StreamReplayMode,
    StreamTerminalOutputRequest, TerminalHistoryChunk,
};
use crate::session::{TerminalSession, TerminalSessionStore};

/// Default size of the "current last frame" sent on reconnect — roughly a few screens of output,
/// large enough to show the user what is on screen now without dumping the whole ring.
pub const DEFAULT_INITIAL_FRAME_BYTES: usize = 8 * 1024;

/// Capacity of the mpsc channel bridging broadcast output to the RPC stream.
pub const TERMINAL_OUTPUT_CHANNEL_CAPACITY: usize = 64;

/// The reserved terminal id for a session's original (agent) terminal; an empty `terminal_id`
/// in a request resolves to this.
pub const MAIN_TERMINAL_ID: &str = "main";

/// Resolve an empty `terminal_id` to the reserved main terminal id.
pub fn resolved_terminal_id(terminal_id: &str) -> &str {
    if terminal_id.is_empty() {
        MAIN_TERMINAL_ID
    } else {
        terminal_id
    }
}

/// A data frame carrying terminal output bytes. `start_offset` / `end_offset` / `at_oldest` are
/// populated only on the initial replay frame; live tail frames leave them at their zero defaults.
fn data_frame(data: Vec<u8>) -> SessionTerminalOutput {
    SessionTerminalOutput {
        data,
        acked_input_offset: 0,
        start_offset: 0,
        end_offset: 0,
        at_oldest: false,
    }
}

/// The initial replay frame: the current last frame, tagged with its absolute offsets and whether
/// it reaches the ring's oldest retained byte (so the client knows whether older history exists).
fn initial_replay_frame(
    data: Vec<u8>,
    start_offset: u64,
    end_offset: u64,
    at_oldest: bool,
) -> SessionTerminalOutput {
    SessionTerminalOutput {
        data,
        acked_input_offset: 0,
        start_offset,
        end_offset,
        at_oldest,
    }
}

/// An ACK frame: empty `data` carrying the applied input offset.
fn ack_frame(acked_input_offset: u64) -> SessionTerminalOutput {
    SessionTerminalOutput {
        data: Vec::new(),
        acked_input_offset,
        start_offset: 0,
        end_offset: 0,
        at_oldest: false,
    }
}

/// Serve `StreamTerminalOutput`: the mode prologue + current last frame first, then live broadcast
/// output (interleaved with input-offset ACKs) until the child exits.
///
/// Returns the mpsc receiver the caller drains into its transport's server stream, or a [`Status`]
/// when the session or terminal cannot be resolved.
pub async fn serve_stream_terminal_output(
    store: &dyn TerminalSessionStore,
    req: StreamTerminalOutputRequest,
) -> Result<mpsc::Receiver<Result<SessionTerminalOutput, Status>>, Status> {
    serve_stream_terminal_output_with(store, req, DEFAULT_INITIAL_FRAME_BYTES).await
}

/// Same as [`serve_stream_terminal_output`] with an explicit initial-frame byte budget. Used by
/// tests to exercise the chunking with a small ring.
pub async fn serve_stream_terminal_output_with(
    store: &dyn TerminalSessionStore,
    req: StreamTerminalOutputRequest,
    initial_frame_bytes: usize,
) -> Result<mpsc::Receiver<Result<SessionTerminalOutput, Status>>, Status> {
    let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
    let session = store
        .get_terminal(&req.session_id, &terminal_id)
        .await
        .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

    let (tx, rx) = mpsc::channel(TERMINAL_OUTPUT_CHANNEL_CAPACITY);
    open_replay_ack_live(
        session,
        tx,
        replay_mode_from_i32(req.mode),
        req.from_offset,
        req.initial_cols,
        req.initial_rows,
        initial_frame_bytes,
    )
    .await;
    Ok(rx)
}

/// Shared open / replay / catch-up / ack / live sequence used by BOTH the split
/// (`StreamTerminalOutput`) and the bidi (`StreamSessionTerminalIO`) variants.
///
/// Sends the mode prologue first, then:
/// - [`StreamReplayMode::Tail`] (first connect): the current last-frame tail chunk (tagged with its
///   absolute offsets), resizes the PTY to the client's dimensions and drains the pre-resize
///   broadcast so the bridge only forwards the fresh post-resize frame.
/// - [`StreamReplayMode::FromOffset`] (reconnect): chunked catch-up via `replay_from(from_offset,
///   tip, initial_frame_bytes)` looped until `at_end`, so a terminal that already holds state up
///   to `from_offset` receives only the bytes it missed — no tail chunk, no resize/drain.
///
/// Then emits the current applied-input offset up front (so a stream opening after some input was
/// already applied learns the ACK position immediately), and spawns the live broadcast bridge that
/// forwards stdout bytes interleaved with ACK frames until the child exits.
async fn open_replay_ack_live(
    session: Arc<dyn TerminalSession>,
    tx: mpsc::Sender<Result<SessionTerminalOutput, Status>>,
    mode: StreamReplayMode,
    from_offset: u64,
    initial_cols: u32,
    initial_rows: u32,
    initial_frame_bytes: usize,
) {
    // Re-issue the mouse-tracking modes the application enabled as the very first frame, before any
    // replay: a client's VT only reports clicks/drags/scrolls once it has seen the DECSET, and the
    // capture ring may have trimmed past the startup bytes that enabled them.
    let prologue = session
        .capture()
        .lock()
        .map(|cap| cap.mode_prologue())
        .unwrap_or_default();
    if !prologue.is_empty() {
        let _ = tx.send(Ok(data_frame(prologue))).await;
    }

    let empty_chunk = || CaptureChunk {
        data: Vec::new(),
        start_offset: 0,
        end_offset: 0,
        at_oldest: true,
        at_end: true,
    };

    match mode {
        StreamReplayMode::Tail => {
            // The current last frame: a tail chunk of the ring, tagged with its absolute offsets so
            // the client can anchor scroll-up history requests at `start_offset`.
            let last_frame = session
                .capture()
                .lock()
                .map(|cap| cap.replay_last(initial_frame_bytes))
                .unwrap_or_else(|_| empty_chunk());
            if !last_frame.data.is_empty() || last_frame.at_oldest {
                let _ = tx
                    .send(Ok(initial_replay_frame(
                        last_frame.data,
                        last_frame.start_offset,
                        last_frame.end_offset,
                        last_frame.at_oldest,
                    )))
                    .await;
            }

            // Resize the PTY to the client's dimensions before bridging live output so the shell
            // redraws at the browser's actual width. Drain any pre-resize broadcast so the bridge
            // only forwards the fresh post-resize frame.
            if initial_cols > 0 && initial_rows > 0 {
                session
                    .resize(
                        req_initial_rows_as_u16(initial_rows),
                        req_initial_cols_as_u16(initial_cols),
                    )
                    .await;
                session.trigger_redraw();
            }
            let mut stdout_rx = session.subscribe_stdout();
            if initial_cols > 0 && initial_rows > 0 {
                use tokio::sync::broadcast::error::TryRecvError;
                loop {
                    match stdout_rx.try_recv() {
                        Ok(_) => {}
                        Err(TryRecvError::Lagged(_)) => continue,
                        Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                    }
                }
            }
            spawn_live_bridge(session, tx, stdout_rx);
        }
        StreamReplayMode::FromOffset => {
            // Chunked catch-up: forward-fill from `from_offset` (clamped up to the ring's
            // `start_offset` when older bytes were evicted) to the capture tip, one bounded frame
            // at a time so an oversized retained history never exceeds the transport's per-message
            // limit. Each frame is tagged with its absolute offsets so the client advances its
            // `currentOffset` to the tip; `at_oldest` signals older history was evicted.
            let mut cursor = from_offset;
            loop {
                let chunk = session
                    .capture()
                    .lock()
                    .map(|cap| cap.replay_from(cursor, 0, initial_frame_bytes))
                    .unwrap_or_else(|_| empty_chunk());
                if !chunk.data.is_empty() {
                    let _ = tx
                        .send(Ok(initial_replay_frame(
                            chunk.data,
                            chunk.start_offset,
                            chunk.end_offset,
                            chunk.at_oldest,
                        )))
                        .await;
                }
                cursor = chunk.end_offset;
                if chunk.at_end {
                    break;
                }
            }
            // No tail chunk, no resize/drain on reconnect — the terminal already holds the right
            // dimensions and state up to `from_offset`; we only fill the gap then go live.
            let stdout_rx = session.subscribe_stdout();
            spawn_live_bridge(session, tx, stdout_rx);
        }
    }
}

/// Spawn the live broadcast bridge: forward stdout bytes interleaved with input-offset ACK frames
/// until the child exits. The current applied offset is emitted up front (before any live byte) so
/// a stream opening after some input was already applied learns the ACK position immediately.
fn spawn_live_bridge(
    session: Arc<dyn TerminalSession>,
    tx: mpsc::Sender<Result<SessionTerminalOutput, Status>>,
    mut stdout_rx: tokio::sync::broadcast::Receiver<Bytes>,
) {
    let mut acked_rx = session.subscribe_acked_offset();
    let initial_acked = *acked_rx.borrow_and_update();
    let mut pty_done = session.subscribe_pty_done();
    let bridge_tx = tx.clone();
    tokio::spawn(async move {
        if initial_acked > 0 && bridge_tx.send(Ok(ack_frame(initial_acked))).await.is_err() {
            return;
        }
        use tokio::sync::broadcast::error::RecvError;
        let mut ack_open = true;
        loop {
            tokio::select! {
                result = stdout_rx.recv() => match result {
                    Ok(bytes) => {
                        let frame = data_frame(bytes.to_vec());
                        if bridge_tx.send(Ok(frame)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
                },
                changed = acked_rx.changed(), if ack_open => match changed {
                    Ok(()) => {
                        let offset = *acked_rx.borrow_and_update();
                        if bridge_tx.send(Ok(ack_frame(offset))).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => ack_open = false,
                },
                _ = pty_done.changed() => break,
            }
        }
    });
}

fn req_initial_cols_as_u16(c: u32) -> u16 {
    c.clamp(1, u16::MAX as u32) as u16
}

fn req_initial_rows_as_u16(r: u32) -> u16 {
    r.clamp(1, u16::MAX as u32) as u16
}

/// Coerce a raw `mode` field (prost represents enum fields as `i32`) to [`StreamReplayMode`],
/// defaulting unknown values to [`StreamReplayMode::Tail`] so a malformed request still opens a
/// usable stream.
fn replay_mode_from_i32(mode: i32) -> StreamReplayMode {
    match mode {
        x if x == StreamReplayMode::FromOffset as i32 => StreamReplayMode::FromOffset,
        _ => StreamReplayMode::Tail,
    }
}

/// Serve `StreamSessionTerminalIO` (bidi): the open frame (`first`) carries the same replay
/// selection as [`StreamTerminalOutputRequest`] plus identity/control, so a bidi client gets
/// replay-once-at-init / resume-by-offset on the same connection that carries its input.
///
/// Runs the shared [`open_replay_ack_live`] sequence for the OUTPUT side, then forwards the first
/// message's `data` and every subsequent input chunk to the PTY stdin via
/// [`TerminalSession::send_input`] (so the bidi client also receives input-offset ACKs), verifying
/// the control token on each chunk via `verify_control` and ending the forwarder when control is
/// lost (matches the daemon's per-chunk control-token check).
///
/// The caller remains responsible for auth (session-token resolution + OS-user mapping) and the
/// FIRST message's control-token check; this function only verifies subsequent chunks.
pub async fn serve_stream_session_terminal_io_with<S, F, Fut>(
    session: Arc<dyn TerminalSession>,
    session_id: String,
    first: SessionTerminalInput,
    in_stream: S,
    verify_control: F,
    initial_frame_bytes: usize,
) -> Result<mpsc::Receiver<Result<SessionTerminalOutput, Status>>, Status>
where
    S: Stream<Item = Result<SessionTerminalInput, Status>> + Unpin + Send + 'static,
    F: Fn(&str, &str) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = bool> + Send,
{
    let (tx, rx) = mpsc::channel(TERMINAL_OUTPUT_CHANNEL_CAPACITY);

    // Output side: the shared open/replay/catch-up/ack/live sequence.
    open_replay_ack_live(
        Arc::clone(&session),
        tx.clone(),
        replay_mode_from_i32(first.mode),
        first.from_offset,
        first.initial_cols,
        first.initial_rows,
        initial_frame_bytes,
    )
    .await;

    // Forward the first message's data (if any) to stdin.
    if !first.data.is_empty() {
        session.send_input(Bytes::from(first.data), first.input_offset);
    }

    // Spawn a task to forward subsequent input chunks to stdin, verifying the control token on
    // each chunk. Ends when the client stream ends, a stream error occurs, or control is lost.
    let session_for_input = Arc::clone(&session);
    tokio::spawn(async move {
        use tokio_stream::StreamExt;
        let mut in_stream = in_stream;
        while let Some(item) = in_stream.next().await {
            match item {
                Ok(msg) => {
                    if !verify_control(&session_id, &msg.control_token).await {
                        break;
                    }
                    if !msg.data.is_empty() {
                        session_for_input.send_input(Bytes::from(msg.data), msg.input_offset);
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(rx)
}

/// Serve `GetTerminalHistory`: one FORWARD chunk of older output starting at `from_offset`, bounded
/// above by `until_offset` (the anchor; 0 = until the capture tip), then the stream closes. The
/// chunk's `at_end` flag terminates the progressive, append-only forward fill.
pub async fn serve_get_terminal_history(
    store: &dyn TerminalSessionStore,
    req: GetTerminalHistoryRequest,
) -> Result<mpsc::Receiver<Result<TerminalHistoryChunk, Status>>, Status> {
    serve_get_terminal_history_with(store, req, DEFAULT_INITIAL_FRAME_BYTES).await
}

/// Same as [`serve_get_terminal_history`] with an explicit default `max_bytes` when the request
/// leaves it at zero. Used by tests.
pub async fn serve_get_terminal_history_with(
    store: &dyn TerminalSessionStore,
    req: GetTerminalHistoryRequest,
    default_max_bytes: usize,
) -> Result<mpsc::Receiver<Result<TerminalHistoryChunk, Status>>, Status> {
    let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
    let session = store
        .get_terminal(&req.session_id, &terminal_id)
        .await
        .ok_or_else(|| Status::not_found("terminal not found or not running"))?;

    let max_bytes = if req.max_bytes == 0 {
        default_max_bytes
    } else {
        req.max_bytes as usize
    };

    let (tx, rx) = mpsc::channel(1);
    let chunk = session
        .capture()
        .lock()
        .map(|cap| cap.replay_from(req.from_offset, req.until_offset, max_bytes))
        .unwrap_or_else(|_| tddy_task::CaptureChunk {
            data: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            at_oldest: true,
            at_end: true,
        });
    let _ = tx
        .send(Ok(TerminalHistoryChunk {
            data: chunk.data,
            start_offset: chunk.start_offset,
            end_offset: chunk.end_offset,
            at_oldest: chunk.at_oldest,
            at_end: chunk.at_end,
        }))
        .await;
    Ok(rx)
}

/// Serve `SendTerminalInput`: forward the input bytes (with OSC-resize interception) to the PTY and
/// acknowledge the cumulative input offset.
pub async fn serve_send_terminal_input(
    store: &dyn TerminalSessionStore,
    req: crate::proto::terminal_session::SessionTerminalInput,
) -> Result<crate::proto::terminal_session::SendTerminalInputResponse, Status> {
    let terminal_id = resolved_terminal_id(&req.terminal_id).to_string();
    let session = store
        .get_terminal(&req.session_id, &terminal_id)
        .await
        .ok_or_else(|| Status::not_found("terminal not found or not running"))?;
    if !req.data.is_empty() {
        session.send_input(Bytes::from(req.data), req.input_offset);
    }
    Ok(crate::proto::terminal_session::SendTerminalInputResponse {})
}

/// Drain a `serve_stream_terminal_output` receiver into a `tonic`-compatible `ReceiverStream` of
/// `Result<SessionTerminalOutput, Status>`.
pub fn into_tonic_stream(
    rx: mpsc::Receiver<Result<SessionTerminalOutput, Status>>,
) -> ReceiverStream<Result<SessionTerminalOutput, Status>> {
    ReceiverStream::new(rx)
}

/// Drain a `serve_get_terminal_history` receiver into a `tonic`-compatible `ReceiverStream`.
pub fn history_into_tonic_stream(
    rx: mpsc::Receiver<Result<TerminalHistoryChunk, Status>>,
) -> ReceiverStream<Result<TerminalHistoryChunk, Status>> {
    ReceiverStream::new(rx)
}
