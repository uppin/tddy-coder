//! Family P — `pr_stack.PrStackService`.

mod branch_legs;
mod guards;
mod ports;
mod pr_status;

pub use guards::validate_repoint_target;

use std::path::PathBuf;
use std::sync::Arc;

use prost::Message;
use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_session_lifecycle::connection_service::{wire_same, DaemonSessionHost};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::relay_idle::RpcActivity;

/// The eight PR-stack RPCs: the caller's identity, the sessions and stack plans under
/// `tddy_data_dir`, the vaults the GitHub token a PR status is read with is sealed in, and the peer an orchestrator one
/// host over is reached through.
pub struct PrStackRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    credential_vaults: Option<Arc<SessionVaults>>,
    rpc_activity: RpcActivity,
    peer_routing: PeerRouting,
}

impl PrStackRpcHandler {
    /// A handler sharing `host`'s state — the same credential vaults, idle tracker and peer routing.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            credential_vaults: host.credential_vaults(),
            rpc_activity: host.rpc_activity(),
            peer_routing: host.peer_routing(),
        }
    }
}

/// Same as [`wire_same`] for blocking closures that return `anyhow::Result`.
fn wire_same_anyhow<Src: Message, Dst: Message + Default>(src: &Src) -> anyhow::Result<Dst> {
    wire_same(src).map_err(|s| anyhow::anyhow!(s.to_string()))
}

#[cfg(test)]
mod add_planned_pr_unit_tests;
