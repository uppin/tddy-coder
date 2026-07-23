//! Per-room RPC client factory.
//!
//! A daemon forwarding to several peers, or a browser talking to several daemons, needs many
//! [`RpcClient`]s over one LiveKit connection. Building an independent client per call (as
//! `forward_to_peer` historically did) gives each its own [`ClientEngine`] and its own
//! `room.subscribe()` loop — colliding request-id spaces (each starts at 1) and a leaked loop per
//! call. [`LiveKitRpcClientFactory`] owns **one** `ClientEngine` (request-id registry) and **one**
//! response loop per room, and vends lightweight clients that all share them: request ids stay
//! distinct across every target, and there is a single loop regardless of how many clients are
//! vended.
//!
//! The factory is a **singleton per room**: `for_room` returns a handle onto the same shared
//! registry for the same `Arc<Room>`, so clients obtained through separate handles still draw from
//! one id space.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use livekit::prelude::*;

use tddy_rpc::client_engine::ClientEngine;

use crate::chunking::{self, ChunkReassembler};
use crate::client::RpcClient;
use crate::envelope::decode_response;
use crate::rpc_trace;

const RPC_TOPIC: &str = "tddy-rpc";

/// Shared per-room state: the single request-id registry every vended client uses. The response
/// loop (spawned in [`LiveKitRpcClientFactory::build`]) feeds decoded responses into it, and holds
/// this `Arc` for its whole life — so the registry stays live (and reusable via the table) exactly
/// while the room's event stream is open. It deliberately does **not** hold the `Arc<Room>`, so it
/// never keeps the connection alive; when the room drops, the stream closes, the loop ends, and the
/// registry is released.
struct RoomRpcRegistry {
    engine: Arc<ClientEngine>,
}

/// Room identity used to key the singleton table. `Arc<Room>` clones share one allocation, so its
/// pointer identifies the underlying connection.
fn room_key(room: &Arc<Room>) -> usize {
    Arc::as_ptr(room) as usize
}

fn registry_table() -> &'static Mutex<HashMap<usize, Weak<RoomRpcRegistry>>> {
    static TABLE: OnceLock<Mutex<HashMap<usize, Weak<RoomRpcRegistry>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hands out [`RpcClient`]s that share one request-id registry and one response loop per room.
#[derive(Clone)]
pub struct LiveKitRpcClientFactory {
    room: Arc<Room>,
    registry: Arc<RoomRpcRegistry>,
}

impl LiveKitRpcClientFactory {
    /// Return the factory for `room`, creating its shared registry (and response loop) on first
    /// use and reusing it for every later call with the same `Arc<Room>`.
    pub fn for_room(room: Arc<Room>) -> Self {
        let key = room_key(&room);
        let mut table = registry_table().lock().expect("rpc client factory table");
        if let Some(existing) = table.get(&key).and_then(Weak::upgrade) {
            return Self {
                room,
                registry: existing,
            };
        }
        let registry = Self::build(&room);
        table.insert(key, Arc::downgrade(&registry));
        Self { room, registry }
    }

    /// Whether a live shared registry already exists for `room`, without creating one. Lets callers
    /// (and tests) observe that a room's clients are being served by a single shared factory.
    pub fn is_registered(room: &Arc<Room>) -> bool {
        registry_table()
            .lock()
            .expect("rpc client factory table")
            .get(&room_key(room))
            .and_then(Weak::upgrade)
            .is_some()
    }

    /// Vend a client that sends to `target` over the room's shared registry.
    pub fn client(&self, target: impl Into<ParticipantIdentity>) -> RpcClient {
        RpcClient::from_shared_engine(
            self.room.clone(),
            target.into(),
            self.registry.engine.clone(),
        )
    }

    /// Create the shared registry for a room and spawn its single response loop. The loop routes
    /// every RPC response into the shared engine by `request_id`; because that engine is the only
    /// id allocator for the room, an id identifies exactly one pending call regardless of which
    /// peer replied, so no sender filter is needed. The loop holds the registry `Arc`, so it (and
    /// the table entry) stays live for as long as the room's event stream is open.
    fn build(room: &Arc<Room>) -> Arc<RoomRpcRegistry> {
        let local_identity = room.local_participant().identity().to_string();
        let engine = Arc::new(ClientEngine::new(local_identity));
        let registry = Arc::new(RoomRpcRegistry { engine });
        let registry_for_task = registry.clone();
        let mut events = room.subscribe();

        tokio::spawn(async move {
            let engine_for_task = registry_for_task.engine.clone();
            log::debug!("LiveKitRpcClientFactory: shared response loop started");
            // This loop reassembles responses from *every* peer in the room (a browser may talk to
            // several daemons), so chunk frames are grouped per sender — message ids are only
            // unique within one sender, and mixing two senders' chunks would corrupt reassembly.
            let mut reassemblers: HashMap<Option<String>, ChunkReassembler> = HashMap::new();
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
                    let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                    let payload = if chunking::is_chunk_frame(&payload) {
                        let sender = participant.as_ref().map(|p| p.identity().to_string());
                        let reassembler = reassemblers.entry(sender).or_default();
                        match reassembler.accept(&payload) {
                            Ok(Some(full)) => full,
                            Ok(None) => continue,
                            Err(e) => {
                                rpc_trace!("LiveKitRpcClientFactory: malformed chunk frame: {}", e);
                                continue;
                            }
                        }
                    } else {
                        payload
                    };
                    match decode_response(&payload) {
                        Ok(response) => {
                            rpc_trace!(
                                "LiveKitRpcClientFactory: response request_id={} error={} end_of_stream={} payload_len={}",
                                response.request_id,
                                response.error.is_some(),
                                response.end_of_stream,
                                response.response_message.len()
                            );
                            engine_for_task.on_response(response).await;
                        }
                        Err(e) => {
                            rpc_trace!("LiveKitRpcClientFactory: failed to decode response: {}", e);
                        }
                    }
                }
            }
            log::debug!("LiveKitRpcClientFactory: shared response loop ended");
        });

        registry
    }
}
