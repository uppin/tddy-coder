//! Test support for the webview-IPC flavour: the two hosts under test behind one adapter, a service
//! to call, a fake sink to observe, a builder for request frames, and domain assertions on response
//! frames.

// Each test binary uses a subset of these helpers, and only the binaries with shared behaviour
// expand `against_both_hosts!`.
#![allow(dead_code, unused_macros)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::envelope::{self, CallMetadata, RpcRequest, RpcResponse};
use tddy_rpc::{
    BidiStreamOutput, MultiRpcService, ResponseBody, RpcMessage, RpcResult, RpcService,
    ServiceEntry, Status,
};
use tddy_tauri_rpc::{
    ConnectionTarget, FrameError, FrameSink, MultiConnectionHost, RosterResolver, SinkClosed,
    WebviewRpcHost,
};
use tokio::sync::{mpsc, oneshot};

/// The service every test calls.
pub const ECHO_SERVICE: &str = "test.EchoService";

/// Safety net around awaits that are driven entirely by channels, not by polling — so a failure
/// reports which frame never arrived instead of hanging the suite. Not an expected duration: the
/// frames these tests wait for are produced in-process, in microseconds.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Hosts under test
// ---------------------------------------------------------------------------

/// A host serving [`ECHO_SERVICE`] through a `MultiRpcService`, the way the daemon serves its own
/// roster — so a call to an unregistered service is answered by the multiplexer rather than by the
/// echo service itself.
pub fn a_webview_rpc_host() -> WebviewRpcHost<MultiRpcService> {
    WebviewRpcHost::new(MultiRpcService::new(vec![ServiceEntry {
        name: ECHO_SERVICE,
        service: Arc::new(EchoService),
    }]))
}

/// The echo roster, as a service a [`RosterResolver`] hands back.
///
/// Same roster `a_webview_rpc_host` serves, exposed on its own so a multi-connection test can give
/// every target its own copy — which is what makes the connections genuinely independent rather
/// than two names for one service.
pub fn an_echo_roster() -> Arc<dyn RpcService> {
    Arc::new(MultiRpcService::new(vec![ServiceEntry {
        name: ECHO_SERVICE,
        service: Arc::new(EchoService),
    }]))
}

/// A [`MultiConnectionHost`] serving that same roster on its daemon target — the host the desktop
/// app actually runs, so behaviour both hosts owe a page can be pinned on the one that ships.
pub fn a_multi_connection_host() -> MultiConnectionHost<DaemonEchoRoster> {
    MultiConnectionHost::new(DaemonEchoRoster)
}

/// Resolves the daemon target to the echo roster, and nothing else.
///
/// Session targets are what [`MultiConnectionHost`] gained over the single-slot host, so they have
/// no counterpart to compare against and belong in the tests written for that host alone
/// (`tests/concurrent_webview_connections.rs`). Answering `None` for them here says so.
pub struct DaemonEchoRoster;

impl RosterResolver for DaemonEchoRoster {
    fn roster_for(&self, target: &ConnectionTarget) -> Option<Arc<dyn RpcService>> {
        match target {
            ConnectionTarget::Daemon => Some(an_echo_roster()),
            ConnectionTarget::Session { .. } => None,
        }
    }
}

/// What both hosts owe a page, in the one shape a test can drive either of them through.
///
/// Deliberately narrow. The two hosts differ on what *opening* a connection does — `WebviewRpcHost`
/// abandons whatever the previous page had, `MultiConnectionHost` refuses a reused epoch and
/// disturbs nothing — so this adapter offers only "connect a page", never "connect a page and see
/// what that did to the others". A test about that difference belongs to the host that has it.
#[async_trait]
pub trait WebviewHost: Send + Sync + 'static {
    /// Register `sink` as the response channel for the page connection `client_epoch` names.
    async fn connect_page(&self, sink: Arc<dyn FrameSink>, client_epoch: u32);

    /// Decode and dispatch one request frame from the page.
    async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError>;
}

#[async_trait]
impl<S: RpcService> WebviewHost for WebviewRpcHost<S> {
    async fn connect_page(&self, sink: Arc<dyn FrameSink>, client_epoch: u32) {
        WebviewRpcHost::connect(self, sink, client_epoch).await;
    }

    async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError> {
        WebviewRpcHost::handle_request_frame(self, frame).await
    }
}

