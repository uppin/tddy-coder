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

use axum::{http::StatusCode, routing::get, Router};
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::{Family, MetricConstructor};
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::metrics::info::Info;
use prometheus_client::registry::{Registry, Unit};

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

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StartLabels {
    result: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ActiveLabels {
    kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct RpcLabels {
    service: String,
    method: String,
    code: String,
}

/// The RPC-duration buckets (Go-client defaults): 5 ms to 10 s, covering a fast local call to a
/// slow proxied one.
#[derive(Clone, Debug)]
struct RpcDurationBuckets;

impl MetricConstructor<Histogram> for RpcDurationBuckets {
    fn new_metric(&self) -> Histogram {
        Histogram::new([
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ])
    }
}

/// The daemon's metric families.
#[derive(Debug)]
pub struct DaemonMetrics {
    registry: Registry,
    session_starts: Family<StartLabels, Counter>,
    sessions_active: Family<ActiveLabels, Gauge>,
    rpc_requests: Family<RpcLabels, Counter>,
    rpc_durations: Family<RpcLabels, Histogram, RpcDurationBuckets>,
    livekit_rooms: Gauge,
    open_fds: Gauge,
}

impl DaemonMetrics {
    /// A registry holding every initial family, `tddy_build_info{version}` set to 1.
    #[must_use]
    pub fn new(build_version: &str) -> Self {
        let mut registry = Registry::default();

        let build_info = Info::new(vec![(
            "version".to_string(),
            build_version.to_string(),
        )]);
        registry.register(
            "tddy_build_info",
            "The daemon build this registry belongs to, as a version label.",
            build_info,
        );

        let open_fds = Gauge::default();
        registry.register(
            "process_open_fds",
            "Open file descriptors of the daemon process, sampled at scrape time.",
            open_fds.clone(),
        );

        let session_starts = Family::<StartLabels, Counter>::default();
        registry.register(
            "tddy_session_starts_total",
            "Session starts by outcome.",
            session_starts.clone(),
        );

        let sessions_active = Family::<ActiveLabels, Gauge>::default();
        registry.register(
            "tddy_sessions_active",
            "Sessions currently held by kind.",
            sessions_active.clone(),
        );

        let rpc_requests = Family::<RpcLabels, Counter>::default();
        registry.register(
            "tddy_rpc_requests_total",
            "RPC calls by service, method and status code.",
            rpc_requests.clone(),
        );

        // Registered without the unit suffix: the registry appends `_seconds` itself.
        let rpc_durations =
            Family::<RpcLabels, Histogram, RpcDurationBuckets>::new_with_constructor(
                RpcDurationBuckets,
            );
        registry.register_with_unit(
            "tddy_rpc_request_duration",
            "RPC dispatch latency by service, method and status code.",
            Unit::Seconds,
            rpc_durations.clone(),
        );

        let livekit_rooms = Gauge::default();
        registry.register(
            "tddy_livekit_rooms_connected",
            "LiveKit rooms this daemon currently has open.",
            livekit_rooms.clone(),
        );

        Self {
            registry,
            session_starts,
            sessions_active,
            rpc_requests,
            rpc_durations,
            livekit_rooms,
            open_fds,
        }
    }

    pub fn session_started(&self, result: StartResult) {
        let result = match result {
            StartResult::Started => "started",
            StartResult::Failed => "failed",
        };
        self.session_starts.get_or_create(&StartLabels { result }).inc();
    }

    pub fn set_sessions_active(&self, kind: &str, count: i64) {
        self.sessions_active
            .get_or_create(&ActiveLabels {
                kind: kind.to_string(),
            })
            .set(count);
    }

    pub fn rpc_completed(&self, call: &RpcCall<'_>, elapsed: Duration) {
        let labels = RpcLabels {
            service: call.service.to_string(),
            method: call.method.to_string(),
            code: call.code.to_string(),
        };
        self.rpc_requests.get_or_create(&labels).inc();
        self.rpc_durations
            .get_or_create(&labels)
            .observe(elapsed.as_secs_f64());
    }

    pub fn set_livekit_rooms_connected(&self, count: i64) {
        self.livekit_rooms.set(count);
    }

    /// The registry in the OpenMetrics text format, with `process_open_fds` sampled now — so a
    /// direct call and an HTTP scrape read the same current value.
    #[must_use]
    pub fn encode(&self) -> String {
        self.open_fds.set(count_open_fds() as i64);
        let mut buffer = String::new();
        encode(&mut buffer, &self.registry).expect("encoding metrics to a string cannot fail");
        buffer
    }
}

#[cfg(unix)]
fn count_open_fds() -> u64 {
    // The read itself holds one fd while counting, so this reads one high — fine for a gauge.
    std::fs::read_dir("/proc/self/fd")
        .map(|entries| entries.count() as u64)
        .unwrap_or(0)
}

#[cfg(not(unix))]
fn count_open_fds() -> u64 {
    0
}

/// The private metrics listener.
#[derive(Debug)]
pub struct MetricsServer {
    listener: tokio::net::TcpListener,
    metrics: Arc<DaemonMetrics>,
}

impl MetricsServer {
    /// Bind `addr` (port 0 picks a free one).
    pub async fn bind(addr: SocketAddr, metrics: Arc<DaemonMetrics>) -> io::Result<Self> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { listener, metrics })
    }

    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("a bound listener has a local address")
    }

    /// Serve `GET /metrics` (everything else 404) until `shutdown` resolves.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        let metrics = Arc::clone(&self.metrics);
        let app = Router::new()
            .route(
                "/metrics",
                get(move || {
                    let metrics = Arc::clone(&metrics);
                    async move {
                        (
                            [(
                                axum::http::header::CONTENT_TYPE,
                                "text/plain; version=0.0.4",
                            )],
                            metrics.encode(),
                        )
                    }
                }),
            )
            .fallback(|| async { (StatusCode::NOT_FOUND, "not found") });
        axum::serve(self.listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(io::Error::other)
    }
}
