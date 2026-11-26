//! The daemon's Prometheus metrics.
//!
//! [`DaemonMetrics`] owns one registry per daemon instance (no global state, so two in-process
//! daemons in a test never share counters). [`MetricsServer`] serves it as `GET /metrics` on a
//! listener of its own — `metrics.listen` in `daemon.yaml` — never on the public web port.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// How a session start ended, for `tddy_session_starts_total{result}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartResult {
    Started,
    Failed,
}

/// One RPC call, for `tddy_rpc_requests_total` and `tddy_rpc_request_duration_seconds`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCall<'a> {
    pub service: &'a str,
    pub method: &'a str,
    /// The Connect / gRPC status code name, e.g. `ok`, `not_found`.
    pub code: &'a str,
}

/// The daemon's metric families.
#[derive(Debug, Default)]
pub struct DaemonMetrics {
    // TODO(daemon-metrics): implement — the registry and the seven families.
}

impl DaemonMetrics {
    /// A registry holding every initial family, `tddy_build_info{version}` set to 1.
    #[must_use]
    pub fn new(_build_version: &str) -> Self {
        // TODO(daemon-metrics): implement
        Self::default()
    }

    pub fn session_started(&self, _result: StartResult) {
        // TODO(daemon-metrics): implement
    }

    pub fn set_sessions_active(&self, _kind: &str, _count: i64) {
        // TODO(daemon-metrics): implement
    }

    pub fn rpc_completed(&self, _call: &RpcCall<'_>, _elapsed: Duration) {
        // TODO(daemon-metrics): implement
    }

    pub fn set_livekit_rooms_connected(&self, _count: i64) {
        // TODO(daemon-metrics): implement
    }

    /// The registry in the OpenMetrics text format, with `process_open_fds` sampled now.
    #[must_use]
    pub fn encode(&self) -> String {
        // TODO(daemon-metrics): implement
        String::new()
    }
}

/// The private metrics listener.
#[derive(Debug)]
pub struct MetricsServer {
    local_addr: SocketAddr,
}

impl MetricsServer {
    /// Bind `addr` (port 0 picks a free one).
    pub async fn bind(_addr: SocketAddr, _metrics: Arc<DaemonMetrics>) -> io::Result<Self> {
        // TODO(daemon-metrics): implement
        Err(io::Error::other("TODO(daemon-metrics): implement"))
    }

    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Serve `GET /metrics` (everything else 404) until `shutdown` resolves.
    pub async fn serve(
        self,
        _shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        // TODO(daemon-metrics): implement
        Err(io::Error::other("TODO(daemon-metrics): implement"))
    }
}
