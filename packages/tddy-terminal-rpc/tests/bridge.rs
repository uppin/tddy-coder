//! Acceptance tests for the unified streaming bridge in `tddy_terminal_rpc::bridge`.
//!
//! The bridge is exercised against an in-memory [`StubTerminal`] / [`StubStore`] so the tests
//! assert the wire behavior (frame ordering, offsets, ACK interleave, resize/drain, live bridge,
//! history chunking) without a real PTY.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::TerminalCapture;
use tddy_terminal_rpc::proto::terminal_session::{
    GetTerminalHistoryRequest, SessionTerminalInput, SessionTerminalOutput, StreamReplayMode,
    StreamTerminalOutputRequest,
};
use tddy_terminal_rpc::session::{TerminalSession, TerminalSessionStore};
use tddy_terminal_rpc::{
    serve_get_terminal_history_with, serve_send_terminal_input,
    serve_stream_session_terminal_io_with, serve_stream_terminal_output_with,
};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::timeout;

const RECV_TIMEOUT: Duration = Duration::from_secs(2);

/// A stub terminal backed by a real `TerminalCapture` ring plus broadcast/watch channels the
/// bridge subscribes to. Records resizes, inputs, and redraws so tests can assert on them.
struct StubTerminal {
    capture: std::sync::Arc<Mutex<TerminalCapture>>,
    stdout_tx: broadcast::Sender<Bytes>,
    pty_done_tx: watch::Sender<bool>,
    acked_tx: watch::Sender<u64>,
    resizes: Mutex<Vec<(u16, u16)>>,
    inputs: Mutex<Vec<(Bytes, u64)>>,
    redraws: AtomicUsize,
}

impl StubTerminal {
    fn new() -> Self {
        let (stdout_tx, _) = broadcast::channel(64);
        let (pty_done_tx, _) = watch::channel(false);
        let (acked_tx, _) = watch::channel(0u64);
        StubTerminal {
            capture: std::sync::Arc::new(Mutex::new(TerminalCapture::new())),
            stdout_tx,
            pty_done_tx,
            acked_tx,
            resizes: Mutex::new(Vec::new()),
            inputs: Mutex::new(Vec::new()),
            redraws: AtomicUsize::new(0),
        }
    }

    fn write(&self, bytes: &[u8]) {
        self.capture.lock().unwrap().append(bytes);
        let _ = self.stdout_tx.send(Bytes::copy_from_slice(bytes));
    }

    fn set_acked(&self, offset: u64) {
        self.acked_tx.send_replace(offset);
    }

    fn end(&self) {
        self.pty_done_tx.send_replace(true);
    }
}

#[async_trait]
impl TerminalSession for StubTerminal {
    fn capture(&self) -> std::sync::Arc<Mutex<TerminalCapture>> {
        std::sync::Arc::clone(&self.capture)
    }
    fn subscribe_stdout(&self) -> broadcast::Receiver<Bytes> {
        self.stdout_tx.subscribe()
    }
    fn subscribe_pty_done(&self) -> watch::Receiver<bool> {
        self.pty_done_tx.subscribe()
    }
    fn subscribe_acked_offset(&self) -> watch::Receiver<u64> {
        self.acked_tx.subscribe()
    }
    async fn resize(&self, rows: u16, cols: u16) {
        self.resizes.lock().unwrap().push((rows, cols));
    }
    fn send_input(&self, data: Bytes, input_offset: u64) {
        self.inputs.lock().unwrap().push((data, input_offset));
    }
    fn trigger_redraw(&self) {
        self.redraws.fetch_add(1, Ordering::SeqCst);
    }
}

/// A stub store mapping `(session_id, terminal_id)` to a live terminal. Tests register the
/// terminal they want to expose and keep the `Arc` handle to drive it.
struct StubStore {
    terminal: Option<std::sync::Arc<StubTerminal>>,
}

