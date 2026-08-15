//! Join a room and address an RPC client at the participant that serves it.
//!
//! Every rust→rust LiveKit client does the same three things before it can make a call: connect the
//! room, wait for the serving participant to be visible (it may not have rejoined yet after a
//! restart), and vend a client through [`LiveKitRpcClientFactory`] so every client on the room
//! shares one request-id space and one response loop. That sequence lives here so a new client is
//! one call rather than a copy.

use std::sync::Arc;
use std::time::Duration;

use livekit::prelude::*;

use crate::client::RpcClient;
use crate::client_factory::LiveKitRpcClientFactory;

/// Why a client could not be pointed at a serving participant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// The room itself could not be joined (bad URL, rejected token, unreachable server).
    Room(String),
    /// The room was joined, but the participant that serves the RPC never appeared.
    ParticipantAbsent { identity: String, waited: Duration },
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Room(reason) => write!(f, "livekit room connect: {reason}"),
            ConnectError::ParticipantAbsent { identity, waited } => write!(
                f,
                "\"{identity}\" did not join the room within {}s",
                waited.as_secs()
            ),
        }
    }
}

/// A joined room and a client addressed at the participant serving it.
///
/// The `room` is handed back because the connection lives exactly as long as this `Arc`: dropping
/// it leaves the room, so a caller that keeps only the `client` would be disconnected mid-call.
pub struct ConnectedClient {
    pub room: Arc<Room>,
    pub client: RpcClient,
}

/// Connect to `url` as `token`'s identity, wait up to `wait_for_participant` for `target_identity`
/// to be in the room, and return a client addressed at it.
pub async fn connect_client(
    url: &str,
    token: &str,
    target_identity: &str,
    wait_for_participant: Duration,
) -> Result<ConnectedClient, ConnectError> {
    let (room, mut events) = Room::connect(url, token, RoomOptions::default())
        .await
        .map_err(|e| ConnectError::Room(e.to_string()))?;
    let room = Arc::new(room);

    let target: ParticipantIdentity = target_identity.to_string().into();
    if !room.remote_participants().contains_key(&target) {
        // The inner block reports *why* it stopped waiting. Returning `()` would make an exhausted
        // event channel — the room dropped out from under us — indistinguishable from the
        // participant arriving, and this function would hand back a client addressed at somebody
        // who is not there.
        let appeared = tokio::time::timeout(wait_for_participant, async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::ParticipantConnected(participant) = event {
                    if participant.identity() == target {
                        return true;
                    }
                }
            }
            false
        })
        .await;
        match appeared {
            Ok(true) => {}
            Ok(false) => {
                return Err(ConnectError::Room(format!(
                    "the room's event stream ended while waiting for \"{target_identity}\""
                )))
            }
            Err(_) => {
                return Err(ConnectError::ParticipantAbsent {
                    identity: target_identity.to_string(),
                    waited: wait_for_participant,
                })
            }
        }
    }

    let client = LiveKitRpcClientFactory::for_room(Arc::clone(&room)).client(target);
    Ok(ConnectedClient { room, client })
}
