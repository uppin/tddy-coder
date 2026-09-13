//! Acceptance: one session's terminal answers identically through **both wirings** of the
//! `terminal_session.TerminalSessionService` coordinate.
//!
//! `tddy-coder` is the second server of the terminal family. A session reached over LiveKit is
//! answered by the coder participant; the same session reached over HTTP is answered by the daemon
//! that owns it. The repo has already paid for those two disagreeing — changeset
//! `2026-08-02-activities-tail-first-autoscroll` records a session that "would have opened
//! tail-first when reached over HTTP and head-first when reached over LiveKit" — and serving one
//! implementation on both is this node's answer to it.
//!
//! Both wirings are dispatched the way a transport dispatches — encoded request bytes at the
//! registered service name, decoded answers off the wire — because what must agree is the
//! *coordinate*, not a handler someone remembered to call the same way twice:
//!
//! * [`coder_participant_terminal_entry`] is exactly what `session_service_entries` registers on
//!   the coder's participant: the coder's ports, behind its method filter.
//! * [`daemon_constructor_terminal_entry`] is [`build_terminal_session_entry`] called bare — the
//!   same constructor `tddy-daemon`'s `svc_terminal_ports.rs` calls — over the same
//!   [`TerminalManager`] and store, with a lease that really arbitrates between screens in front
//!   of it.
//!
//! # Known gap: this is one server, wired twice — and one of its six ports is the only difference
//!
//! Both sides are this process's code, and most of what they are handed is the *same port over the
//! same session*: [`daemon_constructor_terminal_entry`] passes `CoderTerminalSessionStore::new`,
//! `CoderTerminalRoster::new` and `DEFAULT_INITIAL_FRAME_BYTES`, which are exactly what
//! [`coder_terminal_session_entry`](tddy_coder::session_participant::coder_terminal_session_entry)
//! passes. The two identity ports are different closures, but both answer, so neither side's
//! refusal gate fires and nothing downstream can tell them apart. That leaves two real differences:
//! the control lease ([`ArbitratedControl`] here against `CoderTerminalControl` there) and the
//! method filter, which only the coder's entry has.
//!
//! So the comparisons below divide into three, and it is worth being exact about which is which:
//!
//! * **Genuinely two sided.** [`both_wirings_grant_an_unheld_terminal_control_claim_to_the_same_screen`]
//!   — two `TerminalControl` implementations, so the two answers are computed by different code and
//!   the equality can fail on its own.
//! * **Same code, rescued by a literal.** [`both_wirings_refuse_to_stop_the_sessions_main_terminal`]
//!   reaches one shared handler through both entries; the `Code::InvalidArgument` it ends with is
//!   what pins the refusal. [`keystrokes_through_either_wiring_reach_the_one_pty`] likewise: both
//!   wirings resolve the one PTY through the one store, and what carries the test is the PTY's own
//!   echo of both wirings' bytes.
//! * **`f(x) == f(x)`.** [`both_wirings_open_one_terminal_with_the_same_tail_replay`],
//!   [`both_wirings_resume_one_terminal_from_the_same_offset`] and
//!   [`both_wirings_fill_one_terminals_history_at_the_same_offsets`] ask one store, through one
//!   bridge, twice. **A one-sided change to replay, resume offsets or history chunking cannot fail
//!   their equality assertion** — nothing is one-sided about those ports. The equality is a
//!   determinism check; the literal frames each of them also asserts are what pin the answer, and
//!   are the reason they are kept rather than deleted. None of the three stops at the count it
//!   expects, either: the two opens read the frame *after* the replay (the terminal's next live
//!   byte) and the history fill reads until the stream closes, so an extra frame is part of the
//!   answer rather than left unread behind a count.
//!
//! Making the trio genuinely two-sided is not something this crate can do cheaply: the second
//! implementation of the store and the roster is `tddy-daemon`'s (`DaemonTerminalSessionStore`,
//! `CliSessionManager`'s registry), and reaching it means a dev-dependency on the crate `#unbundle`
//! is splitting. So a change *inside* `tddy-terminal-rpc` moves both sides together and is
//! invisible here by construction; the daemon's half of the guard is
//! `packages/tddy-daemon/tests/terminal_session_acceptance.rs` and
//! `packages/tddy-daemon/tests/sandbox_terminal_parity_acceptance.rs`, against its own store; the
//! shared handlers are covered in `tddy-terminal-rpc` itself. Nothing in this repo compares the two
//! running servers end to end.
//!
//! Run: `cargo test -p tddy-coder --test two_server_parity_acceptance`

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use tddy_coder::session_participant::terminal_manager::{PtyHandle, TerminalManager};
use tddy_coder::session_participant::terminal_session_adapter::CoderTerminalSessionStore;
use tddy_coder::session_participant::terminal_session_service::CoderTerminalRoster;
use tddy_coder::session_participant::{
    session_service_entries, SessionConnectionService, ToolExecutor, ToolOutcome,
};
use tddy_rpc::{Code, RpcMessage, RpcResult, ServiceEntry, Status};
use tddy_terminal_rpc::proto::terminal_session::{
    ClaimTerminalControlRequest, ClaimTerminalControlResponse, GetTerminalHistoryRequest,
    SendTerminalInputResponse, SessionTerminalInput, SessionTerminalOutput,
    StopTerminalSessionRequest, StreamReplayMode, StreamTerminalOutputRequest,
    TerminalHistoryChunk, WatchTerminalControlRequest,
};
use tddy_terminal_rpc::{
    build_terminal_session_entry, ControlChange, ControlClaim, TerminalControl,
    TerminalSessionPorts,
};
use tokio::sync::broadcast;

