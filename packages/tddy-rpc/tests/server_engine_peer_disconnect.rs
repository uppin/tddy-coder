//! `ServerEngine` must release everything it holds for a peer once that peer is gone.
//!
//! Nothing today tells the engine a caller has left. `RoomEvent::ParticipantDisconnected` only logs
//! (`tddy-livekit/src/participant.rs`), so a departed peer keeps its live bidi sessions, its
//! half-accumulated client-streaming calls, and — the costly one — its in-flight streaming
//! forwards, which go on pumping items into the outgoing channel for a client that will never read
//! them. A long-lived `StreamTerminalOutput` does that for as long as its PTY produces output.
//!
//! Attribution (`client_epoch`) already stops those frames being *delivered* to a later connection
//! that reused the request id, so this is a resource-lifetime contract rather than a correctness
//! one: the frames are now harmless, but they are still produced, forwarded and published forever.
//!
//! The contract these tests pin: **when `on_peer_disconnected` returns, nothing further will be
//! published for that peer, and nothing is retained for it.** Returning only once the in-flight
//! forwards are actually finished is what lets a caller — and these tests — know teardown happened,
//! instead of waiting to see whether another frame shows up.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::envelope::{CallMetadata, RpcRequest, RpcResponse};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage, RpcResult, RpcService, Status};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

const A_CLIENT_EPOCH: u32 = 0x5f3a_91c2;
const THE_DEPARTING_PEER: &str = "web-alice";
const A_STAYING_PEER: &str = "web-bob";

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

/// A server-streaming service whose frames are produced by the test.
///
/// `handle_rpc` hands the engine the receiver registered for the request's payload, so each call's
/// items are under the test's control. That also makes the teardown observable without waiting on a
/// clock: once the engine's forwarding task is gone it drops its receiver, so the test's next send
/// fails rather than the test having to watch for a frame that never comes.
struct TestDrivenStreams {
    channels: Mutex<HashMap<Vec<u8>, mpsc::Receiver<StreamItem>>>,
}

/// One frame of a server-streaming response, as the bridge hands it to the engine.
type StreamItem = Result<Vec<u8>, Status>;

impl TestDrivenStreams {
    fn new() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
        }
    }

    /// Register the stream that answers a call carrying `payload`, and return the sender that
    /// feeds it.
    fn register(&self, payload: &[u8]) -> mpsc::Sender<StreamItem> {
        let (tx, rx) = mpsc::channel(8);
        self.channels
            .lock()
            .expect("channels mutex poisoned")
            .insert(payload.to_vec(), rx);
        tx
    }
}

#[async_trait]
impl RpcService for TestDrivenStreams {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        let rx = self
            .channels
            .lock()
            .expect("channels mutex poisoned")
            .remove(&message.payload)
            .expect("a stream must be registered for this call's payload");
        RpcResult::ServerStream(Ok(rx))
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        Err(Status::internal("not used by these tests"))
    }
}

/// A bidi service that reports when its handler is running, and when its input channel closes —
/// the latter is how a handler learns its caller is gone and stops working on its behalf.
struct InputClosedReporter {
    started_tx: Mutex<Option<oneshot::Sender<()>>>,
    closed_tx: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait]
impl RpcService for InputClosedReporter {
    fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool {
        true
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::internal("not used by these tests")))
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        let closed_tx = self
            .closed_tx
            .lock()
            .expect("closed_tx mutex poisoned")
            .take()
            .expect("one bidi call per stub");
        let started_tx = self
            .started_tx
            .lock()
            .expect("started_tx mutex poisoned")
            .take()
            .expect("one bidi call per stub");
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let _ = started_tx.send(());
            while input_rx.recv().await.is_some() {}
            let _ = closed_tx.send(());
            drop(tx);
        });
        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(rx),
        })
    }
}

/// Concatenates every message of a multi-message call, so a dispatch reveals exactly which
/// fragments the engine was still holding.
struct ConcatStub;

#[async_trait]
impl RpcService for ConcatStub {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(message.payload.clone()))
    }

    async fn handle_rpc_stream(
        &self,
        _service: &str,
        _method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        RpcResult::Unary(Ok(messages
            .iter()
            .flat_map(|m| m.payload.clone())
            .collect()))
    }
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

fn an_opening_request(
    request_id: i32,
    method: &str,
    payload: &[u8],
    end_of_stream: bool,
) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: Some(CallMetadata {
            service: "connection.ConnectionService".to_string(),
            method: method.to_string(),
        }),
        metadata: None,
        end_of_stream,
        abort: false,
        sender_identity: None,
        client_epoch: A_CLIENT_EPOCH,
    }
}

/// A continuation of an already-open call: no `call_metadata`, only the opening frame carries it.
fn a_continuation(request_id: i32, payload: &[u8], end_of_stream: bool) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: None,
        metadata: None,
        end_of_stream,
        abort: false,
        sender_identity: None,
        client_epoch: A_CLIENT_EPOCH,
    }
}

/// Waits until the bidi handler is actually running. `ServerEngine` starts a handler in a background
/// task, so a session is open but not yet live when `on_request` returns — a handler that has not
/// started holds no input to close.
async fn a_running_bidi_handler(started: oneshot::Receiver<()>) {
    timeout(Duration::from_secs(1), started)
        .await
        .expect("the bidi handler must start within 1s")
        .expect("the start signal must not be dropped");
}

