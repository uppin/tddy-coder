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

use std::sync::{Arc, Weak};

use tddy_agent_launch::launch_ports::LaunchSessions;
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

    /// Receive, once, a launch snapshot of the host these families were installed on
    /// ([`DaemonSessionHost::with_rpc_families`]). The snapshot reaches the host's full session
    /// start and list paths; it refers back to the families only weakly, so the families holding it
    /// form no `Arc` cycle. Families that never answer a session start need nothing here.
    fn attach_session_start(&self, _launch: LaunchSessions) {}
}

impl DaemonSessionHost {
    /// Install the families. The composition root calls this **last**, after every other `with_*`:
    /// the handlers were built from this host's state and share it, and a `with_*` applied
    /// afterwards would leave them holding the value it replaced.
    ///
    /// The families are handed a launch snapshot of this host, which is what a family that answers a
    /// session start late-binds to. The snapshot holds its families weakly: the live host holds the
    /// strong reference, and a snapshot is only ever reached through the families, so it never keeps
    /// them alive.
    #[must_use]
    pub fn with_rpc_families(mut self, families: Arc<dyn DaemonRpcFamilies>) -> Self {
        let mut snapshot = self.clone();
        snapshot.rpc_families = Some(Arc::new(WeakRpcFamilies(Arc::downgrade(&families))));
        families.attach_session_start(snapshot.launch_sessions());
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

/// The families as a launch snapshot holds them: forwarding to the installed families without owning
/// them. Only the live host owns the strong reference, so a snapshot cannot keep its own families
/// alive. A snapshot is reached only through the families it forwards to, so the forward is live
/// whenever it is used.
struct WeakRpcFamilies(Weak<dyn DaemonRpcFamilies>);

impl WeakRpcFamilies {
    fn installed(&self) -> Arc<dyn DaemonRpcFamilies> {
        self.0
            .upgrade()
            .expect("the RPC families a session start reads were dropped with their session host")
    }
}

impl DaemonRpcFamilies for WeakRpcFamilies {
    fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler> {
        self.installed().pr_stack_handler()
    }

    fn service_entries(&self) -> Vec<tddy_rpc::ServiceEntry> {
        self.installed().service_entries()
    }
}
