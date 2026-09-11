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
//! # Known gap: this is one server, wired twice — not two servers
//!
//! Both sides are this process's code, so what this suite proves is bounded: a change to the
//! coder's ports, its method filter, its store adapter or its control lease that skewed replay,
//! resume offsets, history chunking, keystroke forwarding or the claim outcome fails here, because
//! only one side carries it. A change *inside* `tddy-terminal-rpc` moves both sides together and
//! is invisible here by construction — so is anything specific to the daemon's own ports
//! (`CliSessionManager`'s roster, store and control registry), which cannot be reached from this
//! crate without depending on `tddy-daemon`: the crate `#unbundle` is splitting, and far too heavy
//! a test dependency to take on for it. The daemon's half of the guard is
//! `packages/tddy-daemon/tests/terminal_session_acceptance.rs`, against its own server; the shared
//! handlers are covered in `tddy-terminal-rpc` itself. Nothing in this repo compares the two
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
/// The coordinate the session's tools stay on.
const CONNECTION_SERVICE: &str = "connection.ConnectionService";

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

    /// The first `count` frames of a server-streaming method, or fewer if it closes sooner.
    ///
    /// Bounded because an output stream stays open for the life of the terminal: draining one to
    /// the end would only ever be a timeout.
    async fn frames<Req: Message, Item: Message + Default>(
        &self,
        method: &str,
        request: Req,
        count: usize,
    ) -> Vec<Item> {
        let mut rx = match self.dispatch(TERMINAL_SERVICE, method, request).await {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => panic!("{method} was refused: {status:?}"),
            RpcResult::Unary(_) => panic!("{method} answered without a stream"),
        };
        let mut frames = Vec::new();
        while frames.len() < count {
            match tokio::time::timeout(FRAME_TIMEOUT, rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    frames.push(Item::decode(&bytes[..]).expect("a decodable frame"))
                }
                Ok(Some(Err(status))) => panic!("{method} errored mid-stream: {status:?}"),
                Ok(None) | Err(_) => break,
            }
        }
        frames
    }

    async fn dispatch<Req: Message>(&self, service: &str, method: &str, request: Req) -> RpcResult {
        let message = RpcMessage::new(request.encode_to_vec(), Default::default());
        self.entry
            .service
            .handle_rpc(service, method, &message)
            .await
    }
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
// names below claim, and all they claim — see the module doc's known gap for what a comparison of
// the two *running* servers would additionally catch.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn both_wirings_open_one_terminal_with_the_same_tail_replay() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — each wiring is asked to open the terminal on first connect
    let over_coder: Vec<SessionTerminalOutput> = session
        .coder_participant
        .frames("StreamTerminalOutput", session.a_tail_open(), 2)
        .await;
    let over_daemon: Vec<SessionTerminalOutput> = session
        .daemon_constructor
        .frames("StreamTerminalOutput", session.a_tail_open(), 2)
        .await;

    // Then — the same prologue and the same tail chunk, at the same offsets
    assert_eq!(
        over_coder, over_daemon,
        "a tail open must replay identically through both wirings of the coordinate"
    );
}

#[tokio::test]
async fn both_wirings_resume_one_terminal_from_the_same_offset() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;
    let already_painted = 10;

    // When — each wiring is asked to resume from the byte a client already holds
    let over_coder: Vec<SessionTerminalOutput> = session
        .coder_participant
        .frames(
            "StreamTerminalOutput",
            session.a_resume_open(already_painted),
            2,
        )
        .await;
    let over_daemon: Vec<SessionTerminalOutput> = session
        .daemon_constructor
        .frames(
            "StreamTerminalOutput",
            session.a_resume_open(already_painted),
            2,
        )
        .await;

    // Then — the same catch-up, so a client that reconnects the other way is not re-painted
    assert_eq!(
        over_coder, over_daemon,
        "a resume must send the same missed bytes through both wirings of the coordinate"
    );
}

#[tokio::test]
async fn both_wirings_fill_one_terminals_history_at_the_same_offsets() {
    // Given — a session whose terminal holds retained output, registered under both wirings
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — each wiring is asked for the scroll-up fill
    let over_coder: Vec<TerminalHistoryChunk> = session
        .coder_participant
        .frames("GetTerminalHistory", session.a_history_fill(), 8)
        .await;
    let over_daemon: Vec<TerminalHistoryChunk> = session
        .daemon_constructor
        .frames("GetTerminalHistory", session.a_history_fill(), 8)
        .await;

    // Then — the same chunks, bounded by the same offsets and the same end-of-history marker
    assert_eq!(
        over_coder, over_daemon,
        "history must fill at identical offsets through both wirings of the coordinate"
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
    // Given — a session registered under both wirings, with a client that has typed nothing yet
    let session = SessionUnderBothWirings::with_retained_output(RETAINED_OUTPUT).await;

    // When — keystrokes are sent through each wiring in turn
    let _: SendTerminalInputResponse = session
        .coder_participant
        .answer("SendTerminalInput", session.typing(b"echo one\n", 9))
        .await;
    let _: SendTerminalInputResponse = session
        .daemon_constructor
        .answer("SendTerminalInput", session.typing(b"echo two\n", 18))
        .await;

    // Then — both reached the one PTY, which acknowledges the later cumulative offset
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
            CONNECTION_SERVICE,
            "StreamTerminalOutput",
            session.a_tail_open(),
        )
        .await;

    // Then — it is refused as an unknown service: this entry answers at one name only
    assert_eq!(
        (refusal.code(), refusal.message()),
        (
            Code::NotFound,
            "Unknown service: connection.ConnectionService"
        )
    );
}

#[tokio::test]
async fn the_connection_coordinate_no_longer_streams_a_terminal() {
    // Given — a session participant's connection coordinate
    let worktree = tempfile::tempdir().expect("a worktree for the session");
    let manager = Arc::new(TerminalManager::new());
    let connection = Wiring::new(a_registered_entry(
        session_service_entries(a_session_service(&manager, worktree.path())),
        CONNECTION_SERVICE,
    ));

    // When — a client opens a terminal output stream where the family used to live
    let refusal = connection
        .refusal_at(
            CONNECTION_SERVICE,
            "StreamTerminalOutput",
            a_terminal_open_request(),
        )
        .await;

    // Then — it is refused: `#unbundle` node 6 moved the streaming half to
    // `terminal_session.TerminalSessionService`, which is where both servers now answer it
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
async fn the_connection_coordinate_still_answers_the_sessions_tool_catalog() {
    // Given — a session participant's registered coordinates
    let worktree = tempfile::tempdir().expect("a worktree for the session");
    let manager = Arc::new(TerminalManager::new());
    let connection = Wiring::new(a_registered_entry(
        session_service_entries(a_session_service(&manager, worktree.path())),
        CONNECTION_SERVICE,
    ));

    // When — the session's tool catalog is asked for at the connection coordinate
    let listed = match connection
        .dispatch(
            CONNECTION_SERVICE,
            "ListExecTools",
            a_list_exec_tools_request(),
        )
        .await
    {
        RpcResult::Unary(Ok(bytes)) => {
            tddy_service::proto::connection::ListExecToolsResponse::decode(&bytes[..])
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

/// The session's tool-catalog request, which belongs to `connection.ConnectionService`.
fn a_list_exec_tools_request() -> tddy_service::proto::connection::ListExecToolsRequest {
    tddy_service::proto::connection::ListExecToolsRequest {
        session_token: SESSION_TOKEN.to_string(),
        daemon_instance_id: "local".to_string(),
    }
}
