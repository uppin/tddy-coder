//! Acceptance tests that a **sandboxed** session's terminal, now served through the unified store,
//! produces exactly the frames and offsets the hand-rolled loop inside
//! `connection.ConnectionService::StreamTerminalOutput` produced.
//!
//! That loop was a transcription of `tddy_terminal_rpc::bridge`'s replay/offset arm — its own
//! comment said so — living behind an `if let Some(sandbox) = self.sandbox_manager.get(…)` branch
//! repeated in four RPCs. `#unbundle` node 6 deleted it and made
//! [`DaemonTerminalSessionStore`] composite instead, so a jail's PTY is an ordinary terminal to the
//! bridge. These tests are the evidence for that swap: [`hand_rolled_sandbox_replay`] below is the
//! deleted loop, kept here as the oracle, and every assertion compares the served frames against it.
//!
//! **The oracle is a copy of deleted production code and will drift silently.** Nothing
//! recompiles it against `tddy_terminal_rpc::bridge`, so the first change to the bridge's
//! replay/offset arm — its prologue, its clamping, its frame budget, its end-of-replay rule —
//! leaves these tests comparing new behaviour against a transcription of the old, and whether they
//! pass then says nothing about parity. A maintainer changing that arm must, in the same change,
//! either update [`hand_rolled_sandbox_replay`] to the new intent (keeping it an oracle) or retire
//! this suite in favour of assertions written against the bridge directly. Leaving the oracle
//! untouched is the one option that quietly stops meaning anything.
//!
//! They run against the real [`SandboxSessionState`] and the real
//! `SandboxTerminalSession` behind `DaemonTerminalSessionStore` (crate-private: a jail's PTY is
//! reached through the store, and this suite reaches it the way the handlers do), not a
//! mirror of them, because what is being checked is precisely the mapping between a jail's three
//! channels and everything the bridge asks a terminal for — including the four absences
//! `SandboxTerminalSession` synthesises an answer for.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::terminal_session_adapter::DaemonTerminalSessionStore;
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon_sandbox::sandbox_session::{
    SandboxSessionManager, SandboxSessionState, SandboxSessionStateInit,
};
use tddy_rpc::Request;
use tddy_task::TerminalCapture;
use tddy_terminal_rpc::proto::terminal_session::{
    ClaimTerminalControlRequest, GetTerminalHistoryRequest, SessionTerminalInput,
    SessionTerminalOutput, StreamReplayMode, StreamTerminalOutputRequest, TerminalHistoryChunk,
    TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_terminal_rpc::session::TerminalSessionStore;
use tddy_terminal_rpc::{
    serve_get_terminal_history_with, serve_send_terminal_input, serve_stream_terminal_output_with,
};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;

/// The session id every request below addresses.
const SESSION_ID: &str = "sandboxed-session";
/// The reserved main terminal — the only one a jail has.
const MAIN_TERMINAL_ID: &str = "main";
/// The frame budget the deleted loop used, and the one both sides are given here so the chunk
/// boundaries are comparable at all.
const FRAME_BUDGET_BYTES: usize = 4;
/// How long a frame may take to arrive before the test gives up on the stream.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);
/// Dimensions a browser measures before opening the stream. A jail has no PTY master, so supplying
/// them must not cause a resize — or a drain of the live output that follows one.
const BROWSER_COLS: u32 = 120;
const BROWSER_ROWS: u32 = 40;

// ---------------------------------------------------------------------------
// The oracle: the loop `#unbundle` node 6 deleted
// ---------------------------------------------------------------------------

/// The replay frames the hand-rolled sandbox branch of `StreamTerminalOutput` emitted, verbatim.
///
/// `TAIL` reached it as `from_offset = 0` — a full forward fill of the retained buffer — and
/// `FROM_OFFSET` as the client's own resume point, clamped down to the tip so a client whose
/// counter had drifted ahead was not handed its own bogus offset back.
fn hand_rolled_sandbox_replay(
    capture: &Arc<Mutex<TerminalCapture>>,
    mode: StreamReplayMode,
    requested_from_offset: u64,
) -> Vec<SessionTerminalOutput> {
    let mut frames = Vec::new();
    let from_offset = match mode {
        StreamReplayMode::FromOffset => requested_from_offset,
        StreamReplayMode::Tail => 0,
    };

    let prologue = capture.lock().unwrap().mode_prologue();
    if !prologue.is_empty() {
        frames.push(a_data_frame(prologue));
    }

    let tip = capture.lock().unwrap().end_offset();
    let mut cursor = from_offset.min(tip);
    let mut anchored = false;
    loop {
        let chunk = capture
            .lock()
            .unwrap()
            .replay_from(cursor, 0, FRAME_BUDGET_BYTES);
        let (end_offset, at_end) = (chunk.end_offset, chunk.at_end);
        if !chunk.data.is_empty() || !anchored {
            frames.push(an_anchored_frame(
                chunk.data,
                chunk.start_offset,
                chunk.end_offset,
                chunk.at_oldest,
            ));
            anchored = true;
        }
        cursor = end_offset;
        if at_end {
            break;
        }
    }
    frames
}

/// A live output frame, carrying no offsets because it is contiguous with the stream.
fn a_data_frame(data: Vec<u8>) -> SessionTerminalOutput {
    SessionTerminalOutput {
        data,
        acked_input_offset: 0,
        start_offset: 0,
        end_offset: 0,
        at_oldest: false,
        session_id: SESSION_ID.to_string(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
    }
}

/// A replay frame, tagged with the absolute offsets that let a client anchor its scroll-up.
fn an_anchored_frame(
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
        session_id: SESSION_ID.to_string(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
    }
}

// ---------------------------------------------------------------------------
// A jail-backed session, and the store that resolves it
// ---------------------------------------------------------------------------

/// A registered sandboxed session and the three channels its jail speaks through.
struct SandboxedSession {
    store: DaemonTerminalSessionStore,
    capture: Arc<Mutex<TerminalCapture>>,
    stdout_tx: broadcast::Sender<Bytes>,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
}

impl SandboxedSession {
    /// A jail whose PTY has already produced `output`.
    async fn holding(output: &[u8]) -> Self {
        let (stdout_tx, _) = broadcast::channel(64);
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        let capture = Arc::new(Mutex::new(TerminalCapture::new()));
        capture.lock().unwrap().append(output);

        let sandboxes = Arc::new(SandboxSessionManager::new());
        let state = Arc::new(SandboxSessionState::new(SandboxSessionStateInit {
            pid: 4242,
            worktree_path: PathBuf::from("/unused-by-the-terminal-surface"),
            stdout_tx: stdout_tx.clone(),
            capture: Arc::clone(&capture),
            stdin_tx,
            ready_marker: PathBuf::from("/unused-by-the-terminal-surface"),
            handle: an_exited_sandbox_process(),
            managed_workflow: None,
        }));
        sandboxes.insert(SESSION_ID.to_string(), state).await;

        SandboxedSession {
            store: DaemonTerminalSessionStore::new(Arc::new(CliSessionManager::new()), sandboxes),
            capture,
            stdout_tx,
            stdin_rx,
        }
    }

    /// Fresh output from the jail's PTY: appended to the capture ring and broadcast, as the jail's
    /// reader does.
    fn produces(&self, output: &[u8]) {
        self.capture.lock().unwrap().append(output);
        let _ = self.stdout_tx.send(Bytes::copy_from_slice(output));
    }

    /// The frames the served coordinate emits for this open, up to `count`.
    async fn served_frames(
        &self,
        request: StreamTerminalOutputRequest,
        count: usize,
    ) -> Vec<SessionTerminalOutput> {
        let mut rx = serve_stream_terminal_output_with(&self.store, request, FRAME_BUDGET_BYTES)
            .await
            .expect("the store resolved the jail's terminal");
        let mut frames = Vec::new();
        while frames.len() < count {
            match timeout(FRAME_TIMEOUT, rx.recv()).await {
                Ok(Some(Ok(frame))) => frames.push(frame),
                Ok(Some(Err(status))) => panic!("the stream errored: {status:?}"),
                Ok(None) | Err(_) => break,
            }
        }
        frames
    }

    /// What the deleted loop would have emitted for the same open.
    fn hand_rolled_frames(
        &self,
        mode: StreamReplayMode,
        from_offset: u64,
    ) -> Vec<SessionTerminalOutput> {
        hand_rolled_sandbox_replay(&self.capture, mode, from_offset)
    }
}

/// A process standing in for the jail: `SandboxSessionState` keeps the handle only so delete/resume
/// can kill the tree, and nothing on the terminal surface reads it.
fn an_exited_sandbox_process() -> tddy_sandbox::SandboxHandle {
    let child = std::process::Command::new("true")
        .spawn()
        .expect("a trivial child process");
    tddy_sandbox::SandboxHandle::new(
        child,
        PathBuf::from("/unused-by-the-terminal-surface"),
        PathBuf::from("/unused-by-the-terminal-surface"),
        PathBuf::from("/unused-by-the-terminal-surface"),
    )
}

/// A `StreamTerminalOutputRequest` for the jail's main terminal, resuming as `mode` says.
fn an_output_request(mode: StreamReplayMode, from_offset: u64) -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: String::new(),
        session_id: SESSION_ID.to_string(),
        terminal_id: String::new(),
        initial_cols: BROWSER_COLS,
        initial_rows: BROWSER_ROWS,
        mode: mode as i32,
        from_offset,
    }
}

