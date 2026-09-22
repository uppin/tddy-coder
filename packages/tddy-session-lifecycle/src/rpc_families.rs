//! The port through which session code reaches the RPC families that live above this crate.
//!
//! The Project, Catalog, ExecTool and PR-stack families are served by `tddy-daemon-rpc`, which
//! depends on this crate. Two things here still need them:
//!
//! - a session start whose stack parent is owned by a peer, or which names a stack node, asks the
//!   PR-stack handler — the same path a client's `ResolveStackBase` / `LinkStackNode` takes, peer
//!   routing and refusals included;
//! - a session room serves every family a room's agents may call, these four among them.
//!
//! This crate cannot name the handlers, so the composition root installs them here. A host that
//! was never given them **refuses** the two paths rather than quietly serving a room with four
//! families missing or skipping a stack link; the late-bound `OnceLock` shape was avoided on
//! purpose, since its unwired state is what `docs/dev/todo/2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md`
//! records failing seventeen tests.

use std::sync::Arc;

use tddy_rpc::Status;

use crate::connection_service::DaemonSessionHost;
use crate::PrStackHandler;

/// The RPC families a [`DaemonSessionHost`] reaches without depending on the crate that serves
/// them.
pub trait DaemonRpcFamilies: Send + Sync {
    /// The PR-stack handler a session start routes its stack-base and stack-link questions
    /// through.
    fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler>;

    /// The families' transport entries, as a session room serves them.
    fn service_entries(&self) -> Vec<tddy_rpc::ServiceEntry>;
}

impl DaemonSessionHost {
    /// Install the families. The composition root calls this **last**, after every other `with_*`:
    /// the handlers were built from this host's state and share it, and a `with_*` applied
    /// afterwards would leave them holding the value it replaced.
    #[must_use]
    pub fn with_rpc_families(self, families: Arc<dyn DaemonRpcFamilies>) -> Self {
        // TODO(#carve 11): store the port on the host.
        let _ = families;
        todo!("DaemonSessionHost::with_rpc_families")
    }

    /// The installed families, or `FAILED_PRECONDITION` naming the missing wiring.
    pub fn rpc_families(&self) -> Result<&Arc<dyn DaemonRpcFamilies>, Status> {
        // TODO(#carve 11): read the port the host was given.
        todo!("DaemonSessionHost::rpc_families")
    }
}
