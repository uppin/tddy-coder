//! `ClientEngine` is the transport-agnostic half of what `tddy_livekit::client::RpcClient` does
//! today: request-id correlation for pending unary calls and pending streams, independent of how
//! bytes actually reach the peer. Fails to compile until `tddy_rpc::client_engine` exists. See
//! `docs/dev/1-WIP/rpc-multi-transport.md`.

use std::sync::Arc;

use tddy_rpc::client_engine::ClientEngine;
use tddy_rpc::envelope::RpcResponse;

fn a_response(request_id: i32, payload: &[u8], end_of_stream: bool) -> RpcResponse {
    RpcResponse {
        request_id,
        response_message: payload.to_vec(),
        metadata: None,
        end_of_stream,
        error: None,
        trailers: None,
    }
}

/// The marker a real-time streaming server sends purely to signal that a stream has ended, when
/// it can't tag the last real item directly (the item was already forwarded before the server
/// knew it was last). Carries no data and no error — just closure.
fn a_closing_signal(request_id: i32) -> RpcResponse {
    a_response(request_id, b"", true)
}

#[test]
fn assigns_increasing_request_ids_starting_at_one() {
    // Given a fresh engine
    let engine = ClientEngine::new("client-1");

    // When beginning two unary calls
    let (first, _first_rx) = engine.begin_unary("test.EchoService", "Echo", b"a".to_vec());
    let (second, _second_rx) = engine.begin_unary("test.EchoService", "Echo", b"b".to_vec());

    // Then request ids increase monotonically, starting at 1
    assert_eq!(first.request_id, 1);
    assert_eq!(second.request_id, 2);
}

#[tokio::test]
async fn resolves_a_pending_unary_call_when_its_response_arrives() {
    // Given a unary call in flight
    let engine = ClientEngine::new("client-1");
    let (request, rx) = engine.begin_unary("test.EchoService", "Echo", b"hello".to_vec());

    // When the matching response arrives
    engine
        .on_response(a_response(request.request_id, b"hello", true))
        .await;

    // Then the caller's receiver resolves with the response payload
    let result = rx.await.expect("oneshot sender dropped without a response");
    assert_eq!(result.expect("unary call failed"), b"hello");
}

#[tokio::test]
async fn closes_a_pending_stream_after_end_of_stream() {
    // Given a streaming call in flight
    let engine = ClientEngine::new("client-1");
    let (request, mut rx) = engine.begin_stream("test.EchoService", "EchoStream", b"AB".to_vec());

    // When two chunks arrive, the second marked end_of_stream
    engine
        .on_response(a_response(request.request_id, b"chunk-1", false))
        .await;
    engine
        .on_response(a_response(request.request_id, b"chunk-2", true))
        .await;

    // Then both chunks are delivered in order, and the channel closes after end_of_stream
    let first = rx.recv().await.expect("first chunk missing");
    let second = rx.recv().await.expect("second chunk missing");
    let closed = rx.recv().await;

    assert_eq!(first.expect("first chunk error"), b"chunk-1");
    assert_eq!(second.expect("second chunk error"), b"chunk-2");
    assert!(
        closed.is_none(),
        "stream should be closed after end_of_stream"
    );
}

#[tokio::test]
async fn closes_a_pending_stream_without_forwarding_a_payload_free_closing_signal() {
    // Given a streaming call in flight with one real chunk already delivered
    let engine = ClientEngine::new("client-1");
    let (request, mut rx) = engine.begin_stream("test.EchoService", "EchoStream", b"AB".to_vec());
    engine
        .on_response(a_response(request.request_id, b"chunk-1", false))
        .await;

    // When a payload-free closing signal arrives — sent because a real-time forwarder can't know
    // a data item is the last one until after it's already been sent, so closure is signaled
    // separately rather than by tagging a real item
    engine
        .on_response(a_closing_signal(request.request_id))
        .await;

    // Then the real chunk is delivered, but the closing signal itself is not treated as data —
    // it only closes the stream
    let first = rx.recv().await.expect("first chunk missing");
    let closed = rx.recv().await;

    assert_eq!(first.expect("first chunk error"), b"chunk-1");
    assert!(
        closed.is_none(),
        "closing signal should close the stream, not be delivered as an extra empty item"
    );
}

#[tokio::test]
async fn delivers_every_stream_item_even_when_the_consumer_drains_after_a_large_burst() {
    // Given a streaming call in flight, and a producer that pushes more items than the stream
    // channel's internal capacity before this test starts draining — a legitimate pattern (a
    // caller may send a whole burst of requests before reading any responses) that silently lost
    // data when delivery used a non-blocking, drop-on-full send instead of backpressured send.
    let engine = Arc::new(ClientEngine::new("client-1"));
    let (request, mut rx) = engine.begin_stream("test.EchoService", "EchoStream", b"go".to_vec());
    const N: usize = 40; // exceeds the internal channel capacity (32)

    let producer_engine = engine.clone();
    let request_id = request.request_id;
    let producer = tokio::spawn(async move {
        for i in 0..N {
            let end_of_stream = i + 1 == N;
            producer_engine
                .on_response(a_response(
                    request_id,
                    format!("item-{i}").as_bytes(),
                    end_of_stream,
                ))
                .await;
        }
    });

    // When draining only starts after the producer is already under way
    let mut received = Vec::new();
    while let Some(item) = rx.recv().await {
        received.push(item.expect("item error"));
    }
    producer.await.expect("producer task panicked");

    // Then every item arrives, in order — none silently dropped
    let expected: Vec<Vec<u8>> = (0..N).map(|i| format!("item-{i}").into_bytes()).collect();
    assert_eq!(received, expected);
}

#[tokio::test]
async fn ignores_a_response_for_an_unknown_request_id() {
    // Given an engine with no pending calls at all
    let engine = ClientEngine::new("client-1");

    // When a response arrives for a request_id nothing registered (e.g. a duplicate delivery
    // after the pending entry was already removed)
    // Then it is dropped silently — no panic, no pending entry to corrupt
    engine.on_response(a_response(999, b"unexpected", true)).await;
}
