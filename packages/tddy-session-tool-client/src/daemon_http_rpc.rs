//! A `tddy-rpc` client over the daemon's HTTP `/rpc` endpoint (the Connect protocol, protobuf
//! payloads) — what lets `tddy-tools` in remote-codebase mode reach the roster and conversation
//! RPCs through the local relay daemon, which forwards them to the codebase daemon named by each
//! request's `daemon_instance_id`.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host

use async_trait::async_trait;
use tddy_rpc::{Code, RpcClientTransport, Status};
use tokio::sync::mpsc;

const CONNECT_PROTOCOL_VERSION_HEADER: &str = "connect-protocol-version";
const CONNECT_PROTOCOL_VERSION: &str = "1";
const CONTENT_TYPE_PROTO: &str = "application/proto";
const CONTENT_TYPE_CONNECT_PROTO: &str = "application/connect+proto";

/// The flags byte of an envelope carrying one message.
const FLAG_MESSAGE: u8 = 0x00;
/// The flags byte of the envelope that ends a stream; its payload is the trailer, which names an
/// error when the stream ended in one.
const FLAG_END_STREAM: u8 = 0x02;
/// The flags byte, then the payload's length as a big-endian `u32`.
const ENVELOPE_HEADER_LEN: usize = 5;

/// Messages a server stream may run ahead of the caller reading them.
const STREAM_BUFFER: usize = 32;

/// Connect-protocol client for `POST <base_url>/<service>/<method>`: unary calls as
/// `application/proto`, server streams as enveloped `application/connect+proto`.
#[derive(Debug, Clone)]
pub struct DaemonHttpRpcClient {
    /// The daemon's RPC base, e.g. `http://127.0.0.1:8899/rpc`.
    pub base_url: String,
    /// Shared by every call this client makes, so its connections are pooled across them.
    http: reqwest::Client,
}

impl DaemonHttpRpcClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    fn method_url(&self, service: &str, method: &str) -> String {
        format!("{}/{service}/{method}", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl RpcClientTransport for DaemonHttpRpcClient {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let response = self
            .http
            .post(self.method_url(service, method))
            .header(CONNECT_PROTOCOL_VERSION_HEADER, CONNECT_PROTOCOL_VERSION)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE_PROTO)
            .body(request_bytes)
            .send()
            .await
            .map_err(transport_error)?;
        let http_status = response.status();
        let body = response.bytes().await.map_err(transport_error)?;
        if http_status.is_success() {
            Ok(body.to_vec())
        } else {
            Err(error_from_body(http_status, &body))
        }
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let response = self
            .http
            .post(self.method_url(service, method))
            .header(CONNECT_PROTOCOL_VERSION_HEADER, CONNECT_PROTOCOL_VERSION)
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE_CONNECT_PROTO)
            .body(envelope(FLAG_MESSAGE, &request_bytes))
            .send()
            .await
            .map_err(transport_error)?;
        let http_status = response.status();
        if !http_status.is_success() {
            let body = response.bytes().await.map_err(transport_error)?;
            return Err(error_from_body(http_status, &body));
        }
        let (tx, rx) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(relay_envelopes(response, tx));
        Ok(rx)
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented(
            "the daemon's HTTP endpoint serves no client-streaming RPC this client is used for",
        ))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented(
            "the daemon's HTTP endpoint serves no bidirectional RPC this client is used for",
        ))
    }
}

/// Forwards each message of a server stream to `tx` as its bytes arrive, then ends the stream with
/// the error its trailer names, if any.
///
/// Stops early when the caller drops the receiver, so an abandoned stream does not keep its
/// connection open until the daemon next writes.
async fn relay_envelopes(
    mut response: reqwest::Response,
    tx: mpsc::Sender<Result<Vec<u8>, Status>>,
) {
    let mut reader = EnvelopeReader::default();
    loop {
        let chunk = tokio::select! {
            () = tx.closed() => return,
            chunk = response.chunk() => chunk,
        };
        let chunk = match chunk {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                let _ = tx
                    .send(Err(Status::unavailable(
                        "the daemon closed the stream before its end-of-stream envelope",
                    )))
                    .await;
                return;
            }
            Err(error) => {
                let _ = tx.send(Err(transport_error(error))).await;
                return;
            }
        };
        reader.push(&chunk);
        loop {
            match reader.next_frame() {
                Ok(None) => break,
                Ok(Some(Frame::Message(payload))) => {
                    if tx.send(Ok(payload)).await.is_err() {
                        return;
                    }
                }
                Ok(Some(Frame::EndOfStream(trailer))) => {
                    if let Some(status) = trailer_error(&trailer) {
                        let _ = tx.send(Err(status)).await;
                    }
                    return;
                }
                Err(status) => {
                    let _ = tx.send(Err(status)).await;
                    return;
                }
            }
        }
    }
}