impl StubStore {
    /// Wrap a terminal in the store, returning the store and a shared handle the test drives.
    fn with(terminal: StubTerminal) -> (Self, std::sync::Arc<StubTerminal>) {
        let arc = std::sync::Arc::new(terminal);
        (
            StubStore {
                terminal: Some(arc.clone()),
            },
            arc,
        )
    }

    /// An empty store that exposes no terminal.
    fn empty() -> Self {
        StubStore { terminal: None }
    }
}

#[async_trait]
impl TerminalSessionStore for StubStore {
    async fn get_terminal(
        &self,
        _session_id: &str,
        _terminal_id: &str,
    ) -> Option<std::sync::Arc<dyn TerminalSession>> {
        self.terminal
            .clone()
            .map(|t| t as std::sync::Arc<dyn TerminalSession>)
    }
}

/// Collect every frame the bridge emits until the stream ends (child exit) or the timeout fires.
async fn drain(
    rx: mpsc::Receiver<Result<SessionTerminalOutput, tddy_rpc::Status>>,
) -> Vec<SessionTerminalOutput> {
    let mut rx = rx;
    let mut out = Vec::new();
    while let Ok(Some(frame)) = timeout(RECV_TIMEOUT, rx.recv()).await {
        out.push(frame.unwrap());
    }
    out
}

fn req(session_id: &str, terminal_id: &str, cols: u32, rows: u32) -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: String::new(),
        session_id: session_id.into(),
        terminal_id: terminal_id.into(),
        initial_cols: cols,
        initial_rows: rows,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
    }
}

/// Build a `StreamTerminalOutputRequest` in `FROM_OFFSET` mode resuming from `from_offset`.
fn req_from_offset(
    session_id: &str,
    terminal_id: &str,
    from_offset: u64,
) -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: String::new(),
        session_id: session_id.into(),
        terminal_id: terminal_id.into(),
        initial_cols: 0,
        initial_rows: 0,
        mode: StreamReplayMode::FromOffset as i32,
        from_offset,
    }
}

// ---------------------------------------------------------------------------
// StreamTerminalOutput: last-frame-first + offsets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_stream_terminal_output_emits_the_last_screen_chunk_with_offsets_as_the_first_frame()
{
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);

    // When the client attaches and requests the last 4 bytes as the initial frame
    let rx = serve_stream_terminal_output_with(&store, req("s1", "", 0, 0), 4)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the first frame is the current last frame, tagged with its absolute offsets
    assert_eq!(frames[0].data, b"6789");
    assert_eq!(frames[0].start_offset, 6);
    assert_eq!(frames[0].end_offset, 10);
    assert!(!frames[0].at_oldest);
}

#[tokio::test]
async fn serve_stream_terminal_output_marks_at_oldest_when_the_initial_frame_reaches_the_ring_start(
) {
    // Given a terminal that has produced only 3 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"abc");
    let (store, handle) = StubStore::with(terminal);

    // When the client requests more than the ring holds as the initial frame
    let rx = serve_stream_terminal_output_with(&store, req("s1", "", 0, 0), 64)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the whole buffer is returned and at_oldest signals no older history exists
    assert_eq!(frames[0].data, b"abc");
    assert_eq!(frames[0].start_offset, 0);
    assert_eq!(frames[0].end_offset, 3);
    assert!(frames[0].at_oldest);
}

// ---------------------------------------------------------------------------
// StreamTerminalOutput: ACK interleave
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_stream_terminal_output_emits_the_current_acked_offset_up_front() {
    // Given a terminal that has already applied 7 bytes of input
    let terminal = StubTerminal::new();
    terminal.set_acked(7);
    let (store, handle) = StubStore::with(terminal);

    // When the client attaches
    let rx = serve_stream_terminal_output_with(&store, req("s1", "", 0, 0), 4)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then an ACK frame carrying offset 7 is emitted
    let ack = frames
        .iter()
        .find(|f| f.acked_input_offset == 7 && f.data.is_empty())
        .expect("an ACK frame for offset 7");
    assert_eq!(ack.acked_input_offset, 7);
}

