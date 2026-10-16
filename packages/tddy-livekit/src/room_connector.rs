//! LiveKit's [`RoomConnector`]: joins over [`crate::client_connect::connect_client`], minting the
//! participant's token when it is given API credentials, and wraps the joined room as a
//! [`RoomSession`] — calls through [`crate::RpcClient`], duplexes through its bidi streams,
//! broadcasts through [`crate::BroadcastChannel`].

use std::sync::Arc;
use std::time::Duration;

use tddy_room_client::{RoomConnectError, RoomConnector, RoomJoin, RoomSession};

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
        let _ = (join, peer_identity, wait);
        todo!("LiveKitRoomConnector::connect")
    }
}
