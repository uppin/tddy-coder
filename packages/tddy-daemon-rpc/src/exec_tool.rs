//! Family L — `exec_tools.ExecToolService`.

mod path_guard;
mod ports;
mod result_frames;

pub use result_frames::EXEC_TOOL_FRAME_BYTES;

use std::path::PathBuf;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_session_lifecycle::connection_service::{DaemonSessionHost, LocalExecTools};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::relay_idle::RpcActivity;

/// The four exec-tool RPCs: the caller's identity, the peer a split session's tools are forwarded
/// to, and the three places a tool actually runs — this host's task registry, a workspace sandbox,
/// or a hosted agent clone.
pub struct ExecToolRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    peer_routing: PeerRouting,
    rpc_activity: RpcActivity,
    local_exec_tools: LocalExecTools,
}

impl ExecToolRpcHandler {
    /// A handler sharing `host`'s state — the same task registry, sandboxes and hosted clones, so
    /// a tool it runs is one the rest of the daemon can see.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            peer_routing: host.peer_routing(),
            rpc_activity: host.rpc_activity(),
            local_exec_tools: host.local_exec_tools(),
        }
    }
}
