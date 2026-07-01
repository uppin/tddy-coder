//! stdio-side implementation of [`RpcClientTransport`]: sends framed `RpcRequest`s to the peer
//! on the other end of the pipe and resolves them as framed `RpcResponse`s arrive (fed in by
//! [`crate::StdioEndpoint::run`] via [`StdioRpcClient::deliver_response`]).

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::client_engine::ClientEngine;
use tddy_rpc::envelope::{self, RpcRequest, RpcResponse};
use tddy_rpc::transport::{encode_frame, FrameKind};
use tddy_rpc::{RpcClientTransport, Status};
use tokio::sync::mpsc;

/// Identity used on outgoing requests. A stdio pipe pair has exactly one peer per end, so this
/// only needs to be a stable, human-readable label (unlike LiveKit's participant identity, which
/// must be a real room-unique address).
const LOCAL_IDENTITY: &str = "stdio-client";

type BidiStreamResult<'a> =
    Result<(StdioBidiSender<'a>, mpsc::Receiver<Result<Vec<u8>, Status>>), Status>;

/// Client for calling RPCs over a framed stdio (or stdio-like) duplex channel.
pub struct StdioRpcClient {
    engine: ClientEngine,
    writer_tx: mpsc::Sender<Vec<u8>>,
}

impl StdioRpcClient {
    pub(crate) fn new(writer_tx: mpsc::Sender<Vec<u8>>) -> Arc<Self> {
        Arc::new(Self {
            engine: ClientEngine::new(LOCAL_IDENTITY),
            writer_tx,
        })
    }

    /// Feed a decoded response arriving from the peer into the correlation engine. Called by
    /// [`crate::StdioEndpoint::run`]'s read loop.
    pub(crate) async fn deliver_response(&self, response: RpcResponse) {
        self.engine.on_response(response).await;
    }

    async fn send_frame(&self, request: RpcRequest) -> Result<(), Status> {
        let payload = envelope::encode_request(request).map_err(Status::internal)?;
        let frame = encode_frame(FrameKind::Request, &payload);
        self.writer_tx
            .send(frame)
            .await
            .map_err(|_| Status::internal("stdio channel closed"))
    }

    async fn send_message_list(
        &self,
        request_id: i32,
        service: &str,
        method: &str,
        payloads: Vec<Vec<u8>>,
    ) -> Result<(), Status> {
        if payloads.is_empty() {
            let request = self
                .engine
                .build_request(request_id, service, method, Vec::new(), true);
            return self.send_frame(request).await;
        }
        let len = payloads.len();
        for (i, payload) in payloads.into_iter().enumerate() {
            let end_of_stream = i + 1 == len;
            let request = if i == 0 {
                self.engine
                    .build_request(request_id, service, method, payload, end_of_stream)
            } else {
                self.engine
                    .build_continuation(request_id, payload, end_of_stream)
            };
            self.send_frame(request).await?;
        }
        Ok(())
    }

    /// Start an incremental bidirectional stream: send one message at a time, awaiting each
    /// response before sending the next (real-time streaming — the peer processes each message
    /// as it arrives, not on end_of_stream).
    pub fn start_bidi_stream(&self, service: &str, method: &str) -> BidiStreamResult<'_> {
        let (request_id, rx) = self.engine.register_stream();
        Ok((
            StdioBidiSender {
                client: self,
                request_id,
                service: service.to_string(),
                method: method.to_string(),
                is_first: true,
            },
            rx,
        ))
    }
}

#[async_trait]
impl RpcClientTransport for StdioRpcClient {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let (request, rx) = self.engine.begin_unary(service, method, request_bytes);
        self.send_frame(request).await?;
        rx.await
            .map_err(|_| Status::internal("response channel closed"))?
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let (request, rx) = self.engine.begin_stream(service, method, request_bytes);
        self.send_frame(request).await?;
        Ok(rx)
    }

    async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        let (request_id, rx) = self.engine.register_unary();
        self.send_message_list(request_id, service, method, request_bytes_list)
            .await?;
        rx.await
            .map_err(|_| Status::internal("response channel closed"))?
    }

    async fn call_bidi_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let (request_id, rx) = self.engine.register_stream();
        self.send_message_list(request_id, service, method, request_bytes_list)
            .await?;
        Ok(rx)
    }
}

/// Sender for an incremental bidi stream. Send one message at a time; the peer should react to
/// each before the next is sent for true real-time streaming.
pub struct StdioBidiSender<'a> {
    client: &'a StdioRpcClient,
    request_id: i32,
    service: String,
    method: String,
    is_first: bool,
}

impl StdioBidiSender<'_> {
    /// Send one message. Use `end_of_stream = true` for the last message.
    pub async fn send(
        &mut self,
        request_bytes: Vec<u8>,
        end_of_stream: bool,
    ) -> Result<(), Status> {
        let request = if self.is_first {
            self.client.engine.build_request(
                self.request_id,
                &self.service,
                &self.method,
                request_bytes,
                end_of_stream,
            )
        } else {
            self.client
                .engine
                .build_continuation(self.request_id, request_bytes, end_of_stream)
        };
        self.is_first = false;
        self.client.send_frame(request).await
    }
}