#[tokio::test]
async fn serve_stream_terminal_output_bridges_live_output_until_the_child_exits() {
    // Given a terminal a client is attached to
    let terminal = StubTerminal::new();
    let (store, handle) = StubStore::with(terminal);
    let mut rx = serve_stream_terminal_output_with(&store, req("s1", "", 0, 0), 4)
        .await
        .expect("stream opened");

    // When live output arrives
    handle.write(b"live");

    // Then the live bytes are forwarded as a data frame (consumed before the child exits, so the
    // pty_done branch of the bridge select cannot win the race and drop them)
    let mut saw_live = false;
    for _ in 0..16 {
        let frame = timeout(RECV_TIMEOUT, rx.recv())
            .await
            .expect("timed out waiting for a frame")
            .expect("stream closed unexpectedly")
            .expect("status");
        if frame.data == b"live" {
            saw_live = true;
            break;
        }
    }
    assert!(saw_live, "live bytes were forwarded as a data frame");

    // And when the child exits, the stream ends
    handle.end();
    let next = timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("timed out waiting for stream end");
    assert!(next.is_none(), "stream must end when the child exits");
}

// ---------------------------------------------------------------------------
// StreamTerminalOutput: resize + drain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_stream_terminal_output_resizes_and_drains_stale_broadcast_when_dims_are_provided() {
    // Given a terminal with stale pre-resize bytes already broadcast
    let terminal = StubTerminal::new();
    terminal.write(b"stale");
    let (store, handle) = StubStore::with(terminal);

    // When the client attaches with explicit dimensions
    let rx = serve_stream_terminal_output_with(&store, req("s1", "", 120, 40), 4)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the PTY was resized to the client's dimensions and the stale broadcast was drained
    assert_eq!(handle.resizes.lock().unwrap().clone(), vec![(40, 120)]);
    assert!(
        !frames.iter().any(|f| f.data == b"stale"),
        "stale pre-resize broadcast must be drained, not forwarded"
    );
}

// ---------------------------------------------------------------------------
// StreamTerminalOutput: not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_stream_terminal_output_returns_not_found_for_an_unknown_terminal() {
    // Given a store that exposes no terminal
    let store = StubStore::empty();

    // When the client attaches
    let result = serve_stream_terminal_output_with(&store, req("s1", "", 0, 0), 4).await;

    // Then the stream opens with a not-found status
    let err = result.expect_err("expected a not-found status");
    assert_eq!(err.code(), tddy_rpc::Code::NotFound);
}

// ---------------------------------------------------------------------------
// GetTerminalHistory: progressive forward fill of older history
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_get_terminal_history_returns_the_forward_chunk_starting_at_from_offset() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, _) = StubStore::with(terminal);

    // When the client asks for the first 4 bytes forward from offset 0, bounded by the anchor at 10
    let mut rx = serve_get_terminal_history_with(
        &store,
        GetTerminalHistoryRequest {
            session_token: String::new(),
            session_id: "s1".into(),
            terminal_id: String::new(),
            from_offset: 0,
            until_offset: 10,
            max_bytes: 4,
        },
        64,
    )
    .await
    .expect("stream opened");
    let chunk = timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("timed out waiting for chunk")
        .expect("stream closed")
        .expect("status");

    // Then the chunk is bytes 0..4, reaches the oldest retained byte, and does not terminate
    assert_eq!(chunk.data, b"0123");
    assert_eq!(chunk.start_offset, 0);
    assert_eq!(chunk.end_offset, 4);
    assert!(chunk.at_oldest);
    assert!(!chunk.at_end);
}

