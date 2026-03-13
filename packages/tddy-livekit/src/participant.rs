//! LiveKit room participant that serves RPC over the data channel.

use livekit::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::bridge::RpcBridge;
use crate::envelope::{decode_request, encode_response, response_from_result};
use crate::proto::RpcRequest;
use crate::rpc_trace;

const RPC_TOPIC: &str = "tddy-rpc";

/// Accumulated stream state: sender identity and messages.
struct ActiveStream {
    sender_identity: ParticipantIdentity,
    messages: Vec<RpcRequest>,
}

/// A LiveKit room participant that routes RPC traffic to an RpcBridge.
pub struct LiveKitParticipant<S: crate::bridge::RpcService> {
    room: Room,
    bridge: Arc<RpcBridge<S>>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    active_streams: Arc<Mutex<HashMap<i32, ActiveStream>>>,
    /// Payloads received with participant=None before any remote joined (race with ParticipantConnected).
    pending_data: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl<S: crate::bridge::RpcService> LiveKitParticipant<S> {
    /// Connect to a LiveKit room and create a participant that will serve RPC.
    pub async fn connect(
        url: &str,
        token: &str,
        service: S,
        room_options: RoomOptions,
    ) -> Result<Self, livekit::RoomError> {
        log::debug!("LiveKitParticipant::connect url={}", url);
        let bridge = RpcBridge::new(service);
        let (room, events) = Room::connect(url, token, room_options).await?;
        log::info!(
            "[echo_server] LiveKitParticipant connected, identity={:?}",
            room.local_participant().identity()
        );
        Ok(Self {
            room,
            bridge: Arc::new(bridge),
            events,
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            pending_data: Arc::new(Mutex::new(VecDeque::new())),
        })
    }

    /// Run the participant event loop. Processes DataReceived events for topic "tddy-rpc"
    /// and dispatches to the RpcBridge. Returns when the room disconnects.
    pub async fn run(mut self) {
        log::info!("[echo_server] LiveKitParticipant event loop started");
        while let Some(event) = self.events.recv().await {
            match event {
                RoomEvent::ParticipantConnected(remote) => {
                    let identity = remote.identity().clone();
                    let drained: Vec<_> = {
                        let mut pending = self.pending_data.lock().await;
                        pending.drain(..).collect()
                    };
                    if !drained.is_empty() {
                        log::debug!(
                            "[echo_server] Processing {} buffered payload(s) from {:?}",
                            drained.len(),
                            identity
                        );
                        let remote_identities: Vec<_> =
                            self.room.remote_participants().keys().cloned().collect();
                        for payload in drained {
                            let bridge = self.bridge.clone();
                            let local = self.room.local_participant();
                            let active_streams = self.active_streams.clone();
                            let event_participant = Some(identity.clone());
                            let remote_identities = remote_identities.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_incoming(
                                    &payload,
                                    event_participant,
                                    &remote_identities,
                                    &bridge,
                                    &local,
                                    &active_streams,
                                )
                                .await
                                {
                                    log::error!("RPC handle error (buffered): {}", e);
                                }
                            });
                        }
                    }
                }
                RoomEvent::DataReceived {
                    payload,
                    topic,
                    kind: _,
                    participant,
                } => {
                    if topic.as_deref() != Some(RPC_TOPIC) {
                        continue;
                    }
                    let (event_participant, remote_identities): (
                        Option<ParticipantIdentity>,
                        Vec<ParticipantIdentity>,
                    ) = match &participant {
                        Some(remote) => {
                            let identity = remote.identity().clone();
                            let remotes: Vec<_> =
                                self.room.remote_participants().keys().cloned().collect();
                            (Some(identity), remotes)
                        }
                        None => {
                            let remotes: Vec<_> =
                                self.room.remote_participants().keys().cloned().collect();
                            if remotes.len() == 1 {
                                log::debug!(
                                    "[echo_server] DataReceived without participant, using sole remote {:?}",
                                    remotes[0]
                                );
                                (Some(remotes[0].clone()), remotes)
                            } else if remotes.is_empty() {
                                log::debug!(
                                    "[echo_server] DataReceived without participant (remotes=0), buffering"
                                );
                                let bytes =
                                    Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());
                                self.pending_data.lock().await.push_back(bytes);
                                continue;
                            } else {
                                log::warn!(
                                    "DataReceived without participant identity (remotes={}), ignoring",
                                    remotes.len()
                                );
                                continue;
                            }
                        }
                    };
                    log::info!(
                        "[echo_server] RPC received from {:?} ({} bytes)",
                        event_participant,
                        payload.len()
                    );
                    rpc_trace!(
                        "LiveKitParticipant: incoming RPC payload from {:?} ({} bytes)",
                        event_participant,
                        payload.len()
                    );
                    let bridge = self.bridge.clone();
                    let local = self.room.local_participant();
                    let active_streams = self.active_streams.clone();
                    let payload = Arc::try_unwrap(payload).unwrap_or_else(|a| (*a).clone());

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_incoming(
                            &payload,
                            event_participant,
                            &remote_identities,
                            &bridge,
                            &local,
                            &active_streams,
                        )
                        .await
                        {
                            log::error!("RPC handle error: {}", e);
                        }
                    });
                }
                _ => {}
            }
        }
        log::debug!("LiveKitParticipant::run event loop ended");
    }

    async fn handle_incoming(
        payload: &[u8],
        event_participant: Option<ParticipantIdentity>,
        remote_identities: &[ParticipantIdentity],
        bridge: &RpcBridge<S>,
        local: &LocalParticipant,
        active_streams: &Mutex<HashMap<i32, ActiveStream>>,
    ) -> Result<(), String> {
        let request = decode_request(payload)?;
        let request_id = request.request_id;
        let meta = request.call_metadata.as_ref();
        let end_of_stream = request.end_of_stream;
        rpc_trace!(
            "LiveKitParticipant::handle_incoming request_id={} {}/{} end_of_stream={} event_participant={:?}",
            request_id,
            meta.map(|m| m.service.as_str()).unwrap_or("?"),
            meta.map(|m| m.method.as_str()).unwrap_or("?"),
            end_of_stream,
            event_participant
        );

        let to_process: Option<(Vec<RpcRequest>, ParticipantIdentity)> = {
            let mut streams = active_streams.lock().await;
            if end_of_stream {
                if let Some(mut stream) = streams.remove(&request_id) {
                    stream.messages.push(request);
                    Some((stream.messages, stream.sender_identity))
                } else {
                    let response_identity = resolve_response_identity(
                        &request,
                        event_participant.clone(),
                        remote_identities,
                    )
                    .ok_or_else(|| {
                        "no response destination (sender_identity absent, multiple remotes)"
                            .to_string()
                    })?;
                    Some((vec![request], response_identity))
                }
            } else {
                if request.call_metadata.is_some() {
                    let sender_identity = resolve_response_identity(
                        &request,
                        event_participant.clone(),
                        remote_identities,
                    )
                    .ok_or_else(|| "no response destination for stream (sender_identity absent, multiple remotes)".to_string())?;
                    streams.insert(
                        request_id,
                        ActiveStream {
                            sender_identity,
                            messages: vec![request],
                        },
                    );
                } else if let Some(stream) = streams.get_mut(&request_id) {
                    stream.messages.push(request);
                }
                None
            }
        };

        let Some((messages, response_identity)) = to_process else {
            return Ok(());
        };

        let result = bridge.handle_decoded_requests(&messages).await;

        match result {
            Ok(chunks) => {
                let len = chunks.len();
                log::info!(
                    "[echo_server] RPC request_id={} response sent ({} chunk(s))",
                    request_id,
                    len
                );
                rpc_trace!(
                    "LiveKitParticipant: request_id={} produced {} response chunk(s)",
                    request_id,
                    len
                );
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
                        destination_identities: vec![response_identity.clone()],
                    };
                    local
                        .publish_data(packet)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Err(status) => {
                log::info!(
                    "[echo_server] RPC request_id={} error response: {}",
                    request_id,
                    status
                );
                let response = response_from_result(request_id, Err(status));
                let encoded = encode_response(response)?;
                let packet = DataPacket {
                    payload: encoded,
                    topic: Some(RPC_TOPIC.to_string()),
                    reliable: true,
                    destination_identities: vec![response_identity],
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

/// Resolve which participant identity to send responses to.
/// Prefers sender_identity from the request when present and non-empty; otherwise falls back to event participant or sole remote.
pub(crate) fn resolve_response_identity(
    request: &RpcRequest,
    event_participant: Option<ParticipantIdentity>,
    remote_identities: &[ParticipantIdentity],
) -> Option<ParticipantIdentity> {
    if let Some(ref s) = request.sender_identity {
        if !s.is_empty() {
            return Some(s.clone().into());
        }
    }
    event_participant.or_else(|| {
        if remote_identities.len() == 1 {
            Some(remote_identities[0].clone())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::CallMetadata;

    #[test]
    fn resolve_response_identity_uses_sender_identity_from_request_when_present() {
        let request = RpcRequest {
            request_id: 1,
            request_message: vec![],
            call_metadata: Some(CallMetadata {
                service: "test.EchoService".to_string(),
                method: "Echo".to_string(),
            }),
            metadata: None,
            end_of_stream: true,
            abort: false,
            sender_identity: Some("client1".to_string()),
        };
        let event_participant = Some("client2".to_string().into());
        let remote_identities = vec!["client1".to_string().into(), "client2".to_string().into()];
        let result = resolve_response_identity(&request, event_participant, &remote_identities);
        assert_eq!(
            result.as_ref().map(|p| p.as_str()),
            Some("client1"),
            "response must be sent to sender_identity from request, not event participant"
        );
    }
}
