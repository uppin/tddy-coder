//! The daemon's Prometheus metrics on a [`DaemonSessionHost`]: installed by the composition
//! root, read when a `LaunchSessions` is built from the host for a single call.

use std::sync::Arc;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the metrics session starts are counted on (builder pattern). `None` — the
    /// default — leaves starts uncounted: a host with no `metrics:` section, and every test
    /// host, which asserts outcomes rather than counters.
    pub fn with_metrics(mut self, metrics: Option<Arc<tddy_metrics::DaemonMetrics>>) -> Self {
        self.metrics = metrics;
        self
    }

    /// The metrics installed with [`Self::with_metrics`].
    pub(crate) fn metrics(&self) -> Option<Arc<tddy_metrics::DaemonMetrics>> {
        self.metrics.clone()
    }
}
