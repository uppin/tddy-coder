//! Peer routing lives in [`tddy_host_service::peer_routing`], which routes for every daemon and
//! names no LiveKit crate. What this crate adds is building one from the common-room handles its
//! discovery loop produces.

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
pub use tddy_host_service::peer_routing::PeerRouting;

use crate::livekit_peer_discovery::LiveKitDiscoveryHandles;
use crate::LiveKitPeerForwarder;

/// Routing over the common room `discovery` describes, or [`PeerRouting::local_only`] when there is
/// none: no discovery means this machine is the only host.
///
/// The forwarder is built here, where both the plugin and the routing are in scope: the routing
/// takes the built room and names no transport itself.
#[must_use]
pub fn peer_routing_over(
    config: &DaemonConfig,
    discovery: Option<LiveKitDiscoveryHandles>,
) -> PeerRouting {
    match discovery {
        Some(handles) => PeerRouting::new(
            config.clone(),
            handles.eligible_daemon_source,
            Some(CommonRoom::new(
                LiveKitPeerForwarder::new(handles.common_room_livekit_room),
                config,
            )),
        ),
        None => PeerRouting::local_only(config),
    }
}