/// The coordinate both wirings are dispatched at, as the crate that serves it publishes it.
const TERMINAL_SERVICE: &str = tddy_terminal_rpc::TERMINAL_SESSION_SERVICE;
/// The coordinate the session's tools stay on (`#unbundle` node 8).
const EXEC_TOOL_SERVICE: &str = tddy_tool_engine::EXEC_TOOL_SERVICE;

const SESSION_ID: &str = "sess-aaaaaaaa-0000-4000-8000-000000000001";
const SESSION_TOKEN: &str = "caller-token";
const SCREEN_ID: &str = "screen-ada-laptop";

/// The reserved id of a session's agent terminal, which no `StopTerminalSession` may end.
const MAIN_TERMINAL_ID: &str = "main";

/// A terminal's retained output, chosen long enough that a history fill has offsets to get wrong
/// and short enough to read in a failure message.
const RETAINED_OUTPUT: &[u8] = b"$ cargo test -p tddy-coder\nrunning 215 tests\n";

/// How long a server-streaming answer is given to produce its next frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// The session, under both wirings
// ---------------------------------------------------------------------------

/// One session's terminal, registered under both wirings of the coordinate.
///
/// The terminal is a real PTY from the coder's own [`TerminalManager`] — the store both wirings
/// resolve through — running `/bin/cat`, which produces no output of its own. That silence is what
/// makes the comparison meaningful: the capture ring holds exactly the bytes this fixture put
/// there, so a difference between the two answers is a difference between the two wirings rather
/// than a shell that happened to print a prompt between them.
struct SessionUnderBothWirings {
    /// The coder participant's registration, as `run.rs` makes it.
    coder_participant: Wiring,
    /// The shared constructor called bare, as `tddy-daemon` calls it.
    daemon_constructor: Wiring,
    terminal_id: String,
    terminal: Arc<PtyHandle>,
    /// Kept alive: the terminal's working directory is deleted when this drops.
    _worktree: tempfile::TempDir,
}

impl SessionUnderBothWirings {
    /// A session whose terminal has already produced `output`.
    async fn with_retained_output(output: &[u8]) -> Self {
        let worktree = tempfile::tempdir().expect("a worktree for the session's terminal");
        let manager = Arc::new(TerminalManager::new());
        let svc = Arc::new(a_session_service(&manager, worktree.path()));

        // `/bin/cat` idles until it is fed, so the ring below stays exactly as this fixture leaves
        // it. Started directly rather than through `StartTerminalSession`, which would resolve the
        // caller's login shell and print a prompt into the very bytes under comparison.
        let terminal = manager
            .start_terminal(SESSION_ID, worktree.path().to_path_buf(), "/bin/cat")
            .await
            .expect("a live terminal for the session");
        terminal
            .capture
            .lock()
            .expect("the capture ring")
            .append(output);

        let coder_participant = Wiring::new(coder_participant_terminal_entry(Arc::clone(&svc)));
        let daemon_constructor = Wiring::new(daemon_constructor_terminal_entry(
            &manager,
            Arc::clone(&svc),
        ));
        SessionUnderBothWirings {
            coder_participant,
            daemon_constructor,
            terminal_id: terminal.terminal_id.clone(),
            terminal,
            _worktree: worktree,
        }
    }

    /// A `StreamTerminalOutput` open against this session's terminal, on first connect.
    fn a_tail_open(&self) -> StreamTerminalOutputRequest {
        StreamTerminalOutputRequest {
            session_token: SESSION_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: self.terminal_id.clone(),
            initial_cols: 0,
            initial_rows: 0,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
        }
    }