// ---------------------------------------------------------------------------
// Replay parity with the deleted loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resumes_a_jails_terminal_at_the_clients_offset_with_the_frames_the_deleted_loop_produced()
{
    // Given a jail whose PTY has produced ten bytes, and a client resuming from six
    let session = SandboxedSession::holding(b"0123456789").await;
    let expected = session.hand_rolled_frames(StreamReplayMode::FromOffset, 6);

    // When the client reopens the stream through the unified store
    let served = session
        .served_frames(
            an_output_request(StreamReplayMode::FromOffset, 6),
            expected.len(),
        )
        .await;

    // Then it is handed the same frames at the same offsets the deleted loop handed it
    assert_eq!(served, expected);
    assert_eq!(
        served,
        vec![an_anchored_frame(b"6789".to_vec(), 6, 10, false)]
    );
}

#[tokio::test]
async fn fills_a_jails_terminal_forward_in_bounded_frames_as_the_deleted_loop_did() {
    // Given a jail whose PTY has produced more bytes than one frame may carry
    let session = SandboxedSession::holding(b"0123456789").await;
    let expected = session.hand_rolled_frames(StreamReplayMode::FromOffset, 0);

    // When the client fills forward from the oldest retained byte
    let served = session
        .served_frames(
            an_output_request(StreamReplayMode::FromOffset, 0),
            expected.len(),
        )
        .await;

    // Then the retained buffer arrives as the same ordered, offset-tagged frames — not as one
    // oversized frame the transport would refuse
    assert_eq!(served, expected);
    assert_eq!(
        served,
        vec![
            an_anchored_frame(b"0123".to_vec(), 0, 4, true),
            an_anchored_frame(b"4567".to_vec(), 4, 8, false),
            an_anchored_frame(b"89".to_vec(), 8, 10, false),
        ]
    );
}

