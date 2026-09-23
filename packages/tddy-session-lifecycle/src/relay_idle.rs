//! Idle-timeout tracker for relay daemon mode.
//!
//! The implementation now lives in `tddy-task` ([`tddy_task::IdleTimeoutTracker`]) so it can
//! be shared with other long-running subsystems (e.g. the reusable-LSP registry). This
//! re-export keeps the historical `crate::relay_idle::IdleTimeoutTracker` path stable.

use std::sync::Arc;

pub use tddy_task::IdleTimeoutTracker;

/// What every RPC handler bumps so a relay daemon does not shut itself down mid-session: the
/// daemon's idle tracker, when it has one.
///
/// Shared, not copied: `Clone` hands out the same tracker, so the session host and every family
/// handler built from it (`tddy-daemon-rpc`) record onto the one clock the relay's idle monitor
/// reads.
#[derive(Clone, Default)]
pub struct RpcActivity {
    idle_tracker: Option<Arc<IdleTimeoutTracker>>,
}

impl RpcActivity {
    /// Activity recorded onto `tracker`.
    #[must_use]
    pub fn on(tracker: Arc<IdleTimeoutTracker>) -> Self {
        Self {
            idle_tracker: Some(tracker),
        }
    }

    /// Record one RPC call, if a tracker is attached.
    pub fn record(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }
}
