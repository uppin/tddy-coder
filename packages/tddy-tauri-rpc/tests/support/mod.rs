//! Test support for the webview-IPC flavour: a service to call, a fake sink to observe, a builder
//! for request frames, and domain assertions on response frames.

#![allow(dead_code)] // each test binary uses a subset of these helpers.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::envelope::{self, CallMetadata, RpcRequest, RpcResponse};
use tddy_rpc::{
    BidiStreamOutput, MultiRpcService, ResponseBody, RpcMessage, RpcResult, RpcService,
    ServiceEntry, Status,
};
use tddy_tauri_rpc::{FrameSink, SinkClosed, WebviewRpcHost};
use tokio::sync::mpsc;

/// The service every test calls.
pub const ECHO_SERVICE: &str = "test.EchoService";

/// Safety net around awaits that are driven entirely by channels, not by polling — so a failure
/// reports which frame never arrived instead of hanging the suite. Not an expected duration: the
/// frames these tests wait for are produced in-process, in microseconds.
const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// Host under test
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
