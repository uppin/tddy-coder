//! Room-wide, topic-scoped publish/subscribe over the LiveKit data channel.
//!
//! Every other publish in this crate is a *unicast*: [`crate::client::RpcClient`] fills
//! `destination_identities` with the one peer it is calling, and every receiver hard-filters on the
//! single `tddy-rpc` topic. A broadcast inverts both halves — it leaves `destination_identities`
//! empty so the server fans the packet out to whoever is connected, and it carries its own topic so
//! the RPC receivers drop it instead of trying to decode it as an envelope.
//!
//! The two directions are deliberately independent: a [`BroadcastChannel`] used only to publish
//! never calls `room.subscribe()`, and a subscriber owns its own event stream rather than sharing
//! one with the RPC loops.

use std::sync::Arc;

use livekit::prelude::*;
use tokio::sync::mpsc;

/// Largest payload a single broadcast may carry.
///
/// Well under the ~64 KB the SCTP data channel negotiates (see [`crate::chunking`]), because the
/// budget here is not "what fits on the wire" but "what a snapshot-shaped event should ever be".
pub const MAX_BROADCAST_PAYLOAD_BYTES: usize = 8 * 1024;

/// Why a broadcast did not go out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BroadcastError {
    /// The payload exceeded [`MAX_BROADCAST_PAYLOAD_BYTES`] and was refused, not split.
    PayloadTooLarge { bytes: usize, limit: usize },
    /// The room rejected the publish. Carries the underlying `RoomError`'s `Display` text rather
    /// than the error itself: `RoomError` is not comparable, and callers here only ever log or
    /// surface the message.
    Publish(String),
}

impl std::fmt::Display for BroadcastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PayloadTooLarge { bytes, limit } => {
                write!(
                    f,
                    "broadcast payload of {bytes} bytes exceeds the {limit} byte limit"
                )
            }
            Self::Publish(message) => write!(f, "broadcast publish failed: {message}"),
        }
    }
}

impl std::error::Error for BroadcastError {}

/// One broadcast received on the channel's topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastMessage {
    /// Identity of the participant that published it. `None` when the server delivered the packet
    /// without attributing a sender — the same gap [`crate::participant`] works around for RPC.
    pub from: Option<String>,
    pub payload: Vec<u8>,
}

/// Where a [`BroadcastPublisher`] finds the participant to publish through.
///
/// Never a `LocalParticipant` snapshot. A room that reconnects — on token refresh, or after the
/// signalling channel drops — publishes through a different one afterwards, and a handle captured
/// once would from then on hand every packet to a connection that is gone, failing quietly for as
/// long as the publisher lives.
enum PublishTarget {
    /// A `Room` the publisher's owner still holds; `local_participant()` is re-read per publish.
    Room(Arc<Room>),
    /// A connection whose `Room` was handed to [`crate::LiveKitParticipant::run`], which consumes
    /// it for as long as it serves. The serving side keeps this updated with the participant of
    /// whatever connection is current, so a publisher outlives any single one of them without
    /// opening a second connection — which for a session room would put a second identity in a
    /// room whose whole claim is that only the facilitating daemon is in it.
    Connection(crate::participant::SharedPublisher),
}

/// The publishing half of a topic, on a connection someone else is serving.
///
/// Obtained from [`crate::JoinedParticipant::broadcast_on`] (the serving side) or from
/// [`BroadcastChannel`] (a plain `Room` handle); both publish through the same code below.
pub struct BroadcastPublisher {
    target: PublishTarget,
    topic: String,
}

impl BroadcastPublisher {
    /// Publish through a `Room` handle the caller keeps.
    pub(crate) fn for_room(room: Arc<Room>, topic: impl Into<String>) -> Self {
        Self {
            target: PublishTarget::Room(room),
            topic: topic.into(),
        }
    }

    /// Publish through a served connection, following it across reconnects.
    pub(crate) fn for_connection(
        publisher: crate::participant::SharedPublisher,
        topic: impl Into<String>,
    ) -> Self {
        Self {
            target: PublishTarget::Connection(publisher),
            topic: topic.into(),
        }
    }