    /// A `StreamTerminalOutput` re-open resuming from `from_offset`, as a reconnecting client sends.
    fn a_resume_open(&self, from_offset: u64) -> StreamTerminalOutputRequest {
        StreamTerminalOutputRequest {
            mode: StreamReplayMode::FromOffset as i32,
            from_offset,
            ..self.a_tail_open()
        }
    }

    /// A forward history fill from the oldest retained byte, with no upper bound.
    fn a_history_fill(&self) -> GetTerminalHistoryRequest {
        GetTerminalHistoryRequest {
            session_token: SESSION_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            terminal_id: self.terminal_id.clone(),
            from_offset: 0,
            until_offset: 0,
            max_bytes: 0,
        }
    }

    /// Fresh output from this session's terminal: appended to the capture ring and broadcast, as
    /// the PTY's reader does. What an open stream's next frame carries.
    fn produces(&self, output: &[u8]) {
        self.terminal
            .capture
            .lock()
            .expect("the capture ring")
            .append(output);
        let _ = self
            .terminal
            .stdout_tx
            .send(tddy_pty::Bytes::copy_from_slice(output));
    }

    /// A transcript of everything this session's one PTY echoes from now on.
    fn pty_transcript(&self) -> PtyTranscript {
        PtyTranscript {
            stdout: self.terminal.stdout_tx.subscribe(),
            echoed: Vec::new(),
            read_up_to: 0,
        }
    }

    /// The offset-anchored frame an open emits before any live byte, as this session's terminal.
    fn a_replay_frame(
        &self,
        data: &[u8],
        start_offset: u64,
        end_offset: u64,
        at_oldest: bool,
    ) -> SessionTerminalOutput {
        SessionTerminalOutput {
            data: data.to_vec(),
            acked_input_offset: 0,
            start_offset,
            end_offset,
            at_oldest,
            session_id: SESSION_ID.to_string(),
            terminal_id: self.terminal_id.clone(),
        }
    }

    /// A live output frame, carrying no offsets because it is contiguous with the stream.
    fn a_live_frame(&self, data: &[u8]) -> SessionTerminalOutput {
        SessionTerminalOutput {
            data: data.to_vec(),
            acked_input_offset: 0,
            start_offset: 0,
            end_offset: 0,
            at_oldest: false,
            session_id: SESSION_ID.to_string(),
            terminal_id: self.terminal_id.clone(),
        }
    }

    /// Keystrokes typed into this session's terminal, reaching cumulative `input_offset`.
    fn typing(&self, data: &[u8], input_offset: u64) -> SessionTerminalInput {
        SessionTerminalInput {
            session_token: SESSION_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            data: data.to_vec(),
            terminal_id: self.terminal_id.clone(),
            control_token: String::new(),
            input_offset,
            mode: StreamReplayMode::Tail as i32,
            from_offset: 0,
            initial_cols: 0,
            initial_rows: 0,
        }
    }
}

/// One wiring of the coordinate, dispatched the way a transport dispatches it.
struct Wiring {
    entry: ServiceEntry,
}

impl Wiring {
    fn new(entry: ServiceEntry) -> Self {
        Wiring { entry }
    }

    /// The decoded answer of a unary method at this wiring's registered coordinate.
    async fn answer<Req: Message, Resp: Message + Default>(
        &self,
        method: &str,
        request: Req,
    ) -> Resp {
        match self.dispatch(TERMINAL_SERVICE, method, request).await {
            RpcResult::Unary(Ok(bytes)) => Resp::decode(&bytes[..]).expect("a decodable response"),
            RpcResult::Unary(Err(status)) => panic!("{method} was refused: {status:?}"),
            RpcResult::ServerStream(_) => panic!("{method} answered with a stream"),
        }
    }

    /// Why a method at this wiring's registered coordinate refused.
    async fn refusal<Req: Message>(&self, method: &str, request: Req) -> Status {
        self.refusal_at(TERMINAL_SERVICE, method, request).await
    }

    /// Why a method refused when addressed at `service` — the coordinate a caller named.
    async fn refusal_at<Req: Message>(&self, service: &str, method: &str, request: Req) -> Status {
        match self.dispatch(service, method, request).await {
            RpcResult::Unary(Err(status)) => status,
            RpcResult::ServerStream(Err(status)) => status,
            _ => panic!("{service}/{method} answered instead of refusing"),
        }
    }

    /// A server-streaming method opened at this wiring's registered coordinate.
    async fn open<Req: Message>(&self, method: &str, request: Req) -> ServedStream {
        match self.dispatch(TERMINAL_SERVICE, method, request).await {
            RpcResult::ServerStream(Ok(rx)) => ServedStream {
                method: method.to_string(),
                frames: rx,
            },
            RpcResult::ServerStream(Err(status)) => panic!("{method} was refused: {status:?}"),
            RpcResult::Unary(_) => panic!("{method} answered without a stream"),
        }
    }