#[tokio::test]
async fn clamps_a_drifted_client_offset_back_to_the_jails_tip_as_the_deleted_loop_did() {
    // Given a jail whose PTY has produced ten bytes, and a client whose counter drifted past them
    let session = SandboxedSession::holding(b"0123456789").await;
    let expected = session.hand_rolled_frames(StreamReplayMode::FromOffset, 99);

    // When it reopens the stream at that impossible offset
    let served = session
        .served_frames(
            an_output_request(StreamReplayMode::FromOffset, 99),
            expected.len(),
        )
        .await;

    // Then it is re-anchored to the tip rather than handed its own offset back — otherwise it would
    // keep asking for bytes the capture will never hold
    assert_eq!(served, expected);
    assert_eq!(served, vec![an_anchored_frame(Vec::new(), 10, 10, false)]);
}

#[tokio::test]
async fn re_issues_a_jails_mouse_modes_before_the_anchored_frame_as_the_deleted_loop_did() {
    // Given a jail whose application enabled mouse tracking before producing output
    let session = SandboxedSession::holding(b"\x1b[?1002hscreen").await;
    let expected = session.hand_rolled_frames(StreamReplayMode::FromOffset, 0);

    // When a client opens the stream
    let served = session
        .served_frames(
            an_output_request(StreamReplayMode::FromOffset, 0),
            expected.len(),
        )
        .await;

    // Then the prologue leads, so the client's VT reports clicks even though the DECSET that
    // enabled them may have been evicted from the ring
    assert_eq!(served, expected);
    assert_eq!(
        served.first(),
        Some(&a_data_frame(b"\x1b[?1002h".to_vec())),
        "the mode prologue is the first frame of the stream"
    );
}