    /// Publish `payload` to every participant in the room on this publisher's topic.
    ///
    /// Reliable, so subscribers see the topic's messages in publish order — that ordering is what
    /// makes "the event implies the metadata already changed" hold for a receiver.
    ///
    /// An oversized payload is **refused, never chunk-framed**. [`crate::chunking`] exists precisely
    /// to split large messages, and this deliberately does not use it: chunking is only safe when a
    /// lost frame eventually surfaces as a failed call, and a fire-and-forget broadcast has no reply
    /// to time out. A half-delivered chunked broadcast would leave every subscriber waiting on a
    /// message that never completes and never errors, with nothing to retry it.
    ///
    /// This is the only place a broadcast is sized and framed, so both ways of obtaining a
    /// publisher get the same refusal.
    pub async fn publish(&self, payload: &[u8]) -> Result<(), BroadcastError> {
        if payload.len() > MAX_BROADCAST_PAYLOAD_BYTES {
            return Err(BroadcastError::PayloadTooLarge {
                bytes: payload.len(),
                limit: MAX_BROADCAST_PAYLOAD_BYTES,
            });
        }
        // The empty `destination_identities` is the whole difference from an RPC publish: it is what
        // makes the server fan this out to participants the publisher never named — including ones
        // that joined after the publisher did and are unknown to it.
        match &self.target {
            PublishTarget::Room(room) => {
                let packet = DataPacket {
                    payload: payload.to_vec(),
                    topic: Some(self.topic.clone()),
                    reliable: true,
                    destination_identities: vec![],
                };
                room.local_participant()
                    .publish_data(packet)
                    .await
                    .map_err(|e| BroadcastError::Publish(e.to_string()))
            }
            PublishTarget::Connection(publisher) => publisher
                .publish_data(payload.to_vec(), &self.topic, &[])
                .await
                .map_err(BroadcastError::Publish),
        }
    }
}

/// A single topic on a room's data channel, usable to publish, to subscribe, or both.
pub struct BroadcastChannel {
    room: Arc<Room>,
    topic: String,
}

impl BroadcastChannel {
    pub fn new(room: Arc<Room>, topic: impl Into<String>) -> Self {
        Self {
            room,
            topic: topic.into(),
        }
    }

    /// Publish `payload` to every participant in the room on this channel's topic, through the
    /// same [`BroadcastPublisher::publish`] the serving side uses.
    pub async fn publish(&self, payload: &[u8]) -> Result<(), BroadcastError> {
        BroadcastPublisher::for_room(self.room.clone(), self.topic.clone())
            .publish(payload)
            .await
    }

    /// Stream the broadcasts published on this channel's topic.
    ///
    /// Takes its own `room.subscribe()` handle: the RPC loops consume the events they receive, and a
    /// subscriber that shared one with them would race them for each event. Everything on another
    /// topic — `tddy-rpc` above all — is dropped here, so RPC traffic on the same data channel is
    /// never mistaken for a broadcast.
    ///
    /// The forwarding task ends when the room's event stream closes or the receiver is dropped, so
    /// dropping the returned receiver is how a caller unsubscribes.
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<BroadcastMessage> {
        let mut events = self.room.subscribe();
        let topic = self.topic.clone();
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let RoomEvent::DataReceived {
                    payload,
                    topic: packet_topic,
                    kind: _,
                    participant,
                } = event
                else {
                    continue;
                };
                if packet_topic.as_deref() != Some(topic.as_str()) {
                    continue;
                }
                let message = BroadcastMessage {
                    from: participant.as_ref().map(|p| p.identity().to_string()),
                    // Take the bytes when this subscription is the last holder of them, which it
                    // usually is: copying every broadcast for the benefit of the case where the SDK
                    // still shares the buffer would pay for the exception on every message.
                    payload: Arc::try_unwrap(payload).unwrap_or_else(|shared| (*shared).clone()),
                };
                if tx.send(message).is_err() {
                    log::debug!("BroadcastChannel: subscriber for topic {topic} went away");
                    return;
                }
            }
            log::debug!("BroadcastChannel: room event stream for topic {topic} ended");
        });

        rx
    }
}
