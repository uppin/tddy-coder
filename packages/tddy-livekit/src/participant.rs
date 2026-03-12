//! LiveKit room participant that serves RPC over the data channel.

use livekit::prelude::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::bridge::RpcBridge;
use crate::envelope::{decode_request, encode_response, response_from_result};

const RPC_TOPIC: &str = "tddy-rpc";

/// A LiveKit room participant that routes RPC traffic to an RpcBridge.
pub struct LiveKitParticipant<S: crate::bridge::RpcService> {
    room: Room,
    bridge: Arc<RpcBridge<S>>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
}

impl<S: crate::bridge::RpcService> LiveKitParticipant<S> {
    /// Connect to a LiveKit room and create a participant that will serve RPC.
    pub async fn connect(
        url: &str,
        token: &str,
        service: S,
        room_options: RoomOptions,
    ) -> Result<Self, livekit::RoomError> {
        let bridge = RpcBridge::new(service);
        let (room, events) = Room::connect(url, token, room_options).await?;
        Ok(Self {
            room,
            bridge: Arc::new(bridge),
            events,
        })
    }

    /// Run the participant event loop. Processes DataReceived events for topic "tddy-rpc"
    /// and dispatches to the RpcBridge. Returns when the room disconnects.
    pub async fn run(mut self) {
        while let Some(event) = self.events.recv().await {
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
                let Some(remote) = participant else {
                    log::warn!("DataReceived without participant identity, ignoring");
                    continue;
                };
                let sender_identity = remote.identity().clone();
                let bridge = self.bridge.clone();
                let local = self.room.local_participant();
                let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());

                tokio::spawn(async move {
                    if let Err(e) =
                        Self::handle_incoming(&payload, sender_identity, &bridge, &local).await
                    {
                        log::error!("RPC handle error: {}", e);
                    }
                });
            }
        }
    }

    async fn handle_incoming(
        payload: &[u8],
        sender_identity: ParticipantIdentity,
        bridge: &RpcBridge<S>,
        local: &LocalParticipant,
    ) -> Result<(), String> {
        let request = decode_request(payload)?;
        let request_id = request.request_id;

        let result = bridge.handle_decoded_request(&request).await;

        match result {
            Ok(chunks) => {
                let len = chunks.len();
                for (i, bytes) in chunks.into_iter().enumerate() {
                    let end_of_stream = i == len - 1;
                    let response = crate::proto::RpcResponse {
                        request_id,
                        response_message: bytes,
                        metadata: None,
                        end_of_stream,
                        error: None,
                        trailers: None,
                    };
                    let encoded = encode_response(response)?;
                    let packet = DataPacket {
                        payload: encoded,
                        topic: Some(RPC_TOPIC.to_string()),
                        reliable: true,
                        destination_identities: vec![sender_identity.clone()],
                    };
                    local
                        .publish_data(packet)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Err(status) => {
                let response = response_from_result(request_id, Err(status));
                let encoded = encode_response(response)?;
                let packet = DataPacket {
                    payload: encoded,
                    topic: Some(RPC_TOPIC.to_string()),
                    reliable: true,
                    destination_identities: vec![sender_identity],
                };
                local
                    .publish_data(packet)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Access the underlying room.
    pub fn room(&self) -> &Room {
        &self.room
    }
}
