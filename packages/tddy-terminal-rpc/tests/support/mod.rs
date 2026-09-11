//! In-memory scaffolding shared by this crate's terminal test suites.
//!
//! `StubTerminal` / `StubStore` stand in for a live PTY: a real [`TerminalCapture`] ring plus the
//! broadcast and watch channels the bridge subscribes to, recording resizes, inputs and redraws so
//! a test can assert on them. `TerminalServiceHost` wires those into the ports
//! `terminal_session.TerminalSessionService` needs and hands back the registered service entry, so
//! a suite dispatches on the wire rather than calling a handler directly.
//!
//! Each suite uses a subset of what is here — the alternative to allowing dead code is a copy of
//! the stubs per suite, which is the drift these tests exist to catch.
#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
use tddy_task::TerminalCapture;
use tddy_terminal_rpc::proto::terminal_session::{
    SessionTerminalInput, SessionTerminalOutput, StreamReplayMode, StreamTerminalOutputRequest,
};
use tddy_terminal_rpc::session::{TerminalSession, TerminalSessionStore};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::timeout;

pub const RECV_TIMEOUT: Duration = Duration::from_secs(2);

/// A stub terminal backed by a real `TerminalCapture` ring plus broadcast/watch channels the
/// bridge subscribes to. Records resizes, inputs, and redraws so tests can assert on them.
pub struct StubTerminal {
    pub capture: std::sync::Arc<Mutex<TerminalCapture>>,
    pub stdout_tx: broadcast::Sender<Bytes>,
    pub pty_done_tx: watch::Sender<bool>,
    pub acked_tx: watch::Sender<u64>,
    pub resizes: Mutex<Vec<(u16, u16)>>,
    pub inputs: Mutex<Vec<(Bytes, u64)>>,
    pub redraws: AtomicUsize,
}

