//! Family P — `work_queue.WorkQueueService`.

mod branch_legs;
mod guards;
pub(crate) mod move_activity;
mod ports;
mod pr_status;
mod project_stacks;

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

/// The ten PR-stack RPCs: the caller's identity, the sessions and stack plans under
/// `tddy_data_dir`, the vaults the GitHub token a PR status is read with is sealed in, and the peer an orchestrator one
/// host over is reached through.
pub struct WorkQueueRpcHandler {
    config: DaemonConfig,
    user_resolver: SessionUserResolver,
    tddy_data_dir: PathBuf,
    credential_vaults: Option<Arc<SessionVaults>>,
    /// The `gh` a Local gh orchestrator's repoint reads its token from.
    github_cli: tddy_host_service::GithubCli,
    rpc_activity: RpcActivity,
    peer_routing: PeerRouting,
    /// The terminals and jails this daemon holds, which say which sessions are live.
    terminals: tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore,
    /// The host's GitHub inbox and the project credentials it reads a repository with
    /// (`#workqueue` 6/14): PR status, the status stream and an assigned PR are read through it.
    pull_requests: Arc<dyn tddy_worktree_service::pull_request_view::SessionPullRequests>,
    /// Where a move of a work item is published for its queue's owner (`#workqueue` 8/14); `None`
    /// on a host with no bus.
    notification_bus:
        Option<Arc<tddy_session_activity::session_notifications::SessionNotificationBus>>,
}

impl WorkQueueRpcHandler {
    /// A handler sharing `host`'s state — the same credential vaults, idle tracker and peer routing.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            config: host.config().clone(),
            user_resolver: host.user_resolver(),
            tddy_data_dir: host.tddy_data_dir().to_path_buf(),
            credential_vaults: host.credential_vaults(),
            github_cli: host.github_cli(),
            rpc_activity: host.rpc_activity(),
            peer_routing: host.peer_routing(),
            terminals: host.terminal_sessions(),
            pull_requests: Arc::new(host.clone()),
            notification_bus: host.session_notification_bus(),
        }
    }
}

/// Same as [`wire_same`] for blocking closures that return `anyhow::Result`.
fn wire_same_anyhow<Src: Message, Dst: Message + Default>(src: &Src) -> anyhow::Result<Dst> {
    wire_same(src).map_err(|s| anyhow::anyhow!(s.to_string()))
}

#[cfg(test)]
mod add_work_item_unit_tests;

mod annotations;
pub use annotations::parse_tone;
mod links;
#[cfg(test)]
mod repoint_github_token_unit_tests;
mod status_stream;
mod work_queue_queries;