// ---------------------------------------------------------------------------
// Three of the four absences: the capabilities a jail's terminal does not have
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sends_a_jails_live_output_without_acknowledging_any_input_offset() {
    // Given an open stream on a jail whose input is not acknowledged anywhere
    let session = SandboxedSession::holding(b"0123456789").await;
    let mut rx = serve_stream_terminal_output_with(
        &session.store,
        an_output_request(StreamReplayMode::FromOffset, 10),
        FRAME_BUDGET_BYTES,
    )
    .await
    .expect("the store resolved the jail's terminal");
    // Drain the single anchoring frame every open emits.
    timeout(FRAME_TIMEOUT, rx.recv())
        .await
        .expect("the anchoring frame arrives");

    // When the jail produces fresh output
    session.produces(b"$ ");

    // Then the next frame is that output — no ACK frame is interleaved, matching the branch this
    // replaced, which had no applied-offset source to ack from
    let frame = timeout(FRAME_TIMEOUT, rx.recv())
        .await
        .expect("the live frame arrives")
        .expect("the stream is open")
        .expect("the frame is not an error");
    assert_eq!(frame, a_data_frame(b"$ ".to_vec()));
}

#[tokio::test]
async fn keeps_a_jails_live_output_even_when_the_client_supplies_terminal_dimensions() {
    // Given a jail with no PTY master to resize, and a client that measured its grid
    let session = SandboxedSession::holding(b"0123456789").await;
    let mut rx = serve_stream_terminal_output_with(
        &session.store,
        // TAIL is the mode a browser opens with, and the one that resizes and drains for a terminal
        // that can be resized.
        an_output_request(StreamReplayMode::Tail, 0),
        FRAME_BUDGET_BYTES,
    )
    .await
    .expect("the store resolved the jail's terminal");
    timeout(FRAME_TIMEOUT, rx.recv())
        .await
        .expect("the tail frame arrives");

    // When the jail produces output right after the open
    session.produces(b"$ ");

    // Then it reaches the client: with no resize there is no stale pre-resize output to discard, so
    // the drain that would have thrown these bytes away never runs
    let frame = timeout(FRAME_TIMEOUT, rx.recv())
        .await
        .expect("the live frame arrives")
        .expect("the stream is open")
        .expect("the frame is not an error");
    assert_eq!(frame, a_data_frame(b"$ ".to_vec()));
}

#[tokio::test]
async fn ends_a_jails_stream_when_its_output_broadcast_closes() {
    // Given an open stream on a jail
    let session = SandboxedSession::holding(b"0123456789").await;
    let mut rx = serve_stream_terminal_output_with(
        &session.store,
        an_output_request(StreamReplayMode::FromOffset, 10),
        FRAME_BUDGET_BYTES,
    )
    .await
    .expect("the store resolved the jail's terminal");
    timeout(FRAME_TIMEOUT, rx.recv())
        .await
        .expect("the anchoring frame arrives");

    // When the jail dies and its output broadcast closes — what ended this stream before the
    // unification, and what the adapter now derives its process-exit watch from
    drop(session);

    // Then the stream closes rather than hanging open on a dead jail
    assert!(
        timeout(FRAME_TIMEOUT, rx.recv())
            .await
            .expect("the stream reaches its end")
            .is_none(),
        "the stream ends rather than yielding another frame"
    );
}

// ---------------------------------------------------------------------------
// Input, and the history the old coordinate refused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn forwards_input_into_the_jail_ignoring_an_offset_nothing_there_counts() {
    // Given a jail with a live terminal
    let mut session = SandboxedSession::holding(b"ready").await;

    // When a client sends keystrokes carrying a cumulative offset
    serve_send_terminal_input(
        &session.store,
        SessionTerminalInput {
            session_token: String::new(),
            session_id: SESSION_ID.to_string(),
            data: b"whoami\r".to_vec(),
            terminal_id: String::new(),
            control_token: String::new(),
            input_offset: 7,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            initial_cols: 0,
            initial_rows: 0,
        },
    )
    .await
    .expect("the store resolved the jail's terminal");

    // Then the bytes reach the jail's stdin. The offset is dropped, as it was before: nothing on
    // the far side counts applied bytes, so acking one would be a promise this host cannot keep.
    assert_eq!(
        session.stdin_rx.try_recv().ok(),
        Some(Bytes::from_static(b"whoami\r"))
    );
}

