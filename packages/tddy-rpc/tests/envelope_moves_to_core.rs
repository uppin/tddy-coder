//! The wire envelope (`RpcRequest`/`RpcResponse`) moves from `tddy-livekit` into `tddy-rpc` so
//! every transport (LiveKit, stdio, future ones) shares one definition. This test targets the
//! new location — it fails to compile until `tddy_rpc::envelope` exists.
//! See `docs/dev/1-WIP/rpc-multi-transport.md`.

use tddy_rpc::envelope::{decode_request, encode_request, CallMetadata, RpcRequest};

#[test]
fn rpc_request_round_trips_through_encode_and_decode() {
    // Given an RpcRequest with representative fields set, including the transport-neutral
    // sender_identity used for reply addressing
    let request = RpcRequest {
        request_id: 42,
        request_message: b"hello".to_vec(),
        call_metadata: Some(CallMetadata {
            service: "test.EchoService".to_string(),
            method: "Echo".to_string(),
        }),
        metadata: None,
        end_of_stream: true,
        abort: false,
        sender_identity: Some("client-1".to_string()),
    };

    // When encoding then decoding
    let encoded = encode_request(request.clone()).expect("encode");
    let decoded = decode_request(&encoded).expect("decode");

    // Then every field survives the round trip byte-perfectly
    assert_eq!(decoded.request_id, request.request_id);
    assert_eq!(decoded.request_message, request.request_message);
    assert_eq!(
        decoded.call_metadata.as_ref().unwrap().service,
        "test.EchoService"
    );
    assert_eq!(decoded.call_metadata.as_ref().unwrap().method, "Echo");
    assert_eq!(decoded.end_of_stream, request.end_of_stream);
    assert_eq!(decoded.sender_identity, request.sender_identity);
}
