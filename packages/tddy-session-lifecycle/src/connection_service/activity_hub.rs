//! The per-session demo-VM handle.
//!
//! The `StreamSessionActivity` relay this module was named for left with families M and N in
//! `#unbundle` node 7 ([`tddy_session_activity::streams::relay_agent_activity`]). The module name
//! stays so the `DemoVmHandle` beside it keeps its path.

/// Per-session QEMU demo VM lifecycle state.
pub(crate) enum DemoVmHandle {
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