#[tokio::test]
async fn serves_a_jails_scroll_up_history_where_the_old_coordinate_refused_it() {
    // Given a jail whose PTY has produced ten bytes
    let session = SandboxedSession::holding(b"0123456789").await;

    // When a client scrolls up — a request `connection.ConnectionService` answered `not_found` for
    // any sandboxed session, so a jail's terminal had no history at all
    let mut rx = serve_get_terminal_history_with(
        &session.store,
        GetTerminalHistoryRequest {
            session_token: String::new(),
            session_id: SESSION_ID.to_string(),
            terminal_id: String::new(),
            from_offset: 0,
            until_offset: 0,
            max_bytes: 0,
        },
        FRAME_BUDGET_BYTES,
    )
    .await
    .expect("the store resolved the jail's terminal");

    // Then it is handed the same offset-anchored chunk any other terminal answers with
    assert_eq!(
        rx.recv()
            .await
            .expect("a chunk arrives")
            .expect("the chunk is not an error"),
        TerminalHistoryChunk {
            data: b"0123".to_vec(),
            start_offset: 0,
            end_offset: 4,
            at_oldest: true,
            at_end: false,
        }
    );
}

#[tokio::test]
async fn refuses_a_terminal_of_a_jail_that_only_has_the_main_one() {
    // Given a sandboxed session, which runs exactly one terminal
    let session = SandboxedSession::holding(b"0123456789").await;

    // When a client addresses some other terminal id on it
    let resolved = session.store.get_terminal(SESSION_ID, "bash-1").await;

    // Then nothing resolves, so the caller answers `not_found` — a started shell never reaches a jail
    assert!(resolved.is_none());
}

// ---------------------------------------------------------------------------
// The fourth absence: the control lease a jail's terminal is not behind
// ---------------------------------------------------------------------------
//
// These drive `terminal_session.TerminalSessionService` rather than the bridge helpers above,
// because the lease is checked at the service and nowhere else: the helpers take a resolved
// terminal and an already-verified caller, so nine tests through them cannot see a jail that has
// been put behind a check it cannot answer. Before the unification the sandbox branch of both input
// RPCs forwarded to `stdin_tx` and returned *without* consulting `verify_control` at all.

/// The screen a human clicks "Claim terminal" on. Any browser screen will do — what matters is that
/// it is not the in-jail bridge.
const A_BROWSER_SCREEN: &str = "browser-screen-a";

/// One daemon serving one jailed session at the coordinate both a browser and the in-jail bridge
/// reach it through.
///
/// Built from [`tddy_daemon::test_util::test_service`] and the daemon's own
/// [`ConnectionServiceImpl::sandbox_sessions`] registry, so the store, the lease and the handlers
/// are the ones production wires — the point of these tests is what *this daemon's* coordinate does
/// with a jail's terminal, which a re-assembled set of ports could not answer.
struct ADaemonServingAJailedSession {
    _data_dir: tempfile::TempDir,
    /// The daemon itself, held so the jail stays registered in the registry the coordinate below
    /// resolves terminals out of.
    _daemon: tddy_daemon::connection_service::ConnectionServiceImpl,
    terminals: tddy_terminal_rpc::TerminalSessionServiceImpl,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
}

impl ADaemonServingAJailedSession {
    async fn running_a_jail() -> Self {
        let data_dir = tempfile::tempdir().expect("a data dir");
        let (stdout_tx, _) = broadcast::channel(64);
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();

        let daemon = tddy_daemon::test_util::test_service(data_dir.path().to_path_buf());
        daemon
            .sandbox_sessions()
            .insert(
                SESSION_ID.to_string(),
                Arc::new(SandboxSessionState::new(SandboxSessionStateInit {
                    pid: 4242,
                    worktree_path: PathBuf::from("/unused-by-the-terminal-surface"),
                    stdout_tx,
                    capture: Arc::new(Mutex::new(TerminalCapture::new())),
                    stdin_tx,
                    ready_marker: PathBuf::from("/unused-by-the-terminal-surface"),
                    handle: an_exited_sandbox_process(),
                    managed_workflow: None,
                })),
            )
            .await;

        ADaemonServingAJailedSession {
            _data_dir: data_dir,
            terminals: daemon.terminal_session_service(),
            _daemon: daemon,
            stdin_rx,
        }
    }