#[tokio::test]
async fn serve_get_terminal_history_terminates_with_an_at_end_chunk_at_the_anchor() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, _) = StubStore::with(terminal);

    // When the client asks for a forward chunk from offset 8, bounded by the anchor at 10
    let mut rx = serve_get_terminal_history_with(
        &store,
        GetTerminalHistoryRequest {
            session_token: String::new(),
            session_id: "s1".into(),
            terminal_id: String::new(),
            from_offset: 8,
            until_offset: 10,
            max_bytes: 64,
        },
        64,
    )
    .await
    .expect("stream opened");
    let chunk = timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("timed out waiting for chunk")
        .expect("stream closed")
        .expect("status");

    // Then the chunk is truncated to the anchor (bytes 8..10) and at_end terminates the fill
    assert_eq!(chunk.data, b"89");
    assert_eq!(chunk.start_offset, 8);
    assert_eq!(chunk.end_offset, 10);
    assert!(!chunk.at_oldest);
    assert!(chunk.at_end);
}

// ---------------------------------------------------------------------------
// SendTerminalInput
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_send_terminal_input_forwards_the_bytes_and_the_cumulative_offset() {
    // Given a terminal exposed by the store
    let terminal = StubTerminal::new();
    let (store, handle) = StubStore::with(terminal);

    // When the client sends 3 bytes at cumulative input offset 3
    let _ = serve_send_terminal_input(
        &store,
        SessionTerminalInput {
            session_token: String::new(),
            session_id: "s1".into(),
            data: b"hi!".to_vec(),
            terminal_id: String::new(),
            control_token: String::new(),
            input_offset: 3,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            initial_cols: 0,
            initial_rows: 0,
        },
    )
    .await
    .expect("input accepted");

    // Then the bytes and the offset were forwarded to the PTY
    let inputs = handle.inputs.lock().unwrap().clone();
    assert_eq!(inputs, vec![(Bytes::from_static(b"hi!"), 3)]);
}

// ---------------------------------------------------------------------------
// StreamTerminalOutput: FROM_OFFSET catch-up (reconnect resume)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn serve_stream_terminal_output_from_offset_replays_only_the_gap_from_offset_to_tip() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);

    // When the client reconnects with FROM_OFFSET at 3 (it already holds bytes 0..3)
    let rx = serve_stream_terminal_output_with(&store, req_from_offset("s1", "", 3), 4)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the catch-up is chunked from offset 3 to the tip (10), no tail chunk, each frame tagged
    // with its absolute offsets — the client advances its currentOffset to 10 with no duplicates.
    assert_eq!(frames.len(), 2, "two catch-up chunks: 3..7 and 7..10");
    assert_eq!(frames[0].data, b"3456");
    assert_eq!(frames[0].start_offset, 3);
    assert_eq!(frames[0].end_offset, 7);
    assert!(!frames[0].at_oldest);
    assert_eq!(frames[1].data, b"789");
    assert_eq!(frames[1].start_offset, 7);
    assert_eq!(frames[1].end_offset, 10);
    assert!(!frames[1].at_oldest);
}

#[tokio::test]
async fn serve_stream_terminal_output_from_offset_at_the_tip_emits_no_catch_up() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);

    // When the client reconnects with FROM_OFFSET already at the tip (10)
    let rx = serve_stream_terminal_output_with(&store, req_from_offset("s1", "", 10), 4)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then no catch-up frame is emitted (the gap is empty); the stream goes straight to live and
    // ends on pty_done.
    assert!(
        frames.is_empty(),
        "no catch-up when from_offset == end_offset"
    );
}

#[tokio::test]
async fn serve_stream_terminal_output_from_offset_zero_replays_the_full_retained_buffer() {
    // Given a terminal that has produced 10 bytes (nothing evicted; start_offset == 0)
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);

    // When the client reconnects with FROM_OFFSET 0 and a large frame budget
    let rx = serve_stream_terminal_output_with(&store, req_from_offset("s1", "", 0), 64)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then a single catch-up frame returns the whole retained buffer, reaching the oldest byte.
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].data, b"0123456789");
    assert_eq!(frames[0].start_offset, 0);
    assert_eq!(frames[0].end_offset, 10);
    assert!(frames[0].at_oldest);
}

