use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use tddy_host_service::multi_host::EligibleDaemonInfo;

use tddy_host_service::multi_host::EligibleDaemonSource;

use tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry;

use tddy_daemon_kernel::config::DaemonConfig;
use tokio::sync::mpsc;

use super::STREAM_RELAY_DEPTH;

use tddy_daemon_kernel::peer_forwarding::daemon_rpc_identity;

use tddy_rpc::RpcClientTransport;

use tddy_rpc::Status;

use std::sync::Arc;

use super::RoomHub;

/// Forwards through a hub room, addressed the way the kernel's `CommonRoom` addresses peers:
/// a call for `peer_id` goes to the participant `daemon-{peer_id}`.
pub struct HubPeerForwarder {
    pub(crate) hub: RoomHub,
    pub(crate) room: String,
    pub(crate) from_identity: String,
}

impl HubPeerForwarder {
    pub fn new(
        hub: RoomHub,
        room: impl Into<String>,
        from_identity: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            hub,
            room: room.into(),
            from_identity: from_identity.into(),
        })
    }
}

#[async_trait::async_trait]
impl tddy_daemon_extensions::PeerForwarder for HubPeerForwarder {
    async fn is_connected(&self) -> bool {
        true
    }

    async fn call_unary(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        self.hub
            .client(
                &self.room,
                &self.from_identity,
                &daemon_rpc_identity(peer_id),
            )
            .call_unary(service, method, body)
            .await
    }

    async fn call_server_stream(
        &self,
        peer_id: &str,
        service: &str,
        method: &str,
        body: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        let mut frames = self
            .hub
            .client(
                &self.room,
                &self.from_identity,
                &daemon_rpc_identity(peer_id),
            )
            .call_server_stream(service, method, body)
            .await?;
        let (tx, rx) = mpsc::channel(STREAM_RELAY_DEPTH);
        tokio::spawn(async move {
            loop {
                let frame = tokio::select! {
                    frame = frames.recv() => frame,
                    _ = tx.closed() => break,
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

/// An [`EligibleDaemonSource`] over a hub room: this daemon plus every daemon an attached
/// registry can see in it, merged the way the LiveKit source merges them.
pub struct HubEligibleDaemonSource {
    pub(crate) config: Arc<DaemonConfig>,
    pub(crate) registry: Arc<CommonRoomPeerRegistry>,
    pub(crate) common_room: tddy_daemon_kernel::peer_forwarding::CommonRoom,
}

impl HubEligibleDaemonSource {
    pub fn new(
        config: Arc<DaemonConfig>,
        registry: Arc<CommonRoomPeerRegistry>,
        common_room: tddy_daemon_kernel::peer_forwarding::CommonRoom,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            registry,
            common_room,
        })
    }

    /// The forwarding room this source routes through, for a `PeerRouting` over the same room.
    pub fn common_room(&self) -> tddy_daemon_kernel::peer_forwarding::CommonRoom {
        self.common_room.clone()
    }
}

impl EligibleDaemonSource for HubEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        let local = tddy_host_service::multi_host::eligible_daemon_entry_for(
            tddy_host_service::multi_host::DaemonInstanceId(local_instance_id_for_config(
                &self.config,
            )),
        );
        let remotes = self.registry.snapshot_remotes();
        tddy_daemon_livekit::livekit_peer_discovery::merge_discovered_peers_ordered(local, remotes)
            .unwrap_or_else(|e| {
                log::warn!(
                    "merge_discovered_peers_ordered failed: {} — returning local only",
                    e
                );
                vec![tddy_host_service::multi_host::eligible_daemon_entry_for(
                    tddy_host_service::multi_host::DaemonInstanceId(local_instance_id_for_config(
                        &self.config,
                    )),
                )]
            })
    }
}
