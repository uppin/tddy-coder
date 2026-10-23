//! Family L — `exec_tools.ExecToolService`.

mod path_guard;
mod ports;
mod project_home;
mod result_frames;
mod role_guard;

pub use result_frames::EXEC_TOOL_FRAME_BYTES;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_host_service::GithubCli;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
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
    /// The host's full session start and list paths, late-bound when the RPC families are attached
    /// to the host (`#project-home` 5/7). Unset on a handler built from a host that was never given
    /// its families, and a project-home tool that needs it then refuses.
    session_launch: Arc<OnceLock<LaunchSessions>>,
    /// The vaults a project-home git tool reads a target's GitHub identity from (`#project-home` 7/7).
    credential_vaults: Option<Arc<SessionVaults>>,
    github_cli: GithubCli,
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
            session_launch: Arc::new(OnceLock::new()),
            credential_vaults: host.credential_vaults(),
            github_cli: host.github_cli(),
        }
    }

    /// The late-bound launch cell, shared with the families that attach it.
    pub(crate) fn session_launch_cell(&self) -> Arc<OnceLock<LaunchSessions>> {
        Arc::clone(&self.session_launch)
    }
}