#[tokio::test]
async fn serve_stream_terminal_output_from_offset_clamps_below_start_offset_to_the_ring_start() {
    // Given a terminal that has evicted its earliest 10 bytes (start_offset == 10)
    let terminal = StubTerminal::new();
    terminal.write(&vec![b'x'; tddy_task::TerminalCapture::CAPTURE_LIMIT_BYTES]);
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);

    // When the client reconnects with FROM_OFFSET 0 (older than the retained start_offset of 10)
    let rx = serve_stream_terminal_output_with(&store, req_from_offset("s1", "", 0), 64 * 1024)
        .await
        .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the catch-up is clamped up to start_offset (10): the first frame starts at 10 and reaches
    // the oldest retained byte (at_oldest) — the evicted bytes 0..10 are not replayed.
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].start_offset, 10);
    assert_eq!(
        frames[0].end_offset,
        tddy_task::TerminalCapture::CAPTURE_LIMIT_BYTES as u64 + 10
    );
    assert!(frames[0].at_oldest);
}

#[tokio::test]
async fn serve_stream_terminal_output_from_offset_skips_resize_and_drain_unlike_tail() {
    // Given a terminal with stale pre-resize bytes already broadcast
    let terminal = StubTerminal::new();
    terminal.write(b"stale");
    let (store, handle) = StubStore::with(terminal);

    // When the client reconnects with FROM_OFFSET 0 and explicit dimensions
    let rx = serve_stream_terminal_output_with(
        &store,
        StreamTerminalOutputRequest {
            session_token: String::new(),
            session_id: "s1".into(),
            terminal_id: String::new(),
            initial_cols: 120,
            initial_rows: 40,
            mode: StreamReplayMode::FromOffset as i32,
            from_offset: 0,
        },
        64,
    )
    .await
    .expect("stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then FROM_OFFSET does NOT resize the PTY (the terminal already holds the right dimensions) and
    // does NOT drain the broadcast (no stale pre-resize bytes were forwarded anyway on reconnect).
    assert!(
        handle.resizes.lock().unwrap().is_empty(),
        "FROM_OFFSET must not resize"
    );
    assert_eq!(frames[0].data, b"stale");
    assert!(frames[0].at_oldest);
}

// ---------------------------------------------------------------------------
// StreamSessionTerminalIO (bidi): replay-once / resume-by-offset + input forward
// ---------------------------------------------------------------------------

/// An input stream for the bidi helper: a fixed sequence of `SessionTerminalInput` chunks drained by
/// the bridge's spawned forwarder. `tokio_stream::iter` yields the items then ends, so the
/// forwarder task exits after the last chunk.
fn input_stream(
    msgs: Vec<SessionTerminalInput>,
) -> impl tokio_stream::Stream<Item = Result<SessionTerminalInput, tddy_rpc::Status>> + Send + Unpin
{
    tokio_stream::iter(msgs.into_iter().map(Ok))
}

/// A `verify_control` closure that always approves (tests do not exercise control-token theft).
fn always_allow_control() -> impl Fn(&str, &str) -> std::future::Ready<bool> + Send + Sync {
    move |_sid, _tok| std::future::ready(true)
}

#[tokio::test]
async fn serve_stream_session_terminal_io_tail_emits_the_last_frame_then_forwards_input() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);
    let session = store.terminal.clone().expect("terminal registered");
    let session: std::sync::Arc<dyn TerminalSession> = session;

    // When a bidi client opens with TAIL and sends an open-frame chunk plus two follow-up chunks
    let first = SessionTerminalInput {
        session_token: String::new(),
        session_id: "s1".into(),
        data: b"first".to_vec(),
        terminal_id: String::new(),
        control_token: String::new(),
        input_offset: 0,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
        initial_cols: 0,
        initial_rows: 0,
    };
    let followups = vec![
        SessionTerminalInput {
            session_token: String::new(),
            session_id: "s1".into(),
            data: b"second".to_vec(),
            terminal_id: String::new(),
            control_token: String::new(),
            input_offset: 5,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            initial_cols: 0,
            initial_rows: 0,
        },
        SessionTerminalInput {
            session_token: String::new(),
            session_id: "s1".into(),
            data: b"third".to_vec(),
            terminal_id: String::new(),
            control_token: String::new(),
            input_offset: 11,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            initial_cols: 0,
            initial_rows: 0,
        },
    ];
    let mut rx = serve_stream_session_terminal_io_with(
        session,
        "s1".into(),
        first,
        input_stream(followups),
        always_allow_control(),
        4,
    )
    .await
    .expect("bidi stream opened");
    handle.end();

    // Then the output side emits the TAIL last frame (6789, tagged 6..10) before going live.
    let mut saw_last_frame = false;
    for _ in 0..16 {
        let frame = match timeout(RECV_TIMEOUT, rx.recv()).await {
            Ok(Some(Ok(f))) => f,
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => break,
        };
        if frame.data == b"6789" && frame.start_offset == 6 && frame.end_offset == 10 {
            saw_last_frame = true;
            break;
        }
    }
    assert!(
        saw_last_frame,
        "bidi TAIL emits the last frame with offsets"
    );

    // And the input side forwarded the open-frame data plus both follow-up chunks to the PTY, in order
    // with their cumulative offsets. The forwarder is a spawned task that drains the input stream
    // asynchronously, so poll the recorded inputs until all three arrive (or the timeout fires).
    let inputs = {
        let mut inputs = handle.inputs.lock().unwrap().clone();
        let deadline = std::time::Instant::now() + RECV_TIMEOUT;
        while inputs.len() < 3 && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
            inputs = handle.inputs.lock().unwrap().clone();
        }
        inputs
    };
    assert_eq!(
        inputs,
        vec![
            (Bytes::from_static(b"first"), 0),
            (Bytes::from_static(b"second"), 5),
            (Bytes::from_static(b"third"), 11),
        ]
    );
}

