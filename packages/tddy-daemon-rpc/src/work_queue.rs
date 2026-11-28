//! Family P — `work_queue.WorkQueueService`.

mod blocking_lane;
#[cfg(test)]
mod blocking_lane_unit_tests;
mod branch_legs;
mod branch_reads;
pub(crate) mod chain_health;
mod guards;
mod item_files;
mod item_writes;
pub(crate) mod move_activity;
mod ports;
mod pr_status;
mod project_stacks;
mod pull_base_rpc;
mod repoint_rpc;
mod warn_gate;
#[cfg(test)]
mod warn_gate_unit_tests;

pub use guards::validate_repoint_target;
/// `#fs-port` seam: [`referencing_sessions_by_stack`](project_stacks::referencing_sessions_by_stack) through the port, for `MemFs` tests.
pub use project_stacks::referencing_sessions_by_stack_on;

use std::path::PathBuf;
use std::sync::Arc;

use prost::Message;
use tddy_credentials::SessionVaults;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;
use tddy_session_activity::session_notifications::SessionNotificationBus;
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
    /// What an armed schedule's watcher signs its own short-lived tokens with (`#workqueue` 14/14);
    /// `None` on a daemon that signs nothing.
    session_token_signer: Option<tddy_github::SessionTokenSigner>,
    /// Where a move of a work item is published for its queue's owner (`#workqueue` 8/14); `None`
    /// on a host with no bus.
    notification_bus: Option<Arc<SessionNotificationBus>>,
    /// The bounded, coalescing lanes `QueryBranch`'s blocking legs run in, so its polling cannot
    /// fill the blocking pool the rest of the daemon needs.
    query_branch_lanes: blocking_lane::QueryBranchLanes,
    /// Where this host stages pre-session uploads — the staged arm of an attach fetches from the
    /// root the uploader wrote to.
    staging_base_dir: std::path::PathBuf,
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
            session_token_signer: host.session_token_signer(),
            notification_bus: host.session_notification_bus(),
            query_branch_lanes: blocking_lane::QueryBranchLanes::new(),
            staging_base_dir: host.staging_base_dir(),
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
pub(crate) mod commit_flow;
pub(crate) mod land;
mod lineage;
mod links;
mod queue_changes;
mod queue_watch;
#[cfg(test)]
mod repoint_github_token_unit_tests;
pub(crate) mod schedule_watch;
pub(crate) mod schedules;
mod status_stream;
#[cfg(test)]
mod status_stream_wake_unit_tests;
mod waves;
mod work_queue_queries;