    /// A browser screen takes the session's terminal control, as clicking "Claim terminal" does.
    async fn a_browser_screen_claims_control(&self) {
        let granted = self
            .terminals
            .claim_terminal_control(Request::new(ClaimTerminalControlRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                screen_id: A_BROWSER_SCREEN.to_string(),
                steal: false,
            }))
            .await
            .expect("a screen may claim an uncontrolled session")
            .into_inner();
        assert!(
            granted.granted,
            "the lease these tests are about was never taken"
        );
    }

    /// The in-jail bridge's `SendTerminalInput`. It carries no control token, because
    /// `tddy-sandbox-app` has none to send and no RPC to claim one with.
    async fn the_jail_sends(&self, keystrokes: &[u8]) {
        self.terminals
            .send_terminal_input(Request::new(a_jails_input(keystrokes)))
            .await
            .expect("the in-jail bridge's input reaches its own PTY");
    }

    /// The in-jail bridge's open `StreamSessionTerminalIO`, and the handle it types on.
    ///
    /// The returned sender is the client half of that stream: dropping it ends the bidi call, which
    /// is why the caller holds it for the test's duration.
    async fn the_jails_open_terminal_stream(&self) -> mpsc::UnboundedSender<SessionTerminalInput> {
        let (types, chunks) = mpsc::unbounded_channel();
        types
            .send(a_jails_input(b"\x1b]resize;120;40\x07"))
            .expect("the stream's first frame");
        self.terminals
            .stream_session_terminal_io(Request::new(tddy_rpc::Streaming::new(
                tokio_stream::wrappers::UnboundedReceiverStream::new(chunks).map(Ok),
            )))
            .await
            .expect("the in-jail bridge may open its own terminal stream");
        types
    }

    /// The next bytes handed to the jail's stdin.
    async fn input_reaching_the_jails_pty(&mut self) -> Bytes {
        timeout(FRAME_TIMEOUT, self.stdin_rx.recv())
            .await
            .expect("input reaches the jail's PTY")
            .expect("the jail's stdin is still open")
    }
}

/// A frame from the in-jail bridge: authenticated, addressed at the session's one terminal, and
/// carrying **no** `control_token` — the absence these tests are about.
fn a_jails_input(data: &[u8]) -> SessionTerminalInput {
    SessionTerminalInput {
        session_token: TEST_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        data: data.to_vec(),
        terminal_id: String::new(),
        control_token: String::new(),
        input_offset: 0,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
        initial_cols: 0,
        initial_rows: 0,
    }
}

#[tokio::test]
async fn forwards_a_jails_input_to_its_pty_while_a_browser_screen_holds_the_lease() {
    // Given a jail whose session's terminal control a browser screen holds
    let mut daemon = ADaemonServingAJailedSession::running_a_jail().await;
    daemon.a_browser_screen_claims_control().await;

    // When the in-jail bridge sends a keystroke
    daemon.the_jail_sends(b"whoami\r").await;

    // Then it reaches the PTY: the lease arbitrates between browser screens, and the bridge is not
    // one — it is the process that owns this PTY, so a screen's claim must not sever its input
    assert_eq!(
        daemon.input_reaching_the_jails_pty().await,
        Bytes::from_static(b"whoami\r")
    );
}

#[tokio::test]
async fn keeps_forwarding_a_jails_open_stream_when_a_browser_screen_claims_the_lease_mid_stream() {
    // Given a jail whose in-jail bridge already has its bidi stream open and forwarding
    let mut daemon = ADaemonServingAJailedSession::running_a_jail().await;
    let jail_types = daemon.the_jails_open_terminal_stream().await;
    assert_eq!(
        daemon.input_reaching_the_jails_pty().await,
        Bytes::from_static(b"\x1b]resize;120;40\x07"),
        "the stream's first frame reached the PTY"
    );

    // When a browser screen claims control while that stream is open, and the jail types on
    daemon.a_browser_screen_claims_control().await;
    jail_types
        .send(a_jails_input(b"whoami\r"))
        .expect("the stream is still open");

    // Then the keystroke still reaches the PTY. The per-chunk re-check ends a *screen's* input
    // forwarder when it loses the lease; ending the jail's would stop the user's typing with no
    // error frame and no visible cause, while output kept flowing
    assert_eq!(
        daemon.input_reaching_the_jails_pty().await,
        Bytes::from_static(b"whoami\r")
    );
}