#[tokio::test]
async fn serve_stream_session_terminal_io_from_offset_emits_only_the_catch_up_gap() {
    // Given a terminal that has produced 10 bytes
    let terminal = StubTerminal::new();
    terminal.write(b"0123456789");
    let (store, handle) = StubStore::with(terminal);
    let session = store.terminal.clone().expect("terminal registered");
    let session: std::sync::Arc<dyn TerminalSession> = session;

    // When a bidi client reconnects with FROM_OFFSET at 3 (it already holds bytes 0..3)
    let first = SessionTerminalInput {
        session_token: String::new(),
        session_id: "s1".into(),
        data: Vec::new(),
        terminal_id: String::new(),
        control_token: String::new(),
        input_offset: 0,
        mode: StreamReplayMode::FromOffset as i32,
        from_offset: 3,
        initial_cols: 0,
        initial_rows: 0,
    };
    let rx = serve_stream_session_terminal_io_with(
        session,
        "s1".into(),
        first,
        input_stream(Vec::new()),
        always_allow_control(),
        4,
    )
    .await
    .expect("bidi stream opened");
    handle.end();
    let frames = drain(rx).await;

    // Then the bidi output carries only the catch-up gap (3..7, 7..10), no tail chunk — matching the
    // server-streaming FROM_OFFSET path so a bidi reconnect also resumes without duplicates.
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].data, b"3456");
    assert_eq!(frames[0].start_offset, 3);
    assert_eq!(frames[0].end_offset, 7);
    assert_eq!(frames[1].data, b"789");
    assert_eq!(frames[1].start_offset, 7);
    assert_eq!(frames[1].end_offset, 10);
}
