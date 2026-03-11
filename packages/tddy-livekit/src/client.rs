//! RPC client for calling services over LiveKit data channel.

use livekit::prelude::*;
use prost::Message;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::envelope::encode_request;
use crate::proto::{CallMetadata, RpcRequest, RpcResponse};
use crate::status::Status;

const RPC_TOPIC: &str = "tddy-rpc";

/// Client for making RPC calls to a participant in a LiveKit room.
pub struct RpcClient {
    room: Room,
    target_identity: ParticipantIdentity,
    next_request_id: AtomicI32,
    pending_unary: Arc<Mutex<HashMap<i32, oneshot::Sender<Result<Vec<u8>, Status>>>>>,
}

impl RpcClient {
    /// Create an RpcClient that sends requests to the given participant.
    /// Spawns a background task to handle incoming responses - use the events from room.subscribe().
    pub fn new(
        room: Room,
        target_identity: impl Into<ParticipantIdentity>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) -> Self {
        let pending_unary: Arc<Mutex<HashMap<i32, oneshot::Sender<Result<Vec<u8>, Status>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending_unary.clone();

        tokio::spawn(async move {
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
                    if let Ok(response) = RpcResponse::decode(&payload[..]) {
                        let request_id = response.request_id;
                        let result = if let Some(err) = response.error {
                            Err(Status {
                                code: crate::status::Code::Unknown,
                                message: err.message,
                            })
                        } else {
                            Ok(response.response_message)
                        };
                        if let Ok(mut pending) = pending_clone.lock() {
                            if let Some(tx) = pending.remove(&request_id) {
                                let _ = tx.send(result);
                            }
                        }
                    }
                }
            }
        });

        Self {
            room,
            target_identity: target_identity.into(),
            next_request_id: AtomicI32::new(1),
            pending_unary,
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

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_unary
                .lock()
                .map_err(|e| Status::internal(e.to_string()))?;
            pending.insert(request_id, tx);
        }

        let request = RpcRequest {
            request_id,
            request_message: request_bytes,
            call_metadata: Some(CallMetadata {
                service: service.to_string(),
                method: method.to_string(),
            }),
            metadata: None,
            end_of_stream: false,
            abort: false,
        };

        let payload = encode_request(request).map_err(|e| Status::internal(e))?;
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

        rx.await
            .map_err(|_| Status::internal("response channel closed"))?
    }
}
