//! LiveKit's [`PeerForwarder`]: a forwarded call travels as a data-channel RPC in the common room.
//!
//! The kernel's `CommonRoom` applies the deadline and the stream idle rule; this only obtains a
//! client addressed at `daemon-{peer_id}` and carries the call — and, for a stream, keeps that client
//! alive for the stream's whole life, since it holds the call's registration in the room's shared
//! response loop.

use std::sync::Arc;

use livekit::Room;
use tddy_daemon_extensions::PeerForwarder;
use tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity;
use tddy_livekit::{LiveKitRpcClientFactory, RpcClient};
use tddy_rpc::Status;
use tokio::sync::{mpsc, RwLock};

/// The common room's slot: `Some` while this daemon is connected to it.
pub type CommonRoomSlot = Arc<RwLock<Option<Arc<Room>>>>;

/// Frames the relay of one forwarded stream buffers; matches the transport channel it reads.
/// Independent of the transport's own depth: it adds one buffering hop, it does not replace it.
const STREAM_RELAY_DEPTH: usize = 32;

/// Forwards over the common room in `slot`.
pub struct LiveKitPeerForwarder {
    slot: CommonRoomSlot,
}

impl LiveKitPeerForwarder {
    pub fn new(slot: CommonRoomSlot) -> Arc<Self> {
        Arc::new(Self { slot })
    }

    /// An RPC client addressed at a peer daemon's [`daemon_rpc_identity`], drawn from the common
    /// room's shared client factory.
    ///
    /// The factory keeps **one** request-id registry and **one** response loop per room connection,
    /// regardless of how many peers are forwarded to or how often. Building a client per call (as
    /// `forward_to_peer` once did) leaked a `subscribe()` loop each time.
    async fn peer_client(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        what: &str,
    ) -> Result<RpcClient, Status> {
        let room_arc = {
            let g = self.slot.read().await;
            g.clone()
        }
        .ok_or_else(|| {
            Status::failed_precondition(format!(
                "LiveKit common room is not connected on this daemon; cannot forward {what} to a peer"
            ))
        })?;
        let target = daemon_rpc_identity(peer_id);
        log::debug!(
            "peer_client: peer_id={} target_identity={} service={} method={} ({what})",
            peer_id,
            target,
            service,
            method
        );
        Ok(LiveKitRpcClientFactory::for_room(room_arc).client(target))
    }
}

#[async_trait::async_trait]
impl PeerForwarder for LiveKitPeerForwarder {
    async fn is_connected(&self) -> bool {
        self.slot.read().await.is_some()
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let client = self.peer_client(peer_id, service, method, "an RPC").await?;
        client.call_unary(service, method, body).await
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let client = self
            .peer_client(peer_id, service, method, "a stream")
            .await?;
        let mut frames = client.call_server_stream(service, method, body).await?;

        // The client carries this call's registration in the room's shared engine; hold it for the
        // stream's whole life so the response loop keeps delivering frames to it. Frames pass
        // through a channel of their own, so this relay adds one buffering hop. It does not isolate
        // the room: the transport's channel (32 frames) is filled by the shared response loop with
        // `send().await`, so a reader that stops draining eventually blocks that loop for every
        // in-flight call (head-of-line). Callers must drain promptly or drop the receiver.
        let (tx, rx) = mpsc::channel(STREAM_RELAY_DEPTH);
        tokio::spawn(async move {
            let _client = client;
            loop {
                // A reader that went away (the kernel's idle timeout drops it) ends the relay even
                // when the peer never sends another frame, releasing the client with it.
                let frame = tokio::select! {
                    frame = frames.recv() => frame,
                    () = tx.closed() => break,
                };
                let Some(frame) = frame else { break };
                if tx.send(frame).await.is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}
