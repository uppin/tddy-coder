//! Joining a room and talking in it, from the client side, as ports.
//!
//! A client tool — a worktree mirror, the git SSH shim, a terminal relay, the in-session tool
//! client — joins a room under its own identity, waits for the participant it talks to, and then
//! calls RPCs (through [`tddy_rpc::RpcClientTransport`]), holds a live duplex stream, and listens to
//! broadcast topics. Its library does all of that through [`RoomConnector`]; its binary hands it the
//! implementation (LiveKit's lives in `tddy-livekit`).

use std::sync::Arc;
use std::time::Duration;

use tddy_rpc::Status;
use tokio::sync::mpsc;

#[cfg(feature = "test-support")]
pub mod in_memory;

/// How a client proves it may join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomAuth {
    /// Mint a token for this join from API credentials, valid for `ttl`.
    Credentials {
        api_key: String,
        api_secret: String,
        ttl: Duration,
    },
    /// A token minted elsewhere.
    Token(String),
}

/// One join: where, as whom, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomJoin {
    pub url: String,
    pub room: String,
    pub identity: String,
    pub auth: RoomAuth,
}

/// Why a join did not produce a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomConnectError {
    /// The room itself could not be joined (bad URL, rejected token, unreachable server).
    Room(String),
    /// The room was joined, but the participant to talk to never appeared.
    PeerAbsent { identity: String, waited: Duration },
}

impl std::fmt::Display for RoomConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoomConnectError::Room(reason) => write!(f, "could not join the room: {reason}"),
            RoomConnectError::PeerAbsent { identity, waited } => write!(
                f,
                "\"{identity}\" did not join the room within {}s",
                waited.as_secs()
            ),
        }
    }
}

impl std::error::Error for RoomConnectError {}

/// A live bidirectional stream: send frames as they come, receive replies as they arrive.
pub struct RoomDuplex {
    pub outbound: mpsc::Sender<Vec<u8>>,
    pub inbound: mpsc::Receiver<Result<Vec<u8>, Status>>,
}

/// A joined room, addressed at one peer.
pub trait RoomSession: Send + Sync {
    /// Calls to the peer.
    fn rpc(&self) -> Arc<dyn tddy_rpc::RpcClientTransport>;
    /// Open a live duplex stream to the peer's `service/method`.
    fn open_duplex(&self, service: &str, method: &str) -> Result<RoomDuplex, Status>;
    /// Messages broadcast on `topic` in the room, from now on.
    fn subscribe(&self, topic: &str) -> mpsc::UnboundedReceiver<Vec<u8>>;
    /// Whether the peer is still in the room.
    fn peer_present(&self) -> bool;
}

/// Joins rooms.
#[async_trait::async_trait]
pub trait RoomConnector: Send + Sync {
    /// Join as `join` describes and wait up to `wait` for `peer_identity` to be in the room.
    async fn connect(
        &self,
        join: &RoomJoin,
        peer_identity: &str,
        wait: Duration,
    ) -> Result<Arc<dyn RoomSession>, RoomConnectError>;
}

/// No transport wired: every join fails, naming what is missing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRoomTransport;

#[async_trait::async_trait]
impl RoomConnector for NoRoomTransport {
    async fn connect(
        &self,
        _join: &RoomJoin,
        _peer_identity: &str,
        _wait: Duration,
    ) -> Result<Arc<dyn RoomSession>, RoomConnectError> {
        Err(RoomConnectError::Room(
            "no room transport is wired into this build".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn with_nothing_wired_connecting_fails_naming_the_missing_transport() {
        let refused = NoRoomTransport
            .connect(
                &RoomJoin {
                    url: "ws://10.0.0.5:7880".to_string(),
                    room: "session-abc".to_string(),
                    identity: "syncer-abc-1".to_string(),
                    auth: RoomAuth::Token("t".to_string()),
                },
                "daemon-host",
                Duration::from_secs(1),
            )
            .await
            .err()
            .expect("nothing can join");

        assert_eq!(
            refused,
            RoomConnectError::Room("no room transport is wired into this build".to_string())
        );
    }
}