/// One envelope of a server stream.
enum Frame {
    Message(Vec<u8>),
    EndOfStream(Vec<u8>),
}

/// Splits a response body into its envelopes as the bytes arrive, which may cut them anywhere.
#[derive(Default)]
struct EnvelopeReader {
    buffered: Vec<u8>,
}

impl EnvelopeReader {
    fn push(&mut self, bytes: &[u8]) {
        self.buffered.extend_from_slice(bytes);
    }

    /// The next complete envelope, or `None` while the buffered bytes hold only part of one.
    fn next_frame(&mut self) -> Result<Option<Frame>, Status> {
        let Some(header) = self.buffered.get(..ENVELOPE_HEADER_LEN) else {
            return Ok(None);
        };
        let flags = header[0];
        let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
        let end = ENVELOPE_HEADER_LEN + length;
        if self.buffered.len() < end {
            return Ok(None);
        }
        let payload = self.buffered[ENVELOPE_HEADER_LEN..end].to_vec();
        self.buffered.drain(..end);
        match flags {
            FLAG_MESSAGE => Ok(Some(Frame::Message(payload))),
            FLAG_END_STREAM => Ok(Some(Frame::EndOfStream(payload))),
            other => Err(Status::internal(format!(
                "the daemon sent an envelope with flags {other:#04x}, which this client does not read"
            ))),
        }
    }
}

/// One envelope: its flags byte, the payload's length, the payload.
fn envelope(flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ENVELOPE_HEADER_LEN + payload.len());
    frame.push(flags);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// The error an end-of-stream trailer names. An empty trailer, or one without an `error` member
/// (the daemon writes `{}`), is a stream that ended cleanly.
fn trailer_error(trailer: &[u8]) -> Option<Status> {
    if trailer.is_empty() {
        return None;
    }
    let value: serde_json::Value = match serde_json::from_slice(trailer) {
        Ok(value) => value,
        Err(error) => {
            return Some(Status::internal(format!(
                "the daemon's end-of-stream envelope is not JSON: {error}"
            )));
        }
    };
    value.get("error").map(status_from_error_object)
}

/// The status of a non-200 answer. Unary errors arrive as `{"code": ..., "message": ...}`; a
/// body that is not that shape names no status, so it is reported as `Unknown` with the HTTP
/// status and the body it did carry.
fn error_from_body(http_status: reqwest::StatusCode, body: &[u8]) -> Status {
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(value) if value.get("code").is_some() => status_from_error_object(&value),
        _ => Status {
            code: Code::Unknown,
            message: format!(
                "the daemon answered HTTP {http_status}: {}",
                String::from_utf8_lossy(body).trim()
            ),
        },
    }
}

/// The status a Connect error object (`{"code": "not_found", "message": ...}`) names.
fn status_from_error_object(error: &serde_json::Value) -> Status {
    let code = error
        .get("code")
        .and_then(serde_json::Value::as_str)
        .map_or(Code::Unknown, connect_code);
    let message = error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    Status {
        code,
        message: message.to_string(),
    }
}

/// The `tddy-rpc` code a Connect error code names. A code this client does not know reads as
/// `Unknown`.
fn connect_code(name: &str) -> Code {
    match name {
        "canceled" => Code::Cancelled,
        "unknown" => Code::Unknown,
        "invalid_argument" => Code::InvalidArgument,
        "deadline_exceeded" => Code::DeadlineExceeded,
        "not_found" => Code::NotFound,
        "already_exists" => Code::AlreadyExists,
        "permission_denied" => Code::PermissionDenied,
        "resource_exhausted" => Code::ResourceExhausted,
        "failed_precondition" => Code::FailedPrecondition,
        "aborted" => Code::Aborted,
        "out_of_range" => Code::OutOfRange,
        "unimplemented" => Code::Unimplemented,
        "internal" => Code::Internal,
        "unavailable" => Code::Unavailable,
        "data_loss" => Code::DataLoss,
        "unauthenticated" => Code::Unauthenticated,
        _ => Code::Unknown,
    }
}

/// A request that got no answer: the daemon is unreachable, or it dropped the connection.
fn transport_error(error: reqwest::Error) -> Status {
    Status::unavailable(format!(
        "the daemon's HTTP endpoint did not answer: {error}"
    ))
}