    async fn dispatch<Req: Message>(&self, service: &str, method: &str, request: Req) -> RpcResult {
        let message = RpcMessage::new(request.encode_to_vec(), Default::default());
        self.entry
            .service
            .handle_rpc(service, method, &message)
            .await
    }
}

/// One open server-streaming answer, read frame by frame.
///
/// Frames are read in the groups a method is expected to emit and the frame *after* them one at a
/// time, because a count alone only ever proves a prefix: a stream that emitted the expected
/// frames and then a duplicate — a repeated replay frame, a stray ACK — is indistinguishable from
/// a correct one to a caller that stopped reading at the count it guessed.
struct ServedStream {
    method: String,
    frames: tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
}

impl ServedStream {
    /// The next `count` frames, or fewer if the stream stalls or closes first — which the
    /// comparison against the expected frames then reports as the difference it is.
    async fn frames<Item: Message + Default>(&mut self, count: usize) -> Vec<Item> {
        let mut frames = Vec::new();
        while frames.len() < count {
            match tokio::time::timeout(FRAME_TIMEOUT, self.frames.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    frames.push(Item::decode(&bytes[..]).expect("a decodable frame"))
                }
                Ok(Some(Err(status))) => panic!("{} errored mid-stream: {status:?}", self.method),
                Ok(None) | Err(_) => break,
            }
        }
        frames
    }

    /// Every frame the method emits, read until it closes its stream.
    ///
    /// For a method that ends of its own accord — `GetTerminalHistory` sends its chunks and hangs
    /// up — so the answer is bounded by the close rather than by a count a test chose, and an
    /// extra chunk after the last expected one lands in the vector instead of going unread.
    async fn frames_until_close<Item: Message + Default>(&mut self) -> Vec<Item> {
        let mut frames = Vec::new();
        loop {
            match tokio::time::timeout(FRAME_TIMEOUT, self.frames.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    frames.push(Item::decode(&bytes[..]).expect("a decodable frame"))
                }
                Ok(Some(Err(status))) => panic!("{} errored mid-stream: {status:?}", self.method),
                Ok(None) => return frames,
                Err(_) => panic!(
                    "{} left its stream open after {} frames instead of closing it",
                    self.method,
                    frames.len()
                ),
            }
        }
    }

    /// The single next frame, which must arrive.
    async fn next_frame<Item: Message + Default>(&mut self) -> Item {
        match tokio::time::timeout(FRAME_TIMEOUT, self.frames.recv()).await {
            Ok(Some(Ok(bytes))) => Item::decode(&bytes[..]).expect("a decodable frame"),
            Ok(Some(Err(status))) => panic!("{} errored mid-stream: {status:?}", self.method),
            Ok(None) => panic!(
                "{} closed its stream instead of sending a frame",
                self.method
            ),
            Err(_) => panic!("{} sent no further frame", self.method),
        }
    }
}

/// Everything the one PTY has echoed since this transcript was opened.
///
/// The session's child is `/bin/cat` under a PTY, so every byte written to the terminal comes back
/// out of it: the line discipline echoes the keystrokes (turning the typed `\n` into `\r\n`) and
/// `cat` copies the line it then reads. That round trip is how a test outside the process that owns
/// the PTY's stdin observes what was actually written — a wiring that forwarded a request's
/// `input_offset` and dropped its `data` leaves nothing here.
struct PtyTranscript {
    stdout: broadcast::Receiver<tddy_pty::Bytes>,
    echoed: Vec<u8>,
    /// How far into [`Self::echoed`] the keystrokes matched so far reach, so the next one is looked
    /// for *after* them and the order of two keystrokes is part of what is asserted.
    read_up_to: usize,
}

impl PtyTranscript {
    /// Wait until the PTY has echoed each of `keystrokes`, in the order given.
    ///
    /// Reads the terminal's own output rather than sleeping: each read is bounded by
    /// [`FRAME_TIMEOUT`], and a keystroke that never comes back fails with the transcript so far.
    async fn awaits_echo_of_in_order(&mut self, keystrokes: &[&str]) {
        for keystroke in keystrokes {
            while position_of(&self.echoed[self.read_up_to..], keystroke.as_bytes()).is_none() {
                match tokio::time::timeout(FRAME_TIMEOUT, self.stdout.recv()).await {
                    Ok(Ok(bytes)) => self.echoed.extend_from_slice(&bytes),
                    Ok(Err(error)) => panic!(
                        "the terminal's output ended before it echoed {keystroke:?}: {error} \
                         (echoed so far: {:?})",
                        String::from_utf8_lossy(&self.echoed)
                    ),
                    Err(_) => panic!(
                        "the terminal never echoed {keystroke:?} within {FRAME_TIMEOUT:?} \
                         (echoed so far: {:?})",
                        String::from_utf8_lossy(&self.echoed)
                    ),
                }
            }
            self.read_up_to += position_of(&self.echoed[self.read_up_to..], keystroke.as_bytes())
                .expect("the keystroke was just found")
                + keystroke.len();
        }
    }
}

