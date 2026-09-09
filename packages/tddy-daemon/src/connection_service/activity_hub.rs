//! The `StreamSessionActivity` relay and the demo-VM handle it shares a module with.
//!
//! The hub the relay reads from is [`tddy_daemon_kernel::AgentActivityHub`], not a type declared
//! here: the sandbox subsystem publishes into it, and a sandbox crate that had to depend on the
//! daemon's RPC entry point to publish an activity record would defeat the extraction entirely.

use tddy_service::proto::connection::AgentActivityRecord as ProtoAgentActivityRecord;
use tokio::sync::broadcast::error::RecvError;

/// Relay task for `StreamSessionActivity`: forwards live agent-activity records for one session
/// (the broadcast is already session-scoped) from the hub into `tx` until the client disconnects.
pub(crate) async fn relay_agent_activity(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        tddy_core::agent_activity::AgentActivityRecord,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<ProtoAgentActivityRecord>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

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