#[async_trait]
impl<R: RosterResolver> WebviewHost for MultiConnectionHost<R> {
    async fn connect_page(&self, sink: Arc<dyn FrameSink>, client_epoch: u32) {
        // A page reaching the daemon is the connection both hosts have, so it is the one shared
        // behaviour is pinned on. The refusals `connect` can return are the multi-connection host's
        // own contract and are covered where that contract lives, so here they are a broken fixture.
        MultiConnectionHost::connect(self, ConnectionTarget::Daemon, sink, client_epoch)
            .await
            .expect("the daemon roster resolves and no test reuses an epoch");
    }

    async fn handle_request_frame(&self, frame: &[u8]) -> Result<(), FrameError> {
        MultiConnectionHost::handle_request_frame(self, frame).await
    }
}

/// Run one test body against both hosts, once each.
///
/// ```ignore
/// against_both_hosts! {
///     async fn answers_a_unary_call(host) {
///         // Given / When / Then, driving `host` through `WebviewHost`
///     }
/// }
/// ```
///
/// Each body becomes `<name>::on_the_single_connection_host` and
/// `<name>::on_the_multi_connection_host`, so a failure names the host it failed on. The two are
/// separate implementations rather than one delegating to the other, so a body that passes on one
/// says nothing about the other until it has run there.
macro_rules! against_both_hosts {
    ($(
        $(#[$attribute:meta])*
        async fn $name:ident($host:ident) $body:block
    )*) => {
        $(
            $(#[$attribute])*
            mod $name {
                use super::*;

                async fn behaviour($host: impl WebviewHost) $body

                #[tokio::test]
                async fn on_the_single_connection_host() {
                    behaviour(a_webview_rpc_host()).await;
                }

                #[tokio::test]
                async fn on_the_multi_connection_host() {
                    behaviour(a_multi_connection_host()).await;
                }
            }
        )*
    };
}

/// Deterministic responses for every call shape the flavour has to carry.
///
/// | Method          | Shape           | Behaviour                                              |
/// |-----------------|-----------------|--------------------------------------------------------|
/// | `Echo`          | unary           | the request payload, verbatim                          |
/// | `EchoStream`    | server stream   | one message per comma-separated part of the payload    |
/// | `StreamAndHold` | server stream   | the payload once, then the stream never completes      |
/// | `Collect`       | client stream   | every request payload joined with a pipe byte          |
/// | `EchoEach`      | bidi stream     | one upper-cased response per request message           |
struct EchoService;

#[async_trait]
impl RpcService for EchoService {
    fn is_bidi_stream(&self, _service: &str, method: &str) -> bool {
        method == "EchoEach"
    }

    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        match method {
            "Echo" => RpcResult::Unary(Ok(message.payload.clone())),
            "EchoStream" => RpcResult::ServerStream(Ok(comma_separated_parts(&message.payload))),
            "StreamAndHold" => {
                RpcResult::ServerStream(Ok(one_message_then_never_completes(&message.payload)))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "EchoService has no unary method {other}"
            )))),
        }
    }

    async fn handle_rpc_stream(
        &self,
        _service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        match method {
            "Collect" => {
                let payloads: Vec<Vec<u8>> = messages.iter().map(|m| m.payload.clone()).collect();
                RpcResult::Unary(Ok(payloads.join(&b'|')))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "EchoService has no client-streaming method {other}"
            )))),
        }
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        if method != "EchoEach" {
            return Err(Status::unimplemented(format!(
                "EchoService has no bidi method {method}"
            )));
        }
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            while let Some(message) = input_rx.recv().await {
                if tx
                    .send(Ok(message.payload.to_ascii_uppercase()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(rx),
        })
    }
}

fn comma_separated_parts(payload: &[u8]) -> mpsc::Receiver<Result<Vec<u8>, Status>> {
    let parts: Vec<Vec<u8>> = payload.split(|b| *b == b',').map(|p| p.to_vec()).collect();
    let (tx, rx) = mpsc::channel(parts.len().max(1));
    tokio::spawn(async move {
        for part in parts {
            if tx.send(Ok(part)).await.is_err() {
                return;
            }
        }
    });
    rx
}

/// Emits `payload` once and then holds the sender forever, so the only thing that can end this
/// stream is the host releasing the connection it was opened on.
fn one_message_then_never_completes(payload: &[u8]) -> mpsc::Receiver<Result<Vec<u8>, Status>> {
    let (tx, rx) = mpsc::channel(4);
    let payload = payload.to_vec();
    tokio::spawn(async move {
        let _ = tx.send(Ok(payload)).await;
        std::future::pending::<()>().await;
    });
    rx
}

