//! LiveKit's [`PeerForwarder`]: a forwarded call travels as a data-channel RPC in the common room.
//!
//! The kernel's `CommonRoom` applies the deadline and the stream idle rule; this only obtains a
//! client addressed at `daemon-{peer_id}` and carries the call — and, for a stream, keeps that client
//! alive for the stream's whole life, since it holds the call's registration in the room's shared
//! response loop.

use std::sync::Arc;

use livekit::Room;
use tddy_daemon_extensions::PeerForwarder;
use tddy_rpc::Status;
use tokio::sync::{mpsc, RwLock};

/// The common room's slot: `Some` while this daemon is connected to it.
pub type CommonRoomSlot = Arc<RwLock<Option<Arc<Room>>>>;

/// Forwards over the common room in `slot`.
pub struct LiveKitPeerForwarder {
    slot: CommonRoomSlot,
}

impl LiveKitPeerForwarder {
    pub fn new(slot: CommonRoomSlot) -> Arc<Self> {
        let _ = slot;
        todo!("LiveKitPeerForwarder::new")
    }
}

#[async_trait::async_trait]
impl PeerForwarder for LiveKitPeerForwarder {
    async fn is_connected(&self) -> bool {
        let _ = &self.slot;
        todo!("LiveKitPeerForwarder::is_connected")
    }

    async fn call_unary(
        &self,
        _peer_id: &str,
        _service: &str,
        _method: &str,
        _body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        todo!("LiveKitPeerForwarder::call_unary")
    }

    async fn call_server_stream(
        &self,
        _peer_id: &str,
        _service: &str,
        _method: &str,
        _body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        todo!("LiveKitPeerForwarder::call_server_stream")
    }
}
