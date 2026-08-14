//! Regression tests: a response minted for a previous client connection must never resolve a call
//! of the current one.
//!
//! `ClientEngine` correlates responses to pending calls by `request_id` alone, and `RpcResponse`
//! carries no call identity — unlike `RpcRequest`, which carries `call_metadata`. Request ids
//! restart at 1 whenever the id space is fresh (a browser page reload; a daemon restart), while the
//! peer keeps serving streams opened by the *previous* connection and addressed to the same
//! transport identity. Its frames then resolve whichever call now holds that id, and the payload is
//! decoded as that call's message type.
//!
//! The corruption is silent because the engine hands the caller raw `Vec<u8>`: a
//! `SessionTerminalOutput` frame delivered to a `WatchTerminalControl` stream decodes as a
//! `TerminalControlEvent` with no error at all — `data` (`bytes`, field 1) and `holder_screen_id`
//! (`string`, field 1) share a field number and a wire type.
//!
//! The fix is a per-connection `client_epoch` stamped on every request and echoed on every
//! response, with a mismatch dropped rather than delivered.

use tddy_rpc::client_engine::ClientEngine;
use tddy_rpc::envelope::{CallMetadata, RpcResponse};

/// The epoch of the connection that is open now.
const THIS_CONNECTION: u32 = 0x5f3a_91c2;
/// The epoch of the connection that went away, whose streams the peer still serves.
const THE_CLOSED_CONNECTION: u32 = 0x11ac_07e4;

/// A response frame as the peer would publish it, tagged with the connection that opened the call.
fn a_stream_frame(
    request_id: i32,
    client_epoch: u32,
    payload: &[u8],
    service: &str,
    method: &str,
) -> RpcResponse {
    RpcResponse {
        request_id,
        response_message: payload.to_vec(),
        metadata: None,
        end_of_stream: false,
        error: None,
        trailers: None,
        client_epoch,
        call_metadata: Some(CallMetadata {
            service: service.to_string(),
            method: method.to_string(),
        }),
    }
}

#[tokio::test]
async fn ignores_a_stream_frame_left_over_from_a_previous_connection() {
    // Given — a control watch open on this connection
    let engine = ClientEngine::with_client_epoch("web-alice", THIS_CONNECTION);
    let (request, mut rx) = engine.begin_stream(
        "connection.ConnectionService",
        "WatchTerminalControl",
        vec![],
    );

    // When — the peer publishes a frame of a terminal-output stream opened by the closed
    // connection, which happens to carry the same request id
    engine
        .on_response(a_stream_frame(
            request.request_id,
            THE_CLOSED_CONNECTION,
            b"\x1b[2K  6. Chat about this\r\n",
            "connection.ConnectionService",
            "StreamTerminalOutput",
        ))
        .await;

    // Then — nothing is delivered to the control watch
    assert!(
        rx.try_recv().is_err(),
        "a frame from a previous connection must not resolve a current call"
    );
}

#[tokio::test]
async fn delivers_a_stream_frame_minted_by_this_connection() {
    // Given — a control watch open on this connection
    let engine = ClientEngine::with_client_epoch("web-alice", THIS_CONNECTION);
    let (request, mut rx) = engine.begin_stream(
        "connection.ConnectionService",
        "WatchTerminalControl",
        vec![],
    );

    // When — the peer answers the call this connection actually made
    engine
        .on_response(a_stream_frame(
            request.request_id,
            THIS_CONNECTION,
            b"screen-1755100800000-k3f9qz",
            "connection.ConnectionService",
            "WatchTerminalControl",
        ))
        .await;

    // Then
    let delivered = rx
        .try_recv()
        .expect("a frame from this connection must be delivered");
    assert_eq!(
        delivered.expect("frame must not be an error"),
        b"screen-1755100800000-k3f9qz"
    );
}

#[tokio::test]
async fn ignores_a_frame_answering_a_different_method_on_the_same_request_id() {
    // Given — this connection's call is a control watch
    let engine = ClientEngine::with_client_epoch("web-alice", THIS_CONNECTION);
    let (request, mut rx) = engine.begin_stream(
        "connection.ConnectionService",
        "WatchTerminalControl",
        vec![],
    );

    // When — a frame arrives with this connection's epoch but naming another method. Same-epoch
    // crossing is not the reported failure, but now that a response names its call, refusing a
    // mismatch costs nothing and stops one message being decoded as another.
    engine
        .on_response(a_stream_frame(
            request.request_id,
            THIS_CONNECTION,
            b"\x1b[2K  6. Chat about this\r\n",
            "connection.ConnectionService",
            "StreamTerminalOutput",
        ))
        .await;

    // Then
    assert!(
        rx.try_recv().is_err(),
        "a frame answering another method must not be delivered"
    );
}

#[tokio::test]
async fn ignores_a_unary_response_left_over_from_a_previous_connection() {
    // Given — a unary claim in flight on this connection
    let engine = ClientEngine::with_client_epoch("web-alice", THIS_CONNECTION);
    let (request, rx) = engine.begin_unary(
        "connection.ConnectionService",
        "ClaimTerminalControl",
        vec![],
    );

    // When — a response from the closed connection reuses the request id
    let mut stale = a_stream_frame(
        request.request_id,
        THE_CLOSED_CONNECTION,
        b"stale",
        "connection.ConnectionService",
        "ClaimTerminalControl",
    );
    stale.end_of_stream = true;
    engine.on_response(stale).await;

    // Then — the call is still pending; the sender was not consumed by the stale response
    drop(engine);
    assert!(
        rx.await.is_err(),
        "a stale response must leave the pending call unresolved"
    );
}

#[test]
fn stamps_every_request_with_this_connections_epoch() {
    // Given
    let engine = ClientEngine::with_client_epoch("web-alice", THIS_CONNECTION);

    // When
    let (request, _rx) = engine.begin_unary("test.EchoService", "Echo", b"hi".to_vec());

    // Then — the peer can only echo an epoch the request carried
    assert_eq!(request.client_epoch, THIS_CONNECTION);
}

#[test]
fn mints_a_distinct_epoch_for_each_connection() {
    // Given / When — two engines, as two page loads would build
    let first = ClientEngine::new("web-alice");
    let second = ClientEngine::new("web-alice");

    // Then — a fresh connection never inherits the id space of the one before it. Random 32-bit
    // values, so this asserts distinctness rather than any particular pair.
    assert_ne!(
        first.client_epoch(),
        second.client_epoch(),
        "each connection must mint its own epoch"
    );
}
