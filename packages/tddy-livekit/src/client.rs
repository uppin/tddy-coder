//! RPC client for calling services over LiveKit data channel.
//!
//! Delegates request-id correlation to [`tddy_rpc::client_engine::ClientEngine`] — the same
//! engine `tddy-stdio`'s `StdioRpcClient` uses — so this transport only owns what's genuinely
//! LiveKit-specific: building a `DataPacket` and publishing it via `Room::local_participant`.

use livekit::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use async_trait::async_trait;
use tddy_rpc::client_engine::ClientEngine;
use tddy_rpc::{RpcClientTransport, Status};

use crate::chunking::{self, ChunkReassembler};
use crate::envelope::{decode_response, encode_request};
use crate::proto::RpcRequest;
use crate::rpc_trace;

const RPC_TOPIC: &str = "tddy-rpc";

type BidiStreamResult<'a> = Result<
    (
        BidiStreamSender<'a>,
        mpsc::Receiver<Result<Vec<u8>, Status>>,
    ),
    Status,
>;

/// Client for making RPC calls to a participant in a LiveKit room.
pub struct RpcClient {
    room: Arc<Room>,
    target_identity: ParticipantIdentity,
    engine: Arc<ClientEngine>,
}

impl RpcClient {
    /// Create an RpcClient that sends requests to the given participant.
    /// Spawns a background task to handle incoming responses - use the events from room.subscribe().
    pub fn new(
        room: Room,
        target_identity: impl Into<ParticipantIdentity>,
        events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) -> Self {
        Self::new_shared(Arc::new(room), target_identity, events)
    }

    /// Like [`Self::new`], but shares an existing [`Arc`] so multiple clients can use the same room
    /// (e.g. discovery + **StartSession** forwarding on one LiveKit connection).
    pub fn new_shared(
        room: Arc<Room>,
        target_identity: impl Into<ParticipantIdentity>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) -> Self {
        let target_identity = target_identity.into();
        let local_identity = room.local_participant().identity().to_string();
        let engine = Arc::new(ClientEngine::new(local_identity));
        let engine_for_task = engine.clone();
        // Only responses published by *this* client's target participant are ours. Several
        // `RpcClient`s can share one room (a daemon forwarding to multiple peers, a browser talking
        // to multiple daemons), and each `ClientEngine` numbers request ids from 1 independently —
        // so without filtering by sender, a sibling client's response carrying a colliding request
        // id would resolve the wrong pending call, crossing responses between peers.
        let target_for_task = target_identity.clone();

        tokio::spawn(async move {
            log::debug!("RpcClient: background event loop started");
            // The loop only accepts frames from `target_for_task`, so one reassembler (one sender)
            // suffices; ordered reliable delivery keeps a message's chunks contiguous per sender.
            let mut reassembler = ChunkReassembler::default();
            while let Some(event) = events.recv().await {
                if let RoomEvent::DataReceived {
                    payload,
                    topic,
                    kind: _,
                    participant,
                } = event
                {
                    if topic.as_deref() != Some(RPC_TOPIC) {
                        continue;
                    }
                    let from_target = match &participant {
                        Some(p) => p.identity() == target_for_task,
                        None => false,
                    };
                    if !from_target {
                        continue;
                    }
                    let payload =
                        std::sync::Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                    let payload = if chunking::is_chunk_frame(&payload) {
                        match reassembler.accept(&payload) {
                            Ok(Some(full)) => full,
                            Ok(None) => continue,
                            Err(e) => {
                                rpc_trace!("RpcClient: malformed chunk frame: {}", e);
                                continue;
                            }
                        }
                    } else {
                        payload
                    };
                    match decode_response(&payload) {
                        Ok(response) => {
                            rpc_trace!(
                                "RpcClient: received response request_id={} error={} end_of_stream={} payload_len={}",
                                response.request_id,
                                response.error.is_some(),
                                response.end_of_stream,
                                response.response_message.len()
                            );
                            engine_for_task.on_response(response).await;
                        }
                        Err(e) => {
                            rpc_trace!("RpcClient: failed to decode response: {}", e);
                        }
                    }
                }
            }
            log::debug!("RpcClient: background event loop ended");
        });

        Self {
            room,
            target_identity,
            engine,
        }
    }

    /// Build a client over a **shared** room + [`ClientEngine`], without spawning its own response
    /// loop. Used by [`crate::client_factory::LiveKitRpcClientFactory`], which owns the single
    /// response loop and request-id registry for a room and vends one of these per target — so
    /// every client on that room shares one id space and one `subscribe()` loop.
    pub(crate) fn from_shared_engine(
        room: Arc<Room>,
        target_identity: ParticipantIdentity,
        engine: Arc<ClientEngine>,
    ) -> Self {
        Self {
            room,
            target_identity,
            engine,
        }
    }

