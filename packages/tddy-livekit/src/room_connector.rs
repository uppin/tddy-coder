//! LiveKit's [`RoomConnector`]: joins over [`crate::client_connect::connect_client`], minting the
//! participant's token when it is given API credentials, and wraps the joined room as a
//! [`RoomSession`] — calls through [`crate::RpcClient`], duplexes through its bidi streams,
//! broadcasts through [`crate::BroadcastChannel`].

use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::{ParticipantIdentity, Room};
use tddy_room_client::{
    RoomAuth, RoomConnectError, RoomConnector, RoomDuplex, RoomJoin, RoomSession,
};
use tddy_rpc::Status;
use tokio::sync::mpsc;

use crate::client_connect::{connect_client, ConnectError};
use crate::{BroadcastChannel, RpcClient, TokenGenerator};

/// Frames buffered between a [`RoomDuplex`]'s caller and the room.
const DUPLEX_BUFFER_FRAMES: usize = 64;

/// Joins LiveKit rooms.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveKitRoomConnector;

#[async_trait::async_trait]
impl RoomConnector for LiveKitRoomConnector {
    async fn connect(
        &self,
        join: &RoomJoin,
        peer_identity: &str,
        wait: Duration,
    ) -> Result<Arc<dyn RoomSession>, RoomConnectError> {
        let token = match &join.auth {
            RoomAuth::Token(token) => token.clone(),
            RoomAuth::Credentials {
                api_key,
                api_secret,
                ttl,
            } => TokenGenerator::new(
                api_key.clone(),
                api_secret.clone(),
                join.room.clone(),
                join.identity.clone(),
                *ttl,
            )
            .generate()
            .map_err(|e| {
                RoomConnectError::Room(format!(
                    "could not mint a LiveKit token for room \"{}\": {e}",
                    join.room
                ))
            })?,
        };

        let connected = connect_client(&join.url, &token, peer_identity, wait)
            .await
            .map_err(|e| match e {
                ConnectError::Room(reason) => RoomConnectError::Room(reason),
                ConnectError::ParticipantAbsent { identity, waited } => {
                    RoomConnectError::PeerAbsent { identity, waited }
                }
            })?;

        Ok(Arc::new(LiveKitRoomSession {
            room: connected.room,
            client: Arc::new(connected.client),
            peer: peer_identity.to_string().into(),
        }))
    }
}

/// A joined LiveKit room, addressed at one participant.
///
/// Holds the `Arc<Room>`: the connection lives exactly as long as the session does, and dropping the
/// last handle leaves the room.
struct LiveKitRoomSession {
    room: Arc<Room>,
    client: Arc<RpcClient>,
    peer: ParticipantIdentity,
}

impl RoomSession for LiveKitRoomSession {
    fn rpc(&self) -> Arc<dyn tddy_rpc::RpcClientTransport> {
        Arc::clone(&self.client) as Arc<dyn tddy_rpc::RpcClientTransport>
    }

    /// The stream is opened and pumped by a task that owns the client, because the sender half
    /// borrows it. A half-closed `outbound` stops sending and keeps delivering what the peer still
    /// sends; the task ends when the peer ends the stream or `inbound` is dropped.
    fn open_duplex(&self, service: &str, method: &str) -> Result<RoomDuplex, Status> {
        let (outbound, mut frames_to_send) = mpsc::channel::<Vec<u8>>(DUPLEX_BUFFER_FRAMES);
        let (replies, inbound) = mpsc::channel::<Result<Vec<u8>, Status>>(DUPLEX_BUFFER_FRAMES);
        let client = Arc::clone(&self.client);
        let service = service.to_string();
        let method = method.to_string();

        tokio::spawn(async move {
            let (mut sender, mut peer_frames) = match client.start_bidi_stream(&service, &method) {
                Ok(stream) => stream,
                Err(status) => {
                    let _ = replies.send(Err(status)).await;
                    return;
                }
            };
            let mut sending = true;
            loop {
                tokio::select! {
                    frame = frames_to_send.recv(), if sending => match frame {
                        Some(bytes) => {
                            if let Err(status) = sender.send(bytes, false).await {
                                let _ = replies.send(Err(status)).await;
                                return;
                            }
                        }
                        None => sending = false,
                    },
                    reply = peer_frames.recv() => match reply {
                        Some(reply) => {
                            if replies.send(reply).await.is_err() {
                                return;
                            }
                        }
                        None => return,
                    },
                }
            }
        });

        Ok(RoomDuplex { outbound, inbound })
    }

    fn subscribe(&self, topic: &str) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let mut messages = BroadcastChannel::new(Arc::clone(&self.room), topic).subscribe();
        let (payloads, subscription) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(message) = messages.recv().await {
                if payloads.send(message.payload).is_err() {
                    return;
                }
            }
        });
        subscription
    }

    fn peer_present(&self) -> bool {
        self.room.remote_participants().contains_key(&self.peer)
    }
}