/// Where `needle` starts in `haystack`, if it is there at all.
fn position_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// The two wirings
// ---------------------------------------------------------------------------

/// The coder participant's terminal coordinate, exactly as `run.rs` registers it.
fn coder_participant_terminal_entry(svc: Arc<SessionConnectionService>) -> ServiceEntry {
    tddy_coder::session_participant::coder_terminal_session_entry(svc)
}

/// The shared constructor `tddy-daemon` calls, over the *same* session.
///
/// This is not the daemon's running server — see the module doc's known gap. It is
/// `build_terminal_session_entry` with no method filter in front of it; the store and roster are
/// the same ones the coder's entry resolves through, because this is one session reached a second
/// way rather than a second session. What differs is the control lease: this one arbitrates
/// between screens the way the daemon's registry does, which is the port whose answer this suite
/// asks for on both sides.
fn daemon_constructor_terminal_entry(
    manager: &Arc<TerminalManager>,
    svc: Arc<SessionConnectionService>,
) -> ServiceEntry {
    build_terminal_session_entry(TerminalSessionPorts {
        github_users: Arc::new(|_token: &str| Some("ada".to_string())),
        os_users: Arc::new(|_identity: &str| Some("ada-os".to_string())),
        terminals: Arc::new(CoderTerminalSessionStore::new(Arc::clone(manager))),
        control: Arc::new(ArbitratedControl::unheld()),
        roster: Arc::new(CoderTerminalRoster::new(svc)),
        initial_frame_bytes: tddy_terminal_rpc::bridge::DEFAULT_INITIAL_FRAME_BYTES,
    })
}

/// A control lease that arbitrates between screens, as the daemon's `CliSessionManager` does: an
/// unheld lease grants and accepts any token, and the holder is named once one exists.
struct ArbitratedControl {
    lease: tokio::sync::Mutex<Option<(String, String)>>,
    changes: broadcast::Sender<ControlChange>,
}

impl ArbitratedControl {
    fn unheld() -> Self {
        let (changes, _) = broadcast::channel(4);
        ArbitratedControl {
            lease: tokio::sync::Mutex::new(None),
            changes,
        }
    }
}

#[async_trait]
impl TerminalControl for ArbitratedControl {
    async fn claim(&self, _session_id: &str, screen_id: &str, steal: bool) -> ControlClaim {
        let mut lease = self.lease.lock().await;
        match lease.as_ref() {
            Some((_, holder)) if holder != screen_id && !steal => ControlClaim::Denied {
                holder_screen_id: holder.clone(),
            },
            _ => {
                let control_token = format!("control-token-for-{screen_id}");
                *lease = Some((control_token.clone(), screen_id.to_string()));
                ControlClaim::Granted { control_token }
            }
        }
    }

    async fn verify(&self, _session_id: &str, control_token: &str) -> bool {
        match self.lease.lock().await.as_ref() {
            None => true,
            Some((held, _)) => held == control_token,
        }
    }