    /// Call a unary RPC method. Returns the raw response bytes.
    pub async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let (request, rx) = self.engine.begin_unary(service, method, request_bytes);
        rpc_trace!(
            "RpcClient::call_unary request_id={} {}/{}",
            request.request_id,
            service,
            method
        );
        self.publish_request(request).await?;
        rx.await
            .map_err(|_| Status::internal("response channel closed"))?
    }

    /// Call a server streaming RPC method. Returns a receiver for the response stream.
    /// Each item is a Result containing raw response bytes or a Status error.
    /// The channel closes when the server sends end_of_stream.
    pub async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let (request, rx) = self.engine.begin_stream(service, method, request_bytes);
        rpc_trace!(
            "RpcClient::call_server_stream request_id={} {}/{}",
            request.request_id,
            service,
            method
        );
        self.publish_request(request).await?;
        Ok(rx)
    }

    /// Call a client streaming RPC method. Sends multiple request messages, returns single response.
    pub async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        let (request_id, rx) = self.engine.register_unary();
        rpc_trace!(
            "RpcClient::call_client_stream request_id={} {}/{} ({} messages)",
            request_id,
            service,
            method,
            request_bytes_list.len()
        );
        self.publish_message_list(request_id, service, method, request_bytes_list)
            .await?;
        rx.await
            .map_err(|_| Status::internal("response channel closed"))?
    }

    /// Call a bidirectional streaming RPC method. Sends multiple requests, returns receiver for response stream.
    pub async fn call_bidi_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let (request_id, rx) = self.engine.register_stream();
        rpc_trace!(
            "RpcClient::call_bidi_stream request_id={} {}/{} ({} messages)",
            request_id,
            service,
            method,
            request_bytes_list.len()
        );
        self.publish_message_list(request_id, service, method, request_bytes_list)
            .await?;
        Ok(rx)
    }

    /// Publish a list of request messages under one `request_id`: the first carries
    /// `call_metadata`, the rest are continuations, and the last is marked `end_of_stream`. An
    /// empty list still opens the call (a single `call_metadata`-bearing, `end_of_stream` frame
    /// with no payload) so the server has something to dispatch.
    async fn publish_message_list(
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
            return self.publish_request(request).await;
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
            self.publish_request(request).await?;
        }
        Ok(())
    }

    /// Start a bidirectional stream for real-time send/receive. Caller sends one message,
    /// receives one response, then sends the next. Enables protocol-level tests for
    /// real-time streaming (server processes each message as it arrives, not on end_of_stream).
    pub fn start_bidi_stream(&self, service: &str, method: &str) -> BidiStreamResult<'_> {
        let (request_id, rx) = self.engine.register_stream();
        rpc_trace!(
            "RpcClient::start_bidi_stream request_id={} {}/{}",
            request_id,
            service,
            method
        );
        Ok((
            BidiStreamSender {
                client: self,
                request_id,
                service: service.to_string(),
                method: method.to_string(),
                is_first: true,
            },
            rx,
        ))
    }

    pub(crate) async fn publish_request(&self, request: RpcRequest) -> Result<(), Status> {
        let payload = encode_request(request).map_err(Status::internal)?;
        // Fits-in-one-packet requests go out raw (unchanged wire bytes); an oversized request is
        // split into chunk frames that each fit LiveKit's negotiated max message size.
        let message_id = chunking::next_message_id();
        for frame in chunking::frame_for_transport(message_id, &payload) {
            let packet = DataPacket {
                payload: frame,
                topic: Some(RPC_TOPIC.to_string()),
                reliable: true,
                destination_identities: vec![self.target_identity.clone()],
            };
            self.room
                .local_participant()
                .publish_data(packet)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait]
impl RpcClientTransport for RpcClient {
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.call_unary(service, method, request_bytes).await
    }

    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.call_server_stream(service, method, request_bytes)
            .await
    }

    async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        self.call_client_stream(service, method, request_bytes_list)
            .await
    }

    async fn call_bidi_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        self.call_bidi_stream(service, method, request_bytes_list)
            .await
    }
}

/// Sender for incremental bidi stream. Send one message at a time; server should echo each
/// before the next is sent (real-time streaming).
pub struct BidiStreamSender<'a> {
    client: &'a RpcClient,
    request_id: i32,
    service: String,
    method: String,
    is_first: bool,
}

impl BidiStreamSender<'_> {
    /// Send one message. Use end_of_stream=true for the last message.
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
        self.client.publish_request(request).await
    }
}
