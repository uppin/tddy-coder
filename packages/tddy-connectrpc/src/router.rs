//! Axum router for Connect protocol RPC endpoints.

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use futures::stream::StreamExt;
use std::sync::Arc;
use tddy_rpc::{RequestMetadata, RpcMessage};
use tokio_stream::wrappers::ReceiverStream;

use crate::envelope::{parse_envelope_frames, wrap_end_stream, wrap_envelope};
use crate::error::{code_to_connect_str, code_to_http_status, status_to_error_body};
use crate::protocol::{validate_protocol_version, RequestProtocol};

/// Create an axum Router that serves Connect protocol RPC at `/rpc/{service}/{method}`.
///
/// The router expects POST requests with:
/// - `Connect-Protocol-Version: 1` header (optional)
/// - `Content-Type: application/proto` (protobuf binary) or `application/json`
/// - Body: encoded request message
///
/// Returns responses with appropriate Content-Type and Connect error format on failure.
pub fn connect_router<S: tddy_rpc::RpcService + Send + Sync + 'static>(
    bridge: tddy_rpc::RpcBridge<S>,
) -> Router {
    let bridge = Arc::new(bridge);
    Router::new()
        .route("/rpc/{service}/{method}", post(handle_rpc::<S>))
        .with_state(bridge)
}

async fn handle_rpc<S: tddy_rpc::RpcService>(
    Path((service, method)): Path<(String, String)>,
    State(bridge): State<Arc<tddy_rpc::RpcBridge<S>>>,
    request: Request,
) -> Response {
    log::debug!("ConnectRPC {} / {}", service, method);

    if let Some(msg) = validate_protocol_version(request.headers()) {
        return error_response(
            tddy_rpc::Status::invalid_argument(msg),
            RequestProtocol::Unknown,
        );
    }

    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let protocol = RequestProtocol::from_content_type(content_type);

    if protocol == RequestProtocol::Unknown {
        return error_response(
            tddy_rpc::Status::invalid_argument("unsupported content-type"),
            protocol,
        );
    }

    let body = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("ConnectRPC body read error: {}", e);
            return error_response(
                tddy_rpc::Status::invalid_argument("failed to read request body"),
                protocol,
            );
        }
    };

    let messages: Vec<RpcMessage> = if protocol.is_streaming() {
        match parse_envelope_frames(&body) {
            Ok(frames) if frames.is_empty() => {
                return error_response(
                    tddy_rpc::Status::invalid_argument("streaming request has no messages"),
                    protocol,
                );
            }
            Ok(frames) => frames
                .into_iter()
                .map(|payload| RpcMessage {
                    payload,
                    metadata: RequestMetadata::default(),
                })
                .collect(),
            Err(e) => {
                return error_response(
                    tddy_rpc::Status::invalid_argument(format!("invalid envelope framing: {}", e)),
                    protocol,
                );
            }
        }
    } else {
        vec![RpcMessage {
            payload: body.to_vec(),
            metadata: RequestMetadata::default(),
        }]
    };

    match bridge.handle_messages(&service, &method, &messages).await {
        Ok(tddy_rpc::ResponseBody::Complete(chunks)) => {
            if chunks.len() != 1 {
                return error_response(
                    tddy_rpc::Status::internal("unexpected response chunks"),
                    protocol,
                );
            }
            let response_body = if protocol.is_streaming() {
                let mut buf = Vec::new();
                buf.extend_from_slice(&wrap_envelope(&chunks[0], false));
                buf.extend_from_slice(&wrap_end_stream(b"{}"));
                buf
            } else {
                chunks[0].clone()
            };
            let content_type = if protocol.is_streaming() {
                protocol.streaming_response_content_type()
            } else {
                protocol.response_content_type()
            };
            (
                axum::http::StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                Body::from(response_body),
            )
                .into_response()
        }
        Ok(tddy_rpc::ResponseBody::Streaming(rx)) => {
            let mut body_bytes = Vec::new();
            let mut had_error = false;
            let mut stream = ReceiverStream::new(rx);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(payload) => {
                        body_bytes.extend_from_slice(&wrap_envelope(&payload, false));
                    }
                    Err(status) => {
                        had_error = true;
                        let err_json = serde_json::json!({
                            "error": {
                                "code": code_to_connect_str(&status.code),
                                "message": status.message
                            }
                        });
                        body_bytes
                            .extend_from_slice(&wrap_end_stream(err_json.to_string().as_bytes()));
                        break;
                    }
                }
            }
            if !had_error {
                body_bytes.extend_from_slice(&wrap_end_stream(b"{}"));
            }
            (
                axum::http::StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    protocol.streaming_response_content_type(),
                )],
                Body::from(body_bytes),
            )
                .into_response()
        }
        Err(status) => error_response(status, protocol),
    }
}

fn error_response(status: tddy_rpc::Status, _protocol: RequestProtocol) -> Response {
    let http_status = code_to_http_status(&status.code);
    let body = status_to_error_body(&status);
    (
        http_status,
        [(header::CONTENT_TYPE, "application/json")],
        Json(body),
    )
        .into_response()
}
