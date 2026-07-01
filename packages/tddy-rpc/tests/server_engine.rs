//! `ServerEngine<S>` is the transport-agnostic half of what `tddy_livekit::participant` does
//! today: routes decoded `RpcRequest`s into an `RpcBridge<S>` and multiplexes concurrent
//! request/stream/bidi state by `(peer, request_id)` — a peer identifier is required because
//! request ids are only unique per-peer, not globally (mirrors `participant.rs`'s `SessionKey`).
//! Fails to compile until `tddy_rpc::server_engine` exists. See
//! `docs/dev/1-WIP/rpc-multi-transport.md`.

use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::envelope::{CallMetadata, RpcRequest};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::{BidiStreamOutput, ResponseBody, RpcMessage, RpcResult, RpcService, Status};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Echoes the request payload back unchanged; `EchoBidi` also echoes each incoming bidi message
/// as it arrives. A fake, not a mock — real (if trivial) behavior, per fluent-tests guidelines.
struct EchoStub;

#[async_trait]
impl RpcService for EchoStub {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "test.EchoService" && method == "EchoBidi"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(message.payload.clone()))
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(message) = input_rx.recv().await {
                if tx.send(Ok(message.payload)).await.is_err() {
                    break;
                }
            }
        });
        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(rx),
        })
    }
}

/// A server-streaming service whose handler sends exactly one item and then holds the response
/// channel open forever without sending a second one — simulating a live stream where the next
/// item genuinely isn't known yet (as opposed to a stream that's simply finished).
struct StreamStub;

#[async_trait]
impl RpcService for StreamStub {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        let (tx, rx) = mpsc::channel(1);
        let payload = message.payload.clone();
        tokio::spawn(async move {
            let _ = tx.send(Ok(payload)).await;
            // Never send again and never drop `tx` — the channel stays open, so a correct,
            // real-time forwarder must not need to see a second item before delivering the first.
            std::future::pending::<()>().await;
        });
        RpcResult::ServerStream(Ok(rx))
    }
}

/// A non-bidi service whose multi-message handler concatenates every message it was dispatched
/// with in one call — proves whether a client-streaming call's messages were collected together
/// before dispatch, or whether the engine dispatched prematurely on the first fragment.
struct ConcatStub;

#[async_trait]
impl RpcService for ConcatStub {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        // Reached only if the engine (incorrectly) dispatches on a single fragment of a
        // multi-message call instead of collecting all of them first.
        RpcResult::Unary(Ok(message.payload.clone()))
    }

    async fn handle_rpc_stream(
        &self,
        _service: &str,
        _method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        let concatenated = messages.iter().flat_map(|m| m.payload.clone()).collect();
        RpcResult::Unary(Ok(concatenated))
    }
}

fn a_streaming_request(request_id: i32, payload: &[u8]) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: Some(CallMetadata {
            service: "test.EchoService".to_string(),
            method: "EchoStream".to_string(),
        }),
        metadata: None,
        end_of_stream: true,
        abort: false,
        sender_identity: None,
    }
}

/// Opens a non-bidi, multi-message (client-streaming) call: carries `call_metadata` but is not
/// the end of the call — a continuation is expected to follow.
fn a_client_stream_open_request(request_id: i32, payload: &[u8]) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: Some(CallMetadata {
            service: "test.ConcatService".to_string(),
            method: "ConcatMessages".to_string(),
        }),
        metadata: None,
        end_of_stream: false,
        abort: false,
        sender_identity: None,
    }
}

fn a_unary_request(request_id: i32, payload: &[u8]) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: Some(CallMetadata {
            service: "test.EchoService".to_string(),
            method: "Echo".to_string(),
        }),
        metadata: None,
        end_of_stream: true,
        abort: false,
        sender_identity: None,
    }
}

fn a_bidi_open_request(request_id: i32, payload: &[u8]) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: Some(CallMetadata {
            service: "test.EchoService".to_string(),
            method: "EchoBidi".to_string(),
        }),
        metadata: None,
        end_of_stream: false,
        abort: false,
        sender_identity: None,
    }
}

/// A continuation message for an already-open bidi session omits `call_metadata` — the engine
/// must still route it by `(peer, request_id)` to the live session opened earlier.
fn a_bidi_continuation_request(request_id: i32, payload: &[u8], end_of_stream: bool) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: payload.to_vec(),
        call_metadata: None,
        metadata: None,
        end_of_stream,
        abort: false,
        sender_identity: None,
    }
}

#[tokio::test]
async fn routes_a_unary_request_to_the_bridge_and_publishes_one_response() {
    // Given a server engine wrapping the echo stub
    let engine = ServerEngine::new(EchoStub);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);

    // When handling one unary request from a peer
    engine
        .on_request("peer-a", a_unary_request(1, b"hi"), outgoing_tx)
        .await;

    // Then exactly one response is published, addressed to that peer, echoing the payload
    let (peer, response) = outgoing_rx.recv().await.expect("no response published");
    assert_eq!(peer, "peer-a");
    assert_eq!(response.request_id, 1);
    assert_eq!(response.response_message, b"hi");
    assert!(response.end_of_stream);
    assert!(
        outgoing_rx.try_recv().is_err(),
        "expected exactly one response"
    );
}

