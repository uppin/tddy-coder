//! ConnectRPC acceptance tests.
//!
//! These tests verify the Connect protocol implementation by making HTTP requests
//! to the connect_router and asserting on responses. They are written first (TDD)
//! and fail until the implementation is complete.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use prost::Message;
use tddy_connectrpc::connect_router;
use tddy_service::create_echo_bridge;
use tddy_service::proto::test::{EchoRequest, EchoResponse};
use tower::ServiceExt;

/// Unary RPC with protobuf binary: POST /rpc/test.EchoService/Echo
/// Expect: 200 OK, Content-Type: application/proto, body = protobuf EchoResponse
#[tokio::test]
async fn unary_proto_echo_returns_200_with_echoed_message() {
    let app = connect_router(create_echo_bridge());

    let req = EchoRequest {
        message: "hello".to_string(),
    };
    let body_bytes = req.encode_to_vec();

    let request = Request::builder()
        .method("POST")
        .uri("/rpc/test.EchoService/Echo")
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(body_bytes))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK for unary Echo"
    );
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/proto"),
        "Response Content-Type should be application/proto"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp = EchoResponse::decode(&body[..]).expect("decode response");
    assert_eq!(resp.message, "hello");
}

/// Error format: unknown method returns Connect error JSON with code "not_found"
#[tokio::test]
async fn unknown_method_returns_connect_error_json() {
    let app = connect_router(create_echo_bridge());

    let request = Request::builder()
        .method("POST")
        .uri("/rpc/test.EchoService/UnknownMethod")
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(vec![]))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Unknown method should return 404"
    );
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "Error response Content-Type should be application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("parse error JSON");
    assert_eq!(json["code"], "not_found");
}

/// Server streaming: EchoServerStream returns envelope-framed stream
#[tokio::test]
async fn server_stream_returns_envelope_framed_messages() {
    let app = connect_router(create_echo_bridge());

    let req = EchoRequest {
        message: "hi".to_string(),
    };
    let mut body = Vec::new();
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_envelope(
        &req.encode_to_vec(),
        false,
    ));
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_end_stream(b"{}"));

    let request = Request::builder()
        .method("POST")
        .uri("/rpc/test.EchoService/EchoServerStream")
        .header("Content-Type", "application/connect+proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Server stream should return 200"
    );
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/connect+proto"),
        "Streaming response Content-Type"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    // Envelope format: [flags:1][length:4be][payload] per message, then end-stream frame
    assert!(
        body.len() >= 5,
        "Response should have at least one envelope (5-byte header)"
    );
}

/// Client streaming: EchoClientStream receives envelope-framed messages, returns joined response
#[tokio::test]
async fn client_stream_echo_returns_joined_message() {
    let app = connect_router(create_echo_bridge());

    let req1 = EchoRequest {
        message: "a".to_string(),
    };
    let req2 = EchoRequest {
        message: "b".to_string(),
    };
    let mut body = Vec::new();
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_envelope(
        &req1.encode_to_vec(),
        false,
    ));
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_envelope(
        &req2.encode_to_vec(),
        false,
    ));
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_end_stream(b"{}"));

    let request = Request::builder()
        .method("POST")
        .uri("/rpc/test.EchoService/EchoClientStream")
        .header("Content-Type", "application/connect+proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Client stream should return 200"
    );
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/connect+proto"),
        "Streaming response Content-Type"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let frames = tddy_connectrpc::envelope::parse_envelope_frames(&body).expect("parse frames");
    assert_eq!(frames.len(), 1, "Should have one message frame");
    let resp = EchoResponse::decode(&frames[0][..]).expect("decode response");
    assert_eq!(resp.message, "a | b", "EchoClientStream joins with |");
}

/// Bidi streaming: EchoBidiStream receives envelope-framed messages, returns stream of echoes
#[tokio::test]
async fn bidi_stream_echo_returns_stream_of_echoes() {
    let app = connect_router(create_echo_bridge());

    let req1 = EchoRequest {
        message: "x".to_string(),
    };
    let req2 = EchoRequest {
        message: "y".to_string(),
    };
    let mut body = Vec::new();
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_envelope(
        &req1.encode_to_vec(),
        false,
    ));
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_envelope(
        &req2.encode_to_vec(),
        false,
    ));
    body.extend_from_slice(&tddy_connectrpc::envelope::wrap_end_stream(b"{}"));

    let request = Request::builder()
        .method("POST")
        .uri("/rpc/test.EchoService/EchoBidiStream")
        .header("Content-Type", "application/connect+proto")
        .header("Connect-Protocol-Version", "1")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Bidi stream should return 200"
    );
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok()),
        Some("application/connect+proto"),
        "Streaming response Content-Type"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let frames = tddy_connectrpc::envelope::parse_envelope_frames(&body).expect("parse frames");
    assert_eq!(frames.len(), 2, "Should have two echo responses");
    let resp1 = EchoResponse::decode(&frames[0][..]).expect("decode first");
    let resp2 = EchoResponse::decode(&frames[1][..]).expect("decode second");
    assert_eq!(resp1.message, "x");
    assert_eq!(resp2.message, "y");
}