// ---------------------------------------------------------------------------
// Fake sink
// ---------------------------------------------------------------------------

/// A [`FrameSink`] that queues every frame for the test to read, and reports closure so a test can
/// tell "no frame yet" apart from "this connection is over".
pub struct RecordingSink {
    /// `None` once closed, so closure is observable on the read end even while the test still
    /// holds its own handle to the sink.
    frames: std::sync::Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
}

/// The read end of a [`RecordingSink`].
pub struct SinkFrames {
    frames: mpsc::UnboundedReceiver<Vec<u8>>,
}

/// A sink to hand the host, paired with the frames it will receive.
pub fn a_recording_sink() -> (Arc<RecordingSink>, SinkFrames) {
    let (frames, rx) = mpsc::unbounded_channel();
    (
        Arc::new(RecordingSink {
            frames: std::sync::Mutex::new(Some(frames)),
        }),
        SinkFrames { frames: rx },
    )
}

impl FrameSink for RecordingSink {
    fn send(&self, frame: Vec<u8>) -> Result<(), SinkClosed> {
        let guard = self.frames.lock().expect("recording sink poisoned");
        guard
            .as_ref()
            .ok_or(SinkClosed)?
            .send(frame)
            .map_err(|_| SinkClosed)
    }

    fn close(&self) {
        self.frames.lock().expect("recording sink poisoned").take();
    }
}

/// A [`FrameSink`] whose peer is already gone: every send reports [`SinkClosed`]. Stands for the
/// window that closed, or the page that navigated away, between a call arriving and its answer.
pub struct GoneSink;

/// A sink that will never accept a frame.
pub fn a_sink_whose_peer_is_gone() -> Arc<GoneSink> {
    Arc::new(GoneSink)
}

impl FrameSink for GoneSink {
    fn send(&self, _frame: Vec<u8>) -> Result<(), SinkClosed> {
        Err(SinkClosed)
    }

    fn close(&self) {}
}

impl SinkFrames {
    /// The next response frame, decoded. Panics if the connection ends or nothing arrives.
    pub async fn next_response(&mut self) -> RpcResponse {
        let frame = tokio::time::timeout(FRAME_TIMEOUT, self.frames.recv())
            .await
            .expect("no response frame arrived")
            .expect("the connection was closed before a response frame arrived");
        envelope::decode_response(&frame).expect("response frame did not decode")
    }

    /// The next `count` response frames, decoded, in arrival order.
    pub async fn next_responses(&mut self, count: usize) -> Vec<RpcResponse> {
        let mut responses = Vec::with_capacity(count);
        for _ in 0..count {
            responses.push(self.next_response().await);
        }
        responses
    }

    /// Every stream message up to (and excluding) the closing frame, as payloads. Panics if the
    /// stream carries an error instead of completing.
    pub async fn stream_payloads_until_closed(&mut self) -> Vec<Vec<u8>> {
        let mut payloads = Vec::new();
        loop {
            let response = self.next_response().await;
            assert!(
                response.error.is_none(),
                "stream failed instead of completing: {:?}",
                response.error
            );
            if response.end_of_stream {
                assert_eq!(
                    response.response_message,
                    Vec::<u8>::new(),
                    "the closing frame of a stream carries no message"
                );
                return payloads;
            }
            payloads.push(response.response_message);
        }
    }

    /// `None` once this connection is over. Panics if a frame arrives instead.
    pub async fn closed(&mut self) -> Option<Vec<u8>> {
        tokio::time::timeout(FRAME_TIMEOUT, self.frames.recv())
            .await
            .expect("the connection neither closed nor produced a frame")
    }
}

// ---------------------------------------------------------------------------
// Request frame builder
// ---------------------------------------------------------------------------

/// A request frame for [`ECHO_SERVICE`]`/Echo`, id 1, epoch 1, payload `hello`, complete in one
/// frame. Override only what the scenario is about.
pub fn a_request_frame() -> RequestFrameBuilder {
    RequestFrameBuilder {
        request_id: 1,
        client_epoch: 1,
        service: ECHO_SERVICE.to_string(),
        method: "Echo".to_string(),
        payload: b"hello".to_vec(),
        end_of_stream: true,
        names_the_call: true,
    }
}

