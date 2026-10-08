//! The per-session demo-VM handle.
//!
//! The `StreamSessionActivity` relay this module was named for left with families M and N in
//! `#unbundle` node 7 ([`tddy_session_activity::streams::relay_agent_activity`]). The module name
//! stays so the `DemoVmHandle` beside it keeps its path.

/// Per-session QEMU demo VM lifecycle state.
pub enum DemoVmHandle {
    /// Boot has been requested; waiting for SSH port to become reachable.
    Booting,
    /// VM is up and accepting SSH connections.
    /// `share_url` is the first app port forward URL (e.g. "http://localhost:8080"), if any.
    Running {
        vm: tddy_vm::RunningVm,
        share_url: String,
    },
    /// Boot or shutdown failed.
    Error(String),
}

/// What the demo-VM RPCs read of the host that serves them: the per-session VM table, the data
/// root and the config their sessions base is resolved from, the session-token resolver, and the
/// idle tracker an RPC bumps.
///
/// Built from the host's fields by `demo_vm_service_state` and held by `DemoVmServiceImpl`, so the
/// handlers run without the host.
#[derive(Clone)]
pub struct DemoVmState {
    /// Per-session demo VM state — keyed by session_id.
    pub demo_vm_state:
        std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, DemoVmHandle>>>,
    pub tddy_data_dir: std::path::PathBuf,
    pub user_resolver: tddy_daemon_kernel::SessionUserResolver,
    pub rpc_activity: tddy_daemon_kernel::relay_idle::RpcActivity,
    pub config: tddy_daemon_kernel::config::DaemonConfig,
}