async fn next_response(rx: &mut mpsc::Receiver<(String, RpcResponse)>) -> (String, RpcResponse) {
    timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("a response must be published within 1s")
        .expect("the outgoing channel must not close")
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn stops_forwarding_a_streaming_response_once_its_peer_disconnects() {
    // Given — a streaming call in flight for a peer, with one frame already forwarded
    let service = TestDrivenStreams::new();
    let frames = service.register(b"terminal-output");
    let engine = ServerEngine::new(service);
    let (tx, mut rx) = mpsc::channel(8);
    engine
        .on_request(
            THE_DEPARTING_PEER,
            an_opening_request(3, "StreamTerminalOutput", b"terminal-output", true),
            tx,
        )
        .await;
    frames
        .send(Ok(b"first".to_vec()))
        .await
        .expect("the forwarder must accept a frame");
    let (_peer, first) = next_response(&mut rx).await;
    assert_eq!(first.response_message, b"first");

    // When — the peer goes away
    engine.on_peer_disconnected(THE_DEPARTING_PEER).await;

    // Then — the forward is finished, so the producer has nowhere left to send. A PTY that keeps
    // producing must not keep a forwarder alive for a client that will never read it.
    assert!(
        frames.send(Ok(b"second".to_vec())).await.is_err(),
        "the forwarding task must be gone once on_peer_disconnected returns"
    );
}

#[tokio::test]
async fn leaves_another_peers_streaming_response_running() {
    // Given — two peers each with a streaming call in flight
    let service = TestDrivenStreams::new();
    // Held, not dropped: dropping it would end Alice's stream and publish a closing frame, which
    // would reach the outgoing channel ahead of Bob's and make this test about frame ordering.
    let _departing_frames = service.register(b"alices-terminal");
    let staying_frames = service.register(b"bobs-terminal");
    let engine = ServerEngine::new(service);
    let (tx, mut rx) = mpsc::channel(8);
    engine
        .on_request(
            THE_DEPARTING_PEER,
            an_opening_request(3, "StreamTerminalOutput", b"alices-terminal", true),
            tx.clone(),
        )
        .await;
    engine
        .on_request(
            A_STAYING_PEER,
            an_opening_request(3, "StreamTerminalOutput", b"bobs-terminal", true),
            tx,
        )
        .await;

    // When — only one of them disconnects
    engine.on_peer_disconnected(THE_DEPARTING_PEER).await;

    // Then — the other peer's stream still delivers. Request ids are only unique per peer, so
    // tearing down by id alone would take this one with it.
    staying_frames
        .send(Ok(b"bob-still-watching".to_vec()))
        .await
        .expect("the staying peer's forwarder must still be running");
    let (peer, response) = next_response(&mut rx).await;
    assert_eq!(peer, A_STAYING_PEER);
    assert_eq!(response.response_message, b"bob-still-watching");
}

#[tokio::test]
async fn closes_the_input_of_a_departed_peers_bidi_handler() {
    // Given — a live bidi session for a peer, its handler up and reading its input
    let (started_tx, started) = oneshot::channel();
    let (closed_tx, closed) = oneshot::channel();
    let engine = ServerEngine::new(InputClosedReporter {
        started_tx: Mutex::new(Some(started_tx)),
        closed_tx: Mutex::new(Some(closed_tx)),
    });
    let (tx, _rx) = mpsc::channel(8);
    engine
        .on_request(
            THE_DEPARTING_PEER,
            an_opening_request(7, "StreamSessionTerminalIO", b"open", false),
            tx,
        )
        .await;
    a_running_bidi_handler(started).await;

    // When — the peer goes away
    engine.on_peer_disconnected(THE_DEPARTING_PEER).await;

    // Then — the handler's input closes, which is how it learns to stop
    timeout(Duration::from_secs(1), closed)
        .await
        .expect("the bidi handler's input must close within 1s")
        .expect("the close signal must not be dropped");
}

#[tokio::test]
async fn discards_the_fragments_of_a_departed_peers_half_sent_call() {
    // Given — a client-streaming call that has sent two fragments and not yet terminated
    let engine = ServerEngine::new(ConcatStub);
    let (tx, mut rx) = mpsc::channel(8);
    engine
        .on_request(
            THE_DEPARTING_PEER,
            an_opening_request(9, "UploadAttachment", b"fragment-a", false),
            tx.clone(),
        )
        .await;
    engine
        .on_request(
            THE_DEPARTING_PEER,
            a_continuation(9, b"fragment-b", false),
            tx.clone(),
        )
        .await;

    // When — the peer goes away, and a later caller reuses the same request id
    engine.on_peer_disconnected(THE_DEPARTING_PEER).await;
    engine
        .on_request(
            THE_DEPARTING_PEER,
            an_opening_request(9, "UploadAttachment", b"fragment-c", true),
            tx,
        )
        .await;

    // Then — the abandoned fragments are gone, not silently prepended to the new call
    let (_peer, response) = next_response(&mut rx).await;
    assert_eq!(response.response_message, b"fragment-c");
}