#[tokio::test]
async fn multiplexes_the_same_request_id_from_two_different_peers_independently() {
    // Given a server engine and two peers that happen to both use request_id 1 (request ids are
    // only unique per-peer, mirroring independent RpcClient instances)
    let engine = ServerEngine::new(EchoStub);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);

    // When each peer sends its own request with the same request_id but a distinct payload
    engine
        .on_request("peer-a", a_unary_request(1, b"from-a"), outgoing_tx.clone())
        .await;
    engine
        .on_request("peer-b", a_unary_request(1, b"from-b"), outgoing_tx.clone())
        .await;

    // Then each peer receives its own response, with no cross-talk between them
    let first = outgoing_rx.recv().await.expect("first response missing");
    let second = outgoing_rx.recv().await.expect("second response missing");
    let by_peer: std::collections::HashMap<String, Vec<u8>> = [first, second]
        .into_iter()
        .map(|(peer, response)| (peer, response.response_message))
        .collect();
    assert_eq!(by_peer["peer-a"], b"from-a");
    assert_eq!(by_peer["peer-b"], b"from-b");
}

#[tokio::test]
async fn pairs_a_bidi_continuation_message_without_call_metadata_to_its_live_session() {
    // Given a server engine hosting the bidi-capable echo stub
    let engine = ServerEngine::new(EchoStub);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);

    // When the first message opens the bidi session (carrying call_metadata)...
    engine
        .on_request(
            "peer-a",
            a_bidi_open_request(7, b"first"),
            outgoing_tx.clone(),
        )
        .await;
    // ...and a continuation message arrives without call_metadata, ending the stream
    engine
        .on_request(
            "peer-a",
            a_bidi_continuation_request(7, b"second", true),
            outgoing_tx.clone(),
        )
        .await;

    // Then both messages were routed to the same live session and echoed in order. Neither data
    // frame carries end_of_stream — a bidi producer may only emit its next item after the peer
    // reacts to the current one, so looking ahead to tag the last *data* item would deadlock a
    // truly interactive stream. A separate empty frame signals closure once the stream ends.
    let first = outgoing_rx.recv().await.expect("first echo missing");
    let second = outgoing_rx.recv().await.expect("second echo missing");
    let closing = outgoing_rx.recv().await.expect("closing frame missing");
    assert_eq!(first.1.response_message, b"first");
    assert!(!first.1.end_of_stream);
    assert_eq!(second.1.response_message, b"second");
    assert!(!second.1.end_of_stream);
    assert!(closing.1.response_message.is_empty());
    assert!(closing.1.end_of_stream);
}

#[tokio::test]
async fn forwards_each_server_streaming_item_immediately_without_waiting_for_the_next_one() {
    // Given a server-streaming handler that sends one item and then never sends (or closes)
    // again — the next item genuinely isn't known yet
    let engine = ServerEngine::new(StreamStub);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);

    // When handling the streaming request
    engine
        .on_request("peer-a", a_streaming_request(1, b"chunk"), outgoing_tx)
        .await;

    // Then the first item is published right away — it must not be held back waiting to see
    // whether a second item exists
    let (peer, response) = timeout(Duration::from_millis(200), outgoing_rx.recv())
        .await
        .expect("first streamed item should arrive immediately, not be held back waiting for a next item")
        .expect("no response published");
    assert_eq!(peer, "peer-a");
    assert_eq!(response.response_message, b"chunk");
}

#[tokio::test]
async fn collects_all_messages_of_a_multi_message_call_before_dispatching_once() {
    // Given a server engine hosting a stub that concatenates every message it's dispatched with
    let engine = ServerEngine::new(ConcatStub);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);

    // When a client-streaming call arrives as two frames: the first carries call_metadata and is
    // not the end of the call, the second is a continuation (no call_metadata) that ends it
    engine
        .on_request(
            "peer-a",
            a_client_stream_open_request(3, b"first-"),
            outgoing_tx.clone(),
        )
        .await;
    engine
        .on_request(
            "peer-a",
            a_bidi_continuation_request(3, b"second", true),
            outgoing_tx.clone(),
        )
        .await;

    // Then the handler was dispatched exactly once, with both messages collected together in
    // order — not once per fragment
    let (peer, response) = outgoing_rx.recv().await.expect("no response published");
    assert_eq!(peer, "peer-a");
    assert_eq!(response.response_message, b"first-second");
    assert!(
        outgoing_rx.try_recv().is_err(),
        "expected exactly one response, not one per fragment"
    );
}