    async fn holder_screen_id(&self, _session_id: &str) -> Option<String> {
        self.lease
            .lock()
            .await
            .as_ref()
            .map(|(_, holder)| holder.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.changes.subscribe()
    }
}

/// The session the coder participant serves, with no tools wired — this suite is about terminals.
fn a_session_service(
    manager: &Arc<TerminalManager>,
    worktree: &std::path::Path,
) -> SessionConnectionService {
    SessionConnectionService {
        session_id: SESSION_ID.to_string(),
        session_token: SESSION_TOKEN.to_string(),
        tool_calls_path: worktree.join("tool-calls.jsonl"),
        tools: vec![tddy_coder::session_participant::ToolDef {
            name: "Echo".to_string(),
            description: "Echo a message".to_string(),
            input_schema_json: r#"{"type":"object"}"#.to_string(),
        }],
        executor: Arc::new(UnusedExecutor),
        worktree: worktree.to_path_buf(),
        terminal_manager: Arc::clone(manager),
        agent_activity_dir: worktree.to_path_buf(),
        presenter_events: None,
    }
}

/// No terminal method executes a tool.
struct UnusedExecutor;

#[async_trait]
impl ToolExecutor for UnusedExecutor {
    async fn execute(&self, _tool_name: &str, _args_json: &str) -> ToolOutcome {
        ToolOutcome::default()
    }
}

// ---------------------------------------------------------------------------
// Parity across the seven methods both wirings serve.
//
// Each of these asks one live PTY the same question twice: once through the coder's production
// wiring of the coordinate, once through the constructor `tddy-daemon` calls. That is what the
// names below claim, and all they claim — see the module doc for which of the two answers are
// computed by different code, which are one handler reached twice, and what carries each test as a
// result.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn both_wirings_open_one_terminal_with_the_same_tail_replay() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — each wiring is asked to open the terminal on first connect
    let mut over_coder = session
        .coder_participant
        .open("StreamTerminalOutput", session.a_tail_open())
        .await;
    let mut over_daemon = session
        .daemon_constructor
        .open("StreamTerminalOutput", session.a_tail_open())
        .await;
    let coder_replay: Vec<SessionTerminalOutput> = over_coder.frames(1).await;
    let daemon_replay: Vec<SessionTerminalOutput> = over_daemon.frames(1).await;

    // Then — the same tail chunk, at the same offsets
    assert_eq!(
        coder_replay, daemon_replay,
        "a tail open must replay identically through both wirings of the coordinate"
    );

    // Then — and it is the retained ring anchored at its own offsets. The equality above is two
    // calls into one bridge (see the module doc), so this literal is what pins what either answers
    assert_eq!(
        coder_replay,
        vec![session.a_replay_frame(RETAINED_OUTPUT, 0, RETAINED_OUTPUT.len() as u64, true)]
    );

    // Then — neither stream follows the replay with anything but the terminal's next live byte, so
    // a second replay frame or a stray ACK cannot hide behind the count read above
    session.produces(b"$ ");
    assert_eq!(
        over_coder.next_frame::<SessionTerminalOutput>().await,
        session.a_live_frame(b"$ ")
    );
    assert_eq!(
        over_daemon.next_frame::<SessionTerminalOutput>().await,
        session.a_live_frame(b"$ ")
    );
}

#[tokio::test]
async fn both_wirings_resume_one_terminal_from_the_same_offset() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let already_painted = 10;

    // When — each wiring is asked to resume from the byte a client already holds
    let mut over_coder = session
        .coder_participant
        .open(
            "StreamTerminalOutput",
            session.a_resume_open(already_painted),
        )
        .await;
    let mut over_daemon = session
        .daemon_constructor
        .open(
            "StreamTerminalOutput",
            session.a_resume_open(already_painted),
        )
        .await;
    let coder_catch_up: Vec<SessionTerminalOutput> = over_coder.frames(1).await;
    let daemon_catch_up: Vec<SessionTerminalOutput> = over_daemon.frames(1).await;

    // Then — the same catch-up, so a client that reconnects the other way is not re-painted
    assert_eq!(
        coder_catch_up, daemon_catch_up,
        "a resume must send the same missed bytes through both wirings of the coordinate"
    );

    // Then — and it is only the bytes past that offset, re-anchored to the tip
    assert_eq!(
        coder_catch_up,
        vec![session.a_replay_frame(
            &RETAINED_OUTPUT[already_painted as usize..],
            already_painted,
            RETAINED_OUTPUT.len() as u64,
            false
        )]
    );

    // Then — and the catch-up is that one frame: the next each stream sends is the terminal's next
    // live byte, not a repeat of bytes the client already holds
    session.produces(b"$ ");
    assert_eq!(
        over_coder.next_frame::<SessionTerminalOutput>().await,
        session.a_live_frame(b"$ ")
    );
    assert_eq!(
        over_daemon.next_frame::<SessionTerminalOutput>().await,
        session.a_live_frame(b"$ ")
    );
}

#[tokio::test]
async fn both_wirings_fill_one_terminals_history_at_the_same_offsets() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — each wiring is asked for the scroll-up fill, read until it hangs up rather than to a
    // count, so an extra chunk is in the answer rather than beyond it
    let over_coder: Vec<TerminalHistoryChunk> = session
        .coder_participant
        .open("GetTerminalHistory", session.a_history_fill())
        .await
        .frames_until_close()
        .await;
    let over_daemon: Vec<TerminalHistoryChunk> = session
        .daemon_constructor
        .open("GetTerminalHistory", session.a_history_fill())
        .await
        .frames_until_close()
        .await;

    // Then — the same chunks, bounded by the same offsets and the same end-of-history marker
    assert_eq!(
        over_coder, over_daemon,
        "history must fill at identical offsets through both wirings of the coordinate"
    );

    // Then — and it is the whole retained ring in one chunk, marked as reaching both ends
    assert_eq!(
        over_coder,
        vec![TerminalHistoryChunk {
            data: RETAINED_OUTPUT.to_vec(),
            start_offset: 0,
            end_offset: RETAINED_OUTPUT.len() as u64,
            at_oldest: true,
            at_end: true,
        }]
    );
}