pub struct RequestFrameBuilder {
    request_id: i32,
    client_epoch: u32,
    service: String,
    method: String,
    payload: Vec<u8>,
    end_of_stream: bool,
    names_the_call: bool,
}

impl RequestFrameBuilder {
    pub fn with_id(mut self, request_id: i32) -> Self {
        self.request_id = request_id;
        self
    }

    pub fn with_epoch(mut self, client_epoch: u32) -> Self {
        self.client_epoch = client_epoch;
        self
    }

    pub fn calling(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    pub fn on_service(mut self, service: &str) -> Self {
        self.service = service.to_string();
        self
    }

    pub fn with_payload(mut self, payload: &[u8]) -> Self {
        self.payload = payload.to_vec();
        self
    }

    /// The first frame of a request stream: it names the call, and more frames follow.
    pub fn opening_a_request_stream(mut self) -> Self {
        self.end_of_stream = false;
        self.names_the_call = true;
        self
    }

    /// A middle frame of a request stream: only the opening frame names the call.
    pub fn continuing_a_request_stream(mut self) -> Self {
        self.end_of_stream = false;
        self.names_the_call = false;
        self
    }

    /// The terminal frame of a request stream.
    pub fn closing_a_request_stream(mut self) -> Self {
        self.end_of_stream = true;
        self.names_the_call = false;
        self
    }

    pub fn build(self) -> Vec<u8> {
        let request = RpcRequest {
            request_id: self.request_id,
            request_message: self.payload,
            call_metadata: self.names_the_call.then_some(CallMetadata {
                service: self.service,
                method: self.method,
            }),
            metadata: None,
            end_of_stream: self.end_of_stream,
            abort: false,
            sender_identity: None,
            client_epoch: self.client_epoch,
        };
        envelope::encode_request(request).expect("request frame did not encode")
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

pub trait ResponseAssertions {
    fn assert_answers(&self, request_id: i32, method: &str) -> &Self;
    fn assert_epoch(&self, client_epoch: u32) -> &Self;
    fn assert_message(&self, expected: &[u8]) -> &Self;
    fn assert_complete(&self) -> &Self;
    fn assert_error(&self, code: &str, message_fragment: &str) -> &Self;
}

impl ResponseAssertions for RpcResponse {
    fn assert_answers(&self, request_id: i32, method: &str) -> &Self {
        assert_eq!(self.request_id, request_id, "response answers another id");
        assert_eq!(
            self.call_metadata,
            Some(CallMetadata {
                service: ECHO_SERVICE.to_string(),
                method: method.to_string(),
            }),
            "response does not name the call it answers"
        );
        self
    }

    fn assert_epoch(&self, client_epoch: u32) -> &Self {
        assert_eq!(
            self.client_epoch, client_epoch,
            "response carries another page's epoch"
        );
        self
    }

    fn assert_message(&self, expected: &[u8]) -> &Self {
        assert_eq!(self.response_message, expected, "response message mismatch");
        self
    }

    fn assert_complete(&self) -> &Self {
        assert!(
            self.end_of_stream,
            "expected the call to be complete, but the response was not terminal"
        );
        assert_eq!(self.error, None, "expected a successful response");
        self
    }

    fn assert_error(&self, code: &str, message_fragment: &str) -> &Self {
        let error = self.error.as_ref().unwrap_or_else(|| {
            panic!(
                "expected an error response, got {:?}",
                self.response_message
            )
        });
        assert_eq!(error.code, code, "error code mismatch");
        assert!(
            error.message.contains(message_fragment),
            "expected error message to contain '{message_fragment}', was '{}'",
            error.message
        );
        self
    }
}

/// A sink for a connection no call will ever reach: the read end is dropped on the way out, so
/// there is nothing to observe on it and the first frame published to it would report
/// [`SinkClosed`]. For the tests that need a connection to *exist* — to occupy an epoch, or to be
/// counted — and never call it, where a "recording" sink nobody kept the recording of would only
/// mislead.
pub fn a_sink_nobody_reads() -> Arc<RecordingSink> {
    a_recording_sink().0
}

// ---------------------------------------------------------------------------
// A channel the page stopped reading
// ---------------------------------------------------------------------------

/// A [`FrameSink`] belonging to a page that has stopped taking frames off one of its channels: the
/// first frame published to it parks the connection's drain task there, and it stays parked until
/// the test says the peer is gone.
///
/// [`RecordingSink`] cannot stand in for this. Its queue is unbounded, so it accepts every frame a
/// host can produce and applies no backpressure at all — which is the one thing a channel nobody is
/// reading is about. The park is a genuinely blocked thread, because [`FrameSink::send`] is
/// synchronous and there is nothing to await; `block_in_place` is what tells the runtime so, and it
/// moves the rest of that worker's tasks elsewhere instead of stalling them behind this one. Tests
/// using this sink therefore need `#[tokio::test(flavor = "multi_thread")]`.
pub struct StalledSink {
    /// Fires as the first frame arrives, before parking, so a test can wait for the channel to be
    /// genuinely stuck rather than merely idle.
    stuck: std::sync::Mutex<Option<oneshot::Sender<()>>>,
    /// Unparks when the test drops its end: from then on this page is gone.
    peer_gone: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    /// Fires when the sink itself is dropped — which the drain task does only on its way out, after
    /// it has decided what a departed connection releases.
    departure_handled: Option<oneshot::Sender<()>>,
}

/// The test's end of a [`StalledSink`]: it decides when the page's departure becomes visible, and
/// can wait for the host to have finished acting on it.
pub struct StalledPage {
    stuck: Option<oneshot::Receiver<()>>,
    peer_gone: std::sync::mpsc::Sender<()>,
    departure_handled: oneshot::Receiver<()>,
}

/// A sink whose page has stopped reading, paired with the test's control over its departure.
pub fn a_sink_the_page_stopped_reading() -> (Arc<StalledSink>, StalledPage) {
    let (stuck_tx, stuck_rx) = oneshot::channel();
    let (peer_gone_tx, peer_gone_rx) = std::sync::mpsc::channel();
    let (departure_tx, departure_rx) = oneshot::channel();
    (
        Arc::new(StalledSink {
            stuck: std::sync::Mutex::new(Some(stuck_tx)),
            peer_gone: std::sync::Mutex::new(peer_gone_rx),
            departure_handled: Some(departure_tx),
        }),
        StalledPage {
            stuck: Some(stuck_rx),
            peer_gone: peer_gone_tx,
            departure_handled: departure_rx,
        },
    )
}

impl FrameSink for StalledSink {
    fn send(&self, _frame: Vec<u8>) -> Result<(), SinkClosed> {
        if let Some(stuck) = self.stuck.lock().expect("stalled sink poisoned").take() {
            let _ = stuck.send(());
        }
        let peer_gone = self.peer_gone.lock().expect("stalled sink poisoned");
        let _ = tokio::task::block_in_place(|| peer_gone.recv());
        // Reached only once the test let go of its end: a page that never comes back for its frames
        // is, in the end, a page that is gone.
        Err(SinkClosed)
    }

    fn close(&self) {
        // Deliberately nothing. The host closing this channel must not unpark it: when a departure
        // becomes visible is exactly what the tests using this sink are pinning, so only the test
        // gets to decide it.
    }
}

impl Drop for StalledSink {
    fn drop(&mut self) {
        if let Some(departure_handled) = self.departure_handled.take() {
            let _ = departure_handled.send(());
        }
    }
}

impl StalledPage {
    /// Resolves once a frame is stuck in this channel — the host has published to it and parked
    /// there. Until then the connection is merely idle, and releasing it would prove nothing.
    pub async fn once_a_frame_is_stuck_in_the_channel(&mut self) {
        let stuck = self
            .stuck
            .take()
            .expect("a channel is waited on for its first frame once");
        tokio::time::timeout(FRAME_TIMEOUT, stuck)
            .await
            .expect("nothing was ever published to the stalled channel")
            .expect("the stalled sink went away before anything was published to it");
    }

    /// Let the departure surface, and resolve once the host has finished acting on it.
    ///
    /// The sink is dropped by the drain task on its way out, which is strictly after that task has
    /// decided what a departed connection takes with it — so this is a happens-before, not a wait
    /// for a plausible amount of time. That also means the test must have handed its own `Arc` to
    /// the host and kept none: while anything else holds the sink, the drain task letting go of it
    /// is not observable.
    pub async fn once_the_host_has_handled_the_departure(self) {
        let StalledPage {
            peer_gone,
            departure_handled,
            ..
        } = self;
        drop(peer_gone);
        tokio::time::timeout(FRAME_TIMEOUT, departure_handled)
            .await
            .expect("the host never finished with the departed connection")
            .expect("the departed connection's sink was never dropped");
    }
}
