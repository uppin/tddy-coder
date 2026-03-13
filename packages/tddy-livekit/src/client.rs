//! RPC client for calling services over LiveKit data channel.

use livekit::prelude::*;
use prost::Message;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

use tddy_rpc::{Code, Status};

use crate::envelope::encode_request;
use crate::proto::{CallMetadata, RpcRequest, RpcResponse};
use crate::rpc_trace;

const RPC_TOPIC: &str = "tddy-rpc";

type PendingMap = HashMap<i32, oneshot::Sender<Result<Vec<u8>, Status>>>;
type PendingStreamMap = HashMap<i32, mpsc::Sender<Result<Vec<u8>, Status>>>;

type BidiStreamResult<'a> = Result<
    (
        BidiStreamSender<'a>,
        mpsc::Receiver<Result<Vec<u8>, Status>>,
    ),
    Status,
>;

/// Client for making RPC calls to a participant in a LiveKit room.
pub struct RpcClient {
    room: Room,
    target_identity: ParticipantIdentity,
    next_request_id: AtomicI32,
    pending_unary: Arc<Mutex<PendingMap>>,
    pending_streams: Arc<Mutex<PendingStreamMap>>,
}

impl RpcClient {
    /// Create an RpcClient that sends requests to the given participant.
    /// Spawns a background task to handle incoming responses - use the events from room.subscribe().
    pub fn new(
        room: Room,
        target_identity: impl Into<ParticipantIdentity>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) -> Self {
        let pending_unary: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_streams: Arc<Mutex<PendingStreamMap>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_unary_clone = pending_unary.clone();
        let pending_streams_clone = pending_streams.clone();

        tokio::spawn(async move {
            log::debug!("RpcClient: background event loop started");
            while let Some(event) = events.recv().await {
                if let RoomEvent::DataReceived {
                    payload,
                    topic,
                    kind: _,
                    participant: _,
                } = event
                {
                    if topic.as_deref() != Some(RPC_TOPIC) {
                        continue;
                    }
                    let payload =
                        std::sync::Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                    match RpcResponse::decode(&payload[..]) {
                        Ok(response) => {
                            let request_id = response.request_id;
                            let has_error = response.error.is_some();
                            rpc_trace!(
                                "RpcClient: received response request_id={} error={} end_of_stream={} payload_len={}",
                                request_id, has_error, response.end_of_stream, response.response_message.len()
                            );
                            let result = if let Some(err) = response.error {
                                Err(Status {
                                    code: Code::Unknown,
                                    message: err.message,
                                })
                            } else {
                                Ok(response.response_message)
                            };

                            let mut handled = false;
                            if let Ok(mut streams) = pending_streams_clone.lock() {
                                if let Some(tx) = streams.get(&request_id) {
                                    handled = true;
                                    if tx.try_send(result.clone()).is_ok() {
                                        if has_error || response.end_of_stream {
                                            streams.remove(&request_id);
                                            rpc_trace!(
                                                "RpcClient: stream request_id={} closed (error={} end_of_stream={})",
                                                request_id, has_error, response.end_of_stream
                                            );
                                        }
                                    } else {
                                        rpc_trace!(
                                            "RpcClient: stream try_send failed for request_id={}",
                                            request_id
                                        );
                                    }
                                }
                            }

                            if !handled {
                                if let Ok(mut pending) = pending_unary_clone.lock() {
                                    if let Some(tx) = pending.remove(&request_id) {
                                        let _ = tx.send(result);
                                    } else if !has_error {
                                        rpc_trace!(
                                            "RpcClient: no pending sender for request_id={}",
                                            request_id
                                        );
                                    }
                                }
                            }
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
            target_identity: target_identity.into(),
            next_request_id: AtomicI32::new(1),
            pending_unary,
            pending_streams,
        }
    }

    /// Call a unary RPC method. Returns the raw response bytes.
    pub async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        rpc_trace!(
            "RpcClient::call_unary request_id={} {}/{}  ({} bytes)",
            request_id,
            service,
            method,
            request_bytes.len()
        );

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_unary
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let sender_identity = self.room.local_participant().identity().to_string();
        let request = RpcRequest {
            request_id,
            request_message: request_bytes,
            call_metadata: Some(CallMetadata {
                service: service.to_string(),
                method: method.to_string(),
            }),
            metadata: None,
            end_of_stream: true,
            abort: false,
            sender_identity: Some(sender_identity),
        };

        let payload = encode_request(request).map_err(Status::internal)?;
        let packet = DataPacket {
            payload,
            topic: Some(RPC_TOPIC.to_string()),
            reliable: true,
            destination_identities: vec![self.target_identity.clone()],
        };

        self.room
            .local_participant()
            .publish_data(packet)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        rpc_trace!(
            "RpcClient::call_unary request_id={} published, awaiting response",
            request_id
        );

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
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        rpc_trace!(
            "RpcClient::call_server_stream request_id={} {}/{}  ({} bytes)",
            request_id,
            service,
            method,
            request_bytes.len()
        );

        let (tx, rx) = mpsc::channel(32);

        {
            let mut pending = self
                .pending_streams
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let sender_identity = self.room.local_participant().identity().to_string();
        let request = RpcRequest {
            request_id,
            request_message: request_bytes,
            call_metadata: Some(CallMetadata {
                service: service.to_string(),
                method: method.to_string(),
            }),
            metadata: None,
            end_of_stream: true,
            abort: false,
            sender_identity: Some(sender_identity),
        };

        let payload = encode_request(request).map_err(Status::internal)?;
        let packet = DataPacket {
            payload,
            topic: Some(RPC_TOPIC.to_string()),
            reliable: true,
            destination_identities: vec![self.target_identity.clone()],
        };

        self.room
            .local_participant()
            .publish_data(packet)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        rpc_trace!(
            "RpcClient::call_server_stream request_id={} published, returning receiver",
            request_id
        );

        Ok(rx)
    }

    /// Call a client streaming RPC method. Sends multiple request messages, returns single response.
    pub async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        rpc_trace!(
            "RpcClient::call_client_stream request_id={} {}/{}  ({} messages)",
            request_id,
            service,
            method,
            request_bytes_list.len()
        );

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_unary
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let call_metadata = CallMetadata {
            service: service.to_string(),
            method: method.to_string(),
        };
        let sender_identity = self.room.local_participant().identity().to_string();

        let len = request_bytes_list.len();
        for (i, request_bytes) in request_bytes_list.into_iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == len - 1;
            let request = RpcRequest {
                request_id,
                request_message: request_bytes,
                call_metadata: if is_first {
                    Some(call_metadata.clone())
                } else {
                    None
                },
                metadata: None,
                end_of_stream: is_last,
                abort: false,
                sender_identity: Some(sender_identity.clone()),
            };

            let payload = encode_request(request).map_err(Status::internal)?;
            let packet = DataPacket {
                payload,
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
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        rpc_trace!(
            "RpcClient::call_bidi_stream request_id={} {}/{}  ({} messages)",
            request_id,
            service,
            method,
            request_bytes_list.len()
        );

        let (tx, rx) = mpsc::channel(32);

        {
            let mut pending = self
                .pending_streams
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let call_metadata = CallMetadata {
            service: service.to_string(),
            method: method.to_string(),
        };
        let sender_identity = self.room.local_participant().identity().to_string();

        let mut iter = request_bytes_list.into_iter().peekable();
        let mut is_first = true;
        while let Some(request_bytes) = iter.next() {
            let is_last = iter.peek().is_none();
            let request = RpcRequest {
                request_id,
                request_message: request_bytes,
                call_metadata: if is_first {
                    Some(call_metadata.clone())
                } else {
                    None
                },
                metadata: None,
                end_of_stream: is_last,
                abort: false,
                sender_identity: Some(sender_identity.clone()),
            };
            is_first = false;

            let payload = encode_request(request).map_err(Status::internal)?;
            let packet = DataPacket {
                payload,
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

        if is_first {
            let end_request = RpcRequest {
                request_id,
                request_message: vec![],
                call_metadata: Some(call_metadata),
                metadata: None,
                end_of_stream: true,
                abort: false,
                sender_identity: Some(sender_identity),
            };
            let payload = encode_request(end_request).map_err(Status::internal)?;
            let packet = DataPacket {
                payload,
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

        Ok(rx)
    }

    /// Start a bidirectional stream for real-time send/receive. Caller sends one message,
    /// receives one response, then sends the next. Enables protocol-level tests for
    /// real-time streaming (server processes each message as it arrives, not on end_of_stream).
    pub fn start_bidi_stream(&self, service: &str, method: &str) -> BidiStreamResult<'_> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        rpc_trace!(
            "RpcClient::start_bidi_stream request_id={} {}/{}",
            request_id,
            service,
            method
        );

        let (tx, rx) = mpsc::channel(32);

        {
            let mut pending = self
                .pending_streams
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let sender = BidiStreamSender {
            client: self,
            request_id,
            call_metadata: CallMetadata {
                service: service.to_string(),
                method: method.to_string(),
            },
            sender_identity: self.room.local_participant().identity().to_string(),
            is_first: true,
        };

        Ok((sender, rx))
    }

    pub(crate) async fn publish_request(&self, request: RpcRequest) -> Result<(), Status> {
        let payload = encode_request(request).map_err(Status::internal)?;
        let packet = DataPacket {
            payload,
            topic: Some(RPC_TOPIC.to_string()),
            reliable: true,
            destination_identities: vec![self.target_identity.clone()],
        };
        self.room
            .local_participant()
            .publish_data(packet)
            .await
            .map_err(|e| Status::internal(e.to_string()))
    }
}

/// Sender for incremental bidi stream. Send one message at a time; server should echo each
/// before the next is sent (real-time streaming).
pub struct BidiStreamSender<'a> {
    client: &'a RpcClient,
    request_id: i32,
    call_metadata: CallMetadata,
    sender_identity: String,
    is_first: bool,
}

impl<'a> BidiStreamSender<'a> {
    /// Send one message. Use end_of_stream=true for the last message.
    pub async fn send(
        &mut self,
        request_bytes: Vec<u8>,
        end_of_stream: bool,
    ) -> Result<(), Status> {
        let request = RpcRequest {
            request_id: self.request_id,
            request_message: request_bytes,
            call_metadata: if self.is_first {
                Some(self.call_metadata.clone())
            } else {
                None
            },
            metadata: None,
            end_of_stream,
            abort: false,
            sender_identity: Some(self.sender_identity.clone()),
        };
        self.is_first = false;
        self.client.publish_request(request).await
    }
}
