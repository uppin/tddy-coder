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
    pub fn with_rpc_families(mut self, families: Arc<dyn DaemonRpcFamilies>) -> Self {
        self.rpc_families = Some(families);
        self
    }

    /// Guard for a `with_*`/`set_*` that feeds state the installed handlers were built from: once
    /// the families are installed they hold the earlier value, so changing it here would split the
    /// host and its handlers. A wiring-order bug, caught where it is made.
    pub(crate) fn debug_assert_rpc_families_not_installed(&self, setter: &str) {
        debug_assert!(
            self.rpc_families.is_none(),
            "`{setter}` applied after `with_rpc_families`: apply every `with_*` before the RPC \
             families are installed; the handlers were built from the earlier state"
        );
    }

    /// The installed families, or `FAILED_PRECONDITION` naming the missing wiring.
    pub fn rpc_families(&self) -> Result<&Arc<dyn DaemonRpcFamilies>, Status> {
        self.rpc_families.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon's RPC families were never wired: the composition root did not install \
                 `DaemonRpcFamilies` on its session host (`with_rpc_families`), so a session room \
                 or a stack link that needs them cannot be served",
            )
        })
    }
}