#[tokio::test]
async fn both_wirings_grant_an_unheld_terminal_control_claim_to_the_same_screen() {
    // Given — a session nobody is driving, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let claim = ClaimTerminalControlRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        screen_id: SCREEN_ID.to_string(),
        steal: false,
    };

    // When — a screen claims control through each
    let over_coder: ClaimTerminalControlResponse = session
        .coder_participant
        .answer("ClaimTerminalControl", claim.clone())
        .await;
    let over_daemon: ClaimTerminalControlResponse = session
        .daemon_constructor
        .answer("ClaimTerminalControl", claim)
        .await;

    // Then — both grant it, and neither names a rival holder. The tokens themselves are each
    // lease's own opaque handle and are deliberately not compared; what a screen acts on is
    // whether it was granted and who it was told is driving.
    assert_eq!(
        (over_coder.granted, over_coder.current_holder_screen_id),
        (over_daemon.granted, over_daemon.current_holder_screen_id),
        "an unheld claim must have the same outcome through both wirings of the coordinate"
    );
    assert!(
        over_coder.granted,
        "an unheld lease grants the claiming screen control"
    );
}

#[tokio::test]
async fn both_wirings_refuse_to_stop_the_sessions_main_terminal() {
    // Given — a session registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let stop_main = StopTerminalSessionRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
        control_token: String::new(),
    };

    // When — each wiring is asked to stop the agent's own terminal
    let over_coder = session
        .coder_participant
        .refusal("StopTerminalSession", stop_main.clone())
        .await;
    let over_daemon = session
        .daemon_constructor
        .refusal("StopTerminalSession", stop_main)
        .await;

    // Then — both refuse it the same way: ending the session is its own RPC, on the daemon
    assert_eq!(
        (over_coder.code(), over_coder.message()),
        (over_daemon.code(), over_daemon.message()),
        "stopping the main terminal must be refused identically through both wirings"
    );
    assert_eq!(over_coder.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn keystrokes_through_either_wiring_reach_the_one_pty() {
    // Given — a session registered under both wirings, and a transcript of what its one PTY echoes
    // from here on
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let mut transcript = session.pty_transcript();

    // When — keystrokes are sent through each wiring in turn
    let _: SendTerminalInputResponse = session
        .coder_participant
        .answer("SendTerminalInput", session.typing(b"echo one\n", 9))
        .await;
    let _: SendTerminalInputResponse = session
        .daemon_constructor
        .answer("SendTerminalInput", session.typing(b"echo two\n", 18))
        .await;

    // Then — the one PTY received both wirings' bytes, in the order they were sent. The offsets
    // above are the client's own declared counters, so nothing about them says a byte was written;
    // this is the terminal itself reporting what it was given
    transcript
        .awaits_echo_of_in_order(&["echo one\r\n", "echo two\r\n"])
        .await;

    // Then — and the terminal's acknowledged offset is the later of the two cumulative counters
    assert_eq!(
        *session.terminal.subscribe_acked_offset().borrow(),
        18,
        "input sent through either wiring advances the same terminal's acknowledged offset"
    );
}

// ---------------------------------------------------------------------------
// The coordinate split
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_terminal_coordinate_does_not_answer_a_connection_service_method() {
    // Given — a session served by the coder participant
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — a caller asks the terminal coordinate for one of the session's tool methods
    let refusal = session
        .coder_participant
        .refusal_at(
            TERMINAL_SERVICE,
            "ListExecTools",
            a_list_exec_tools_request(),
        )
        .await;

    // Then — it is refused as an unknown method. The two coordinates are two services, not one
    // service under two names: a single registration would have answered this, because the
    // connection dispatcher matches on the method alone.
    assert_eq!(
        (refusal.code(), refusal.message()),
        (Code::NotFound, "Unknown method: ListExecTools")
    );
}

#[tokio::test]
async fn the_terminal_coordinate_does_not_answer_under_the_connection_service_name() {
    // Given — a session served by the coder participant
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — a caller addresses a terminal method at the *connection* coordinate on it
    let refusal = session
        .coder_participant
        .refusal_at(
            "connection.ConnectionService",
            "StreamTerminalOutput",
            session.a_tail_open(),
        )
        .await;

    // Then — it is refused as an unknown service: the participant no longer registers
    // `connection.ConnectionService` at all
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            Code::NotFound,
            "Unknown service: connection.ConnectionService"
        )
    );
}

