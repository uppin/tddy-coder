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

use bytes::Bytes;
use tddy_rpc::Status;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::proto::terminal_session::{
    GetTerminalHistoryRequest, SessionTerminalOutput, StreamTerminalOutputRequest,
    TerminalHistoryChunk,
};
use crate::session::TerminalSessionStore;

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

    // The current last frame: a tail chunk of the ring, tagged with its absolute offsets so the
    // client can anchor scroll-up history requests at `start_offset`.
    let last_frame = session
        .capture()
        .lock()
        .map(|cap| cap.replay_last(initial_frame_bytes))
        .unwrap_or_else(|_| tddy_task::CaptureChunk {
            data: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            at_oldest: true,
            at_end: true,
        });
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

    // Resize the PTY to the client's dimensions before bridging live output so the shell redraws at
    // the browser's actual width. Drain any pre-resize broadcast so the bridge only forwards the
    // fresh post-resize frame.
    if req.initial_cols > 0 && req.initial_rows > 0 {
        session.resize(req.initial_rows as u16, req.initial_cols as u16).await;
        session.trigger_redraw();
    }

    let mut stdout_rx = session.subscribe_stdout();
    if req.initial_cols > 0 && req.initial_rows > 0 {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match stdout_rx.try_recv() {
                Ok(_) => {}
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
            }
        }
    }

    // Emit the current applied input offset up front so a stream that opens after some input was
    // already applied (e.g. a reconnect) learns the acknowledged position immediately.
    let mut acked_rx = session.subscribe_acked_offset();
    let initial_acked = *acked_rx.borrow_and_update();
    if initial_acked > 0 {
        let _ = tx.send(Ok(ack_frame(initial_acked))).await;
    }

    // Bridge live broadcast output → the stream, interleaving ACK frames, ending when the child
    // exits.
    let mut pty_done = session.subscribe_pty_done();
    let bridge_tx = tx.clone();
    tokio::spawn(async move {
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
