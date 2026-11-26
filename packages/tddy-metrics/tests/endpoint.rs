//! The private listener serves the daemon's metrics, and nothing else.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tddy_metrics::{DaemonMetrics, MetricsServer, RpcCall, StartResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const INITIAL_FAMILIES: [&str; 7] = [
    "tddy_build_info",
    "process_open_fds",
    "tddy_sessions_active",
    "tddy_session_starts_total",
    "tddy_rpc_requests_total",
    "tddy_rpc_request_duration_seconds",
    "tddy_livekit_rooms_connected",
];

async fn a_running_metrics_listener(metrics: Arc<DaemonMetrics>) -> SocketAddr {
    let server = MetricsServer::bind("127.0.0.1:0".parse().unwrap(), metrics)
        .await
        .expect("the metrics listener binds");
    let addr = server.local_addr();
    tokio::spawn(server.serve(std::future::pending()));
    addr
}

async fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connects");
    let request = format!("GET {path} HTTP/1.1\r\nHost: metrics\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_string(&mut response))
        .await
        .expect("answers in time")
        .unwrap();
    response
}

#[tokio::test]
async fn the_metrics_listener_serves_every_initial_family() {
    let addr = a_running_metrics_listener(Arc::new(DaemonMetrics::new("1.2.3"))).await;

    let response = get(addr, "/metrics").await;

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    for family in INITIAL_FAMILIES {
        assert!(
            response.contains(family),
            "missing {family} in:\n{response}"
        );
    }
    assert!(response.contains("version=\"1.2.3\""), "{response}");
}

#[tokio::test]
async fn only_the_metrics_path_is_served() {
    let addr = a_running_metrics_listener(Arc::new(DaemonMetrics::new("1.2.3"))).await;

    assert!(get(addr, "/metrics").await.starts_with("HTTP/1.1 200"));
    assert!(get(addr, "/").await.starts_with("HTTP/1.1 404"));
    assert!(get(addr, "/rpc").await.starts_with("HTTP/1.1 404"));
}

#[test]
fn an_rpc_call_is_counted_by_service_method_and_code() {
    let metrics = DaemonMetrics::new("1.2.3");
    let call = RpcCall {
        service: "session.SessionService",
        method: "ListSessions",
        code: "ok",
    };

    metrics.rpc_completed(&call, Duration::from_millis(12));
    metrics.rpc_completed(&call, Duration::from_millis(30));

    let text = metrics.encode();
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_rpc_requests_total")
                && l.contains("service=\"session.SessionService\"")
                && l.contains("method=\"ListSessions\"")
                && l.contains("code=\"ok\"")
                && l.ends_with(" 2")),
        "{text}"
    );
    assert!(
        text.contains("tddy_rpc_request_duration_seconds_count"),
        "{text}"
    );
}

#[test]
fn session_starts_and_active_sessions_are_reported() {
    let metrics = DaemonMetrics::new("1.2.3");

    metrics.session_started(StartResult::Started);
    metrics.session_started(StartResult::Failed);
    metrics.set_sessions_active("agent", 3);
    metrics.set_livekit_rooms_connected(1);

    let text = metrics.encode();
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_session_starts_total")
                && l.contains("result=\"started\"")
                && l.ends_with(" 1")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_session_starts_total")
                && l.contains("result=\"failed\"")
                && l.ends_with(" 1")),
        "{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("tddy_sessions_active")
            && l.contains("kind=\"agent\"")
            && l.ends_with(" 3")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_livekit_rooms_connected") && l.ends_with(" 1")),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn open_file_descriptors_are_sampled_at_scrape_time() {
    let metrics = DaemonMetrics::new("1.2.3");
    let fds: u64 = metrics
        .encode()
        .lines()
        .find(|l| l.starts_with("process_open_fds "))
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok())
        .expect("process_open_fds has a value");
    assert!(fds >= 3, "stdin/stdout/stderr at least, got {fds}");
}