impl StubTerminal {
    pub fn new() -> Self {
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

    pub fn write(&self, bytes: &[u8]) {
        self.capture.lock().unwrap().append(bytes);
        let _ = self.stdout_tx.send(Bytes::copy_from_slice(bytes));
    }

    pub fn set_acked(&self, offset: u64) {
        self.acked_tx.send_replace(offset);
    }

    pub fn end(&self) {
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
pub struct StubStore {
    pub terminal: Option<std::sync::Arc<StubTerminal>>,
}

impl StubStore {
    /// Wrap a terminal in the store, returning the store and a shared handle the test drives.
    pub fn with(terminal: StubTerminal) -> (Self, std::sync::Arc<StubTerminal>) {
        let arc = std::sync::Arc::new(terminal);
        (
            StubStore {
                terminal: Some(arc.clone()),
            },
            arc,
        )
    }

    /// A second store over a terminal a test already holds, so an oracle reads exactly the ring the
    /// thing under test reads.
    pub fn sharing(terminal: std::sync::Arc<StubTerminal>) -> Self {
        StubStore {
            terminal: Some(terminal),
        }
    }

    /// An empty store that exposes no terminal.
    pub fn empty() -> Self {
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
pub async fn drain(
    rx: mpsc::Receiver<Result<SessionTerminalOutput, tddy_rpc::Status>>,
) -> Vec<SessionTerminalOutput> {
    let mut rx = rx;
    let mut out = Vec::new();
    while let Ok(Some(frame)) = timeout(RECV_TIMEOUT, rx.recv()).await {
        out.push(frame.unwrap());
    }
    out
}

pub fn req(
    session_id: &str,
    terminal_id: &str,
    cols: u32,
    rows: u32,
) -> StreamTerminalOutputRequest {
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
pub fn req_from_offset(
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

/// An input stream for the bidi helper: a fixed sequence of `SessionTerminalInput` chunks drained by
/// the bridge's spawned forwarder. `tokio_stream::iter` yields the items then ends, so the
/// forwarder task exits after the last chunk.
pub fn input_stream(
    msgs: Vec<SessionTerminalInput>,
) -> impl tokio_stream::Stream<Item = Result<SessionTerminalInput, tddy_rpc::Status>> + Send + Unpin
{
    tokio_stream::iter(msgs.into_iter().map(Ok))
}

/// A `verify_control` closure that always approves (tests do not exercise control-token theft).
pub fn always_allow_control() -> impl Fn(&str, &str) -> std::future::Ready<bool> + Send + Sync {
    move |_sid, _tok| std::future::ready(true)
}

/// Every frame of a stream, whatever its kind, must name the terminal it came from.
pub fn assert_all_frames_stamped(
    frames: &[SessionTerminalOutput],
    session_id: &str,
    terminal_id: &str,
) {
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(
            frame.session_id,
            session_id,
            "frame {index} (data={:?}) must name its session",
            String::from_utf8_lossy(&frame.data)
        );
        assert_eq!(
            frame.terminal_id,
            terminal_id,
            "frame {index} (data={:?}) must name its terminal",
            String::from_utf8_lossy(&frame.data)
        );
    }
}

// ---------------------------------------------------------------------------
// terminal_session.TerminalSessionService: the served coordinate
// ---------------------------------------------------------------------------

/// The one session token [`TerminalServiceHost`] authenticates.
pub const SESSION_TOKEN: &str = "session-token-for-ada";
/// The identity that token belongs to.
pub const GITHUB_USER: &str = "ada";
/// The OS user that identity is mapped to on the host.
pub const OS_USER: &str = "ada-os";
/// The session every request below addresses.
pub const SESSION_ID: &str = "session-alpha";
/// A screen claiming terminal control.
pub const SCREEN_ID: &str = "screen-ada-laptop";

/// The control lease, in memory, with the semantics the RPC promises: an unheld lease grants, the
/// holder re-claiming gets its own token back, another screen is refused unless it steals, and a
/// steal notifies watchers.
///
/// A fake rather than a mock because the four methods are stateful across calls — "claim then
/// verify then watch" is one story, and a mock would have to be told the answer to each step.
pub struct InMemoryControl {
    lease: tokio::sync::Mutex<Option<ControlLease>>,
    changes: broadcast::Sender<tddy_terminal_rpc::ControlChange>,
    issued: AtomicUsize,
}

struct ControlLease {
    control_token: String,
    screen_id: String,
}

impl InMemoryControl {
    pub fn unheld() -> Self {
        let (changes, _) = broadcast::channel(16);
        InMemoryControl {
            lease: tokio::sync::Mutex::new(None),
            changes,
            issued: AtomicUsize::new(0),
        }
    }

    /// Tokens are numbered rather than random so a test can assert the exact one it was handed.
    fn issue(&self) -> String {
        let nth = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        format!("control-token-{nth}")
    }
}

#[async_trait]
impl tddy_terminal_rpc::TerminalControl for InMemoryControl {
    async fn claim(
        &self,
        _session_id: &str,
        screen_id: &str,
        steal: bool,
    ) -> tddy_terminal_rpc::ControlClaim {
        use tddy_terminal_rpc::ControlClaim;
        let mut lease = self.lease.lock().await;
        match lease.as_ref() {
            Some(held) if held.screen_id == screen_id => ControlClaim::Granted {
                control_token: held.control_token.clone(),
            },
            Some(held) if !steal => ControlClaim::Denied {
                holder_screen_id: held.screen_id.clone(),
            },
            _ => {
                let control_token = self.issue();
                *lease = Some(ControlLease {
                    control_token: control_token.clone(),
                    screen_id: screen_id.to_string(),
                });
                let _ = self.changes.send(tddy_terminal_rpc::ControlChange {
                    session_id: SESSION_ID.to_string(),
                    holder_screen_id: screen_id.to_string(),
                });
                ControlClaim::Granted { control_token }
            }
        }
    }

    async fn verify(&self, _session_id: &str, control_token: &str) -> bool {
        match self.lease.lock().await.as_ref() {
            None => true,
            Some(held) => held.control_token == control_token,
        }
    }

    async fn holder_screen_id(&self, _session_id: &str) -> Option<String> {
        self.lease
            .lock()
            .await
            .as_ref()
            .map(|held| held.screen_id.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<tddy_terminal_rpc::ControlChange> {
        self.changes.subscribe()
    }
}

/// The terminals of a session, in memory: `start` records the shell it was asked for and appends a
/// numbered entry, `stop` removes one, `list` reports what is left.
pub struct InMemoryRoster {
    terminals: Mutex<Vec<tddy_terminal_rpc::TerminalDescriptor>>,
    started_shells: Mutex<Vec<String>>,
    started: AtomicUsize,
}

impl InMemoryRoster {
    /// A roster holding only the session's main terminal, as a freshly started session has.
    pub fn with_main_terminal() -> Self {
        InMemoryRoster {
            terminals: Mutex::new(vec![tddy_terminal_rpc::TerminalDescriptor {
                terminal_id: tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID.to_string(),
                kind: "claude-cli".to_string(),
                pid: 4242,
            }]),
            started_shells: Mutex::new(Vec::new()),
            started: AtomicUsize::new(0),
        }
    }

    /// The shell path each `StartTerminalSession` asked for, in order.
    pub fn started_shells(&self) -> Vec<String> {
        self.started_shells.lock().unwrap().clone()
    }
}

#[async_trait]
impl tddy_terminal_rpc::TerminalRoster for InMemoryRoster {
    async fn start(&self, _session_id: &str, shell_path: &str) -> Result<String, tddy_rpc::Status> {
        self.started_shells
            .lock()
            .unwrap()
            .push(shell_path.to_string());
        let nth = self.started.fetch_add(1, Ordering::SeqCst) + 1;
        let terminal_id = format!("bash-{nth}");
        self.terminals
            .lock()
            .unwrap()
            .push(tddy_terminal_rpc::TerminalDescriptor {
                terminal_id: terminal_id.clone(),
                kind: "bash".to_string(),
                pid: 5000 + nth as u32,
            });
        Ok(terminal_id)
    }

    async fn stop(&self, _session_id: &str, terminal_id: &str) -> bool {
        let mut terminals = self.terminals.lock().unwrap();
        let before = terminals.len();
        terminals.retain(|terminal| terminal.terminal_id != terminal_id);
        terminals.len() != before
    }

    async fn list(&self, _session_id: &str) -> Vec<tddy_terminal_rpc::TerminalDescriptor> {
        self.terminals.lock().unwrap().clone()
    }
}

/// A host serving `terminal_session.TerminalSessionService`, driven the way a transport drives it:
/// requests go in as encoded bytes at the registered service name and answers come back off the
/// wire, so a test proves the *registration* rather than the handler behind it.
pub struct TerminalServiceHost {
    entry: tddy_rpc::ServiceEntry,
    /// The live terminal the streaming methods attach to, when this host has one.
    pub terminal: Option<std::sync::Arc<StubTerminal>>,
    pub control: std::sync::Arc<InMemoryControl>,
    pub roster: std::sync::Arc<InMemoryRoster>,
}

/// The service name every dispatch below addresses: the coordinate the crate publishes, so a
/// dispatch here reaches the same address a real caller puts on the wire.
pub const SERVICE: &str = tddy_terminal_rpc::TERMINAL_SESSION_SERVICE;

/// Whether the host has an OS user for the identity its token authenticates.
///
/// The two refusals are different remedies and must not collapse into one: an unknown token means
/// re-authenticate, an unmapped identity means ask an operator.
#[derive(Clone, Copy)]
enum UserMapping {
    Mapped,
    Unmapped,
}

impl TerminalServiceHost {
    /// A host whose session has one live terminal — the reserved main one — with `output` already
    /// in its capture ring.
    pub fn serving_main_terminal(output: &[u8]) -> Self {
        let terminal = StubTerminal::new();
        terminal.write(output);
        let (store, handle) = StubStore::with(terminal);
        Self::over(store, Some(handle), UserMapping::Mapped)
    }

    /// A host serving a terminal the test already holds, so it can drive the terminal and read an
    /// oracle off the same ring.
    pub fn sharing_terminal(terminal: std::sync::Arc<StubTerminal>) -> Self {
        Self::over(
            StubStore::sharing(std::sync::Arc::clone(&terminal)),
            Some(terminal),
            UserMapping::Mapped,
        )
    }

    /// A host whose session has no live terminal, so every streaming method must refuse.
    pub fn serving_no_terminal() -> Self {
        Self::over(StubStore::empty(), None, UserMapping::Mapped)
    }

    /// A host that authenticates [`SESSION_TOKEN`] but has no OS user for the identity behind it —
    /// the operator never added the mapping.
    pub fn serving_main_terminal_for_unmapped_user(output: &[u8]) -> Self {
        let terminal = StubTerminal::new();
        terminal.write(output);
        let (store, handle) = StubStore::with(terminal);
        Self::over(store, Some(handle), UserMapping::Unmapped)
    }

    fn over(
        store: StubStore,
        terminal: Option<std::sync::Arc<StubTerminal>>,
        mapping: UserMapping,
    ) -> Self {
        let control = std::sync::Arc::new(InMemoryControl::unheld());
        let roster = std::sync::Arc::new(InMemoryRoster::with_main_terminal());
        let entry = tddy_terminal_rpc::build_terminal_session_entry(
            tddy_terminal_rpc::TerminalSessionPorts {
                github_users: std::sync::Arc::new(|token: &str| {
                    (token == SESSION_TOKEN).then(|| GITHUB_USER.to_string())
                }),
                os_users: std::sync::Arc::new(move |github_user: &str| match mapping {
                    UserMapping::Mapped => {
                        (github_user == GITHUB_USER).then(|| OS_USER.to_string())
                    }
                    UserMapping::Unmapped => None,
                }),
                terminals: std::sync::Arc::new(store),
                control: control.clone(),
                roster: roster.clone(),
                // Small enough that a handful of bytes exercises the chunking a real ring only
                // reaches after kilobytes of output.
                initial_frame_bytes: 4,
            },
        );
        TerminalServiceHost {
            entry,
            terminal,
            control,
            roster,
        }
    }

    pub fn service_name(&self) -> &str {
        self.entry.name
    }

    /// The live terminal this host serves.
    pub fn terminal(&self) -> &std::sync::Arc<StubTerminal> {
        self.terminal.as_ref().expect("a live terminal")
    }

    /// The input chunks the live terminal has been handed, with their cumulative offsets.
    pub fn typed_into_pty(&self) -> Vec<(Bytes, u64)> {
        self.terminal().inputs.lock().unwrap().clone()
    }

    /// Put `screen_id` in control of the session's terminals and return the token it must present.
    pub async fn control_held_by(&self, screen_id: &str) -> String {
        use tddy_terminal_rpc::ControlClaim;
        match tddy_terminal_rpc::TerminalControl::claim(&*self.control, SESSION_ID, screen_id, true)
            .await
        {
            ControlClaim::Granted { control_token } => control_token,
            ControlClaim::Denied { holder_screen_id } => {
                panic!("a steal was refused by {holder_screen_id}")
            }
        }
    }

    /// The decoded answer of a unary method dispatched at the registered coordinate.
    pub async fn answer<Req: prost::Message, Resp: prost::Message + Default>(
        &self,
        method: &str,
        request: Req,
    ) -> Resp {
        let bytes = match self.dispatch(method, request).await {
            tddy_rpc::RpcResult::Unary(Ok(bytes)) => bytes,
            tddy_rpc::RpcResult::Unary(Err(status)) => panic!("{method} was refused: {status:?}"),
            tddy_rpc::RpcResult::ServerStream(_) => panic!("{method} answered with a stream"),
        };
        Resp::decode(&bytes[..]).expect("a decodable response")
    }

    /// Why a method dispatched at the registered coordinate refused.
    pub async fn refusal<Req: prost::Message>(
        &self,
        method: &str,
        request: Req,
    ) -> tddy_rpc::Status {
        match self.dispatch(method, request).await {
            tddy_rpc::RpcResult::Unary(Err(status)) => status,
            tddy_rpc::RpcResult::ServerStream(Err(status)) => status,
            _ => panic!("{method} answered instead of refusing"),
        }
    }

    /// Every frame of a server-streaming method that closes on its own (`GetTerminalHistory`).
    pub async fn all_frames<Req: prost::Message, Item: prost::Message + Default>(
        &self,
        method: &str,
        request: Req,
    ) -> Vec<Item> {
        self.stream_frames(method, request, usize::MAX).await
    }

    /// The first `count` frames of a server-streaming method, or every frame if it closes sooner.
    ///
    /// Bounded because the output and control streams stay open for the life of the terminal:
    /// draining them to the end would only ever be a timeout.
    pub async fn stream_frames<Req: prost::Message, Item: prost::Message + Default>(
        &self,
        method: &str,
        request: Req,
        count: usize,
    ) -> Vec<Item> {
        let mut rx = match self.dispatch(method, request).await {
            tddy_rpc::RpcResult::ServerStream(Ok(rx)) => rx,
            tddy_rpc::RpcResult::ServerStream(Err(status)) => {
                panic!("{method} was refused: {status:?}")
            }
            tddy_rpc::RpcResult::Unary(_) => panic!("{method} answered without a stream"),
        };
        let mut frames = Vec::new();
        while frames.len() < count {
            match timeout(RECV_TIMEOUT, rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    frames.push(Item::decode(&bytes[..]).expect("a decodable frame"))
                }
                Ok(Some(Err(status))) => panic!("{method} errored mid-stream: {status:?}"),
                Ok(None) | Err(_) => break,
            }
        }
        frames
    }

    /// Why `StreamSessionTerminalIO` refused to open for this first message.
    pub async fn refusal_on_bidi(&self, opening: SessionTerminalInput) -> tddy_rpc::Status {
        let (input, input_rx) = mpsc::channel(8);
        input
            .send(encoded(&opening))
            .await
            .expect("the decoder task takes the opening frame");
        match self
            .entry
            .service
            .start_bidi_stream(SERVICE, "StreamSessionTerminalIO", input_rx)
            .await
        {
            Err(status) => status,
            Ok(_) => panic!("StreamSessionTerminalIO opened instead of refusing"),
        }
    }

    /// Open `StreamSessionTerminalIO` with its first message, the way a bidi transport does.
    pub async fn open_bidi(&self, opening: SessionTerminalInput) -> BidiSession {
        let (input, input_rx) = mpsc::channel(8);
        input
            .send(encoded(&opening))
            .await
            .expect("the decoder task takes the opening frame");
        let started = self
            .entry
            .service
            .start_bidi_stream(SERVICE, "StreamSessionTerminalIO", input_rx)
            .await
            .expect("the bidi stream opened");
        let output = match started.output {
            tddy_rpc::ResponseBody::Streaming(rx) => rx,
            tddy_rpc::ResponseBody::Complete(_) => panic!("bidi answered without a live stream"),
        };
        BidiSession { input, output }
    }

    async fn dispatch<Req: prost::Message>(
        &self,
        method: &str,
        request: Req,
    ) -> tddy_rpc::RpcResult {
        self.entry
            .service
            .handle_rpc(SERVICE, method, &encoded(&request))
            .await
    }
}

/// One open `StreamSessionTerminalIO` stream: the input half a client types into and the output
/// half it renders.
pub struct BidiSession {
    input: mpsc::Sender<tddy_rpc::RpcMessage>,
    output: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
}

impl BidiSession {
    /// Type one more chunk into the open stream.
    pub async fn send(&self, chunk: SessionTerminalInput) {
        self.input
            .send(encoded(&chunk))
            .await
            .expect("the decoder task is still reading input");
    }

    /// End the client's half of the stream, as a disconnecting client does.
    pub fn hang_up(self) -> mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>> {
        self.output
    }

    /// The next output frame, or `None` once the output half has closed.
    ///
    /// Distinct from [`Self::frames`], which stops on a closed stream *and* on a stall and so
    /// cannot tell an ended stream from a silent one: a test that asserts closure reads it here,
    /// where a stall is a failure rather than an end.
    pub async fn next_frame_or_close(&mut self) -> Option<SessionTerminalOutput> {
        match timeout(RECV_TIMEOUT, self.output.recv()).await {
            Ok(Some(Ok(bytes))) => {
                Some(SessionTerminalOutput::decode(&bytes[..]).expect("a decodable output frame"))
            }
            Ok(Some(Err(status))) => panic!("the bidi stream errored: {status:?}"),
            Ok(None) => None,
            Err(_) => panic!(
                "the bidi stream neither produced a frame nor closed within {RECV_TIMEOUT:?}"
            ),
        }
    }

    /// The next `count` output frames, or fewer if the stream closes first.
    pub async fn frames(&mut self, count: usize) -> Vec<SessionTerminalOutput> {
        let mut frames = Vec::new();
        while frames.len() < count {
            match timeout(RECV_TIMEOUT, self.output.recv()).await {
                Ok(Some(Ok(bytes))) => frames.push(
                    SessionTerminalOutput::decode(&bytes[..]).expect("a decodable output frame"),
                ),
                Ok(Some(Err(status))) => panic!("the bidi stream errored: {status:?}"),
                Ok(None) | Err(_) => break,
            }
        }
        frames
    }
}

/// One request as the transport hands it over: an encoded payload with no metadata.
fn encoded<M: prost::Message>(message: &M) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(message.encode_to_vec(), Default::default())
}

/// A `SessionTerminalInput` with every field at its zero value, for a test to override only the
/// ones its scenario is about.
pub fn an_input() -> SessionTerminalInput {
    SessionTerminalInput {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        data: Vec::new(),
        terminal_id: String::new(),
        control_token: String::new(),
        input_offset: 0,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
        initial_cols: 0,
        initial_rows: 0,
    }
}

/// A `StreamTerminalOutputRequest` addressed at the host's session and its main terminal.
pub fn an_output_request() -> StreamTerminalOutputRequest {
    StreamTerminalOutputRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: String::new(),
        initial_cols: 0,
        initial_rows: 0,
        mode: StreamReplayMode::Tail as i32,
        from_offset: 0,
    }
}

/// A `GetTerminalHistoryRequest` addressed at the host's session and its main terminal, filling
/// forward from the oldest retained byte with no upper bound.
pub fn a_history_request() -> tddy_terminal_rpc::GetTerminalHistoryRequest {
    tddy_terminal_rpc::GetTerminalHistoryRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        terminal_id: String::new(),
        from_offset: 0,
        until_offset: 0,
        max_bytes: 0,
    }
}

/// A `SessionTerminalInput` carrying keystrokes, with the cumulative offset they reach.
pub fn typing(data: &[u8], input_offset: u64) -> SessionTerminalInput {
    SessionTerminalInput {
        data: data.to_vec(),
        input_offset,
        ..an_input()
    }
}

/// A `StartTerminalSessionRequest` addressed at the host's session.
pub fn a_start_request() -> tddy_terminal_rpc::proto::terminal_session::StartTerminalSessionRequest
{
    tddy_terminal_rpc::proto::terminal_session::StartTerminalSessionRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        control_token: String::new(),
    }
}

/// A `WatchTerminalControlRequest` from the screen holding `control_token`.
pub fn a_watch_request(
    control_token: &str,
) -> tddy_terminal_rpc::proto::terminal_session::WatchTerminalControlRequest {
    tddy_terminal_rpc::proto::terminal_session::WatchTerminalControlRequest {
        session_token: SESSION_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        control_token: control_token.to_string(),
    }
}