#[tokio::test]
async fn the_exec_tool_coordinate_does_not_stream_a_terminal() {
    // Given — a session participant's exec-tool coordinate
    let worktree = tempfile::tempdir().expect("a worktree for the session");
    let manager = Arc::new(TerminalManager::new());
    let exec_tools = Wiring::new(a_registered_entry(
        session_service_entries(a_session_service(&manager, worktree.path())),
        EXEC_TOOL_SERVICE,
    ));

    // When — a client opens a terminal output stream on the wrong coordinate
    let refusal = exec_tools
        .refusal_at(
            EXEC_TOOL_SERVICE,
            "StreamTerminalOutput",
            a_terminal_open_request(),
        )
        .await;

    // Then — it is refused: terminal streaming lives on `terminal_session.TerminalSessionService`
    assert_eq!(refusal.code(), Code::Unimplemented);
}

/// The entry registered under `service` among a participant's coordinates.
fn a_registered_entry(entries: Vec<ServiceEntry>, service: &str) -> ServiceEntry {
    entries
        .into_iter()
        .find(|entry| entry.name == service)
        .unwrap_or_else(|| panic!("the participant registers {service}"))
}

/// A `StreamTerminalOutput` open for a session's main terminal, addressed at no live terminal in
/// particular — a refusal is decided before one is resolved.
fn a_terminal_open_request() -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: MAIN_TERMINAL_ID.to_string(),
        initial_cols: 0,
        initial_rows: 0,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
    }
}

#[tokio::test]
async fn the_terminal_coordinate_refuses_the_bidi_stream_the_coder_does_not_serve() {
    // Given — a session served by the coder participant
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — a client opens the bidi terminal stream on it
    let refusal = session
        .coder_participant
        .refusal("StreamSessionTerminalIO", session.typing(b"", 0))
        .await;

    // Then — it is refused: the coder carries a session's bytes on
    // `terminal.TerminalService/StreamTerminalIO`, and a second bidi terminal here would be new
    // behaviour rather than a moved one. See
    // `docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md`.
    assert_eq!(refusal.code(), Code::Unimplemented);
}

#[tokio::test]
async fn the_terminal_coordinate_refuses_to_report_a_control_lease_it_does_not_arbitrate() {
    // Given — a session served by the coder participant
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let watch = WatchTerminalControlRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        control_token: String::new(),
    };

    // When — a screen asks who is driving the session's terminals
    let refusal = session
        .coder_participant
        .refusal("WatchTerminalControl", watch)
        .await;

    // Then — it is refused rather than answered. The coder's lease is a permanent grant, so it
    // would tell every watching screen that it is the controller — including one the daemon's real
    // lease has just displaced, which is the screen the event exists to correct.
    assert_eq!(refusal.code(), Code::Unimplemented);
}

#[tokio::test]
async fn the_exec_tool_coordinate_answers_the_sessions_tool_catalog() {
    // Given — a session participant's registered coordinates
    let worktree = tempfile::tempdir().expect("a worktree for the session");
    let manager = Arc::new(TerminalManager::new());
    let exec_tools = Wiring::new(a_registered_entry(
        session_service_entries(a_session_service(&manager, worktree.path())),
        EXEC_TOOL_SERVICE,
    ));

    // When — the session's tool catalog is asked for at the exec-tool coordinate
    let listed = match exec_tools
        .dispatch(
            EXEC_TOOL_SERVICE,
            "ListExecTools",
            a_list_exec_tools_request(),
        )
        .await
    {
        RpcResult::Unary(Ok(bytes)) => {
            tddy_service::proto::exec_tools::ListExecToolsResponse::decode(&bytes[..])
                .expect("a decodable catalog")
        }
        RpcResult::Unary(Err(status)) => panic!("ListExecTools was refused: {status:?}"),
        RpcResult::ServerStream(_) => panic!("ListExecTools answered with a stream"),
    };

    // Then — it answers, so moving the terminal family did not take the tools with it
    assert_eq!(
        listed
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Echo"]
    );
}

/// The session's tool-catalog request, which belongs to `exec_tools.ExecToolService`.
fn a_list_exec_tools_request() -> tddy_service::proto::exec_tools::ListExecToolsRequest {
    tddy_service::proto::exec_tools::ListExecToolsRequest {
        session_token: SESSION_TOKEN.to_string(),
        daemon_instance_id: "local".to_string(),
    }
}
