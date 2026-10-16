//! Which daemon serves an addressed request, and the common room a request for a peer is forwarded
//! through — beside the [`EligibleDaemonSource`] it routes by. LiveKit-free: the room forwards
//! through whatever transport it was built with.

use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::{CommonRoom, PeerRoute};
use tddy_rpc::Status;

use crate::multi_host::EligibleDaemonSource;

/// Routes requests to the daemon they address.
pub struct PeerRouting {
    config: DaemonConfig,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    common_room: Option<CommonRoom>,
}

impl PeerRouting {
    pub fn new(
        config: DaemonConfig,
        eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
        common_room: Option<CommonRoom>,
    ) -> Self {
        Self {
            config,
            eligible_daemon_source,
            common_room,
        }
    }

    /// `Local` for this daemon (or no daemon named), `Forward` for an eligible peer; an unknown id
    /// is refused.
    pub fn classify_daemon_route(&self, requested_daemon: &str) -> Result<PeerRoute, Status> {
        let _ = (requested_daemon, &self.config, &self.eligible_daemon_source);
        todo!("PeerRouting::classify_daemon_route")
    }

    /// The common room to forward `rpc_name` through, or a precondition failure without one.
    pub fn common_room_slot(&self, rpc_name: &str) -> Result<&CommonRoom, Status> {
        let _ = (rpc_name, &self.common_room);
        todo!("PeerRouting::common_room_slot")
    }
}
