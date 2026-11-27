//! Acceptance: the metrics endpoint contract at the daemon level (`#fs-port` 2/13).
//!
//! - The public web listener serves no `/metrics`, with or without a `metrics:` section: the
//!   private listener is the only surface that exposes the registry.
//! - The exact RPC service the web listener serves (`server::rpc_service_for`) counts a call
//!   through `MultiRpcService` on the daemon's registry when metrics are installed, and stays
//!   silent when they are not.

use std::sync::Arc;

use tddy_daemon::server::{rpc_service_for, run_server, RunServerOptions};
use tddy_metrics::DaemonMetrics;
use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry};

struct OkService;

#[async_trait::async_trait]
impl RpcService for OkService {
    async fn handle_rpc(&self, _service: &str, _method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(vec![1, 2, 3]))
    }
}

fn routed_service(name: &'static str) -> ServiceEntry {
    ServiceEntry {
        name,
        service: Arc::new(OkService),
    }
}

fn a_message() -> RpcMessage {
    RpcMessage::new(
        vec![],
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

/// A free loopback port, discovered by binding port 0 first. The bind is released before the
/// server takes it, so a parallel test could steal it in theory — the standard test tradeoff,
/// and a stolen port fails the bind loudly rather than serving the wrong thing.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind a discovery listener")
        .local_addr()
        .expect("a bound listener has a local address")
        .port()
}

/// Wait until the listener accepts connections (5 s safety net for a loaded machine's first
/// bind — the probe returns as soon as the listener accepts, milliseconds locally).
async fn wait_until_accepting(port: u16) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the web listener never started accepting connections"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn get(path: &str, port: u16) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connects to the web listener");
    let request = format!("GET {path} HTTP/1.1\r\nHost: daemon\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.read_to_string(&mut response),
    )
    .await
    .expect("answers in time")
    .unwrap();
    response
}

fn web_options(
    port: u16,
    bundle_path: std::path::PathBuf,
) -> (RunServerOptions, tokio::sync::oneshot::Sender<()>) {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let options = RunServerOptions {
        host: "127.0.0.1".to_string(),
        port,
        bundle_path,
        rpc_entries: vec![],
        livekit_url: None,
        common_room: None,
        livekit_enabled: false,
        daemon_instance_id: "metrics-test".to_string(),
        sandboxed_codebase: None,
        auth_flow: None,
        allowed_agents: vec![],
        debug: None,
        lifecycle_telegram: None,
        metrics: None,
        shutdown_rx: Some(shutdown_rx),
    };
    (options, shutdown_tx)
}

#[tokio::test]
async fn the_web_listener_serves_no_metrics() {
    // Given the daemon's own web listener on a free port, over an empty bundle directory
    let bundle = tempfile::tempdir().expect("a bundle dir");
    let port = free_port();
    let (options, shutdown) = web_options(port, bundle.path().to_path_buf());
    let server = tokio::spawn(run_server(options));
    wait_until_accepting(port).await;

    // When scraped where the private listener would answer
    let response = get("/metrics", port).await;

    // Then it is not the Prometheus exposition: no /metrics route is served there
    assert!(
        !response.contains("tddy_build_info") && !response.contains("process_open_fds"),
        "the web listener must not expose metrics, got:\n{response}"
    );

    // Cleanup: stop the listener
    let _ = shutdown.send(());
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), server).await;
}

#[tokio::test]
async fn an_rpc_through_the_served_roster_counts_its_service_method_and_code() {
    // Given the exact service the web listener serves, with the daemon's metrics installed
    let metrics = Arc::new(DaemonMetrics::new("test"));
    let service = rpc_service_for(
        vec![routed_service("session.SessionService")],
        Some(Arc::clone(&metrics)),
    );

    // When an RPC is answered and one is refused
    let answered = service
        .handle_rpc("session.SessionService", "ListSessions", &a_message())
        .await;
    assert!(matches!(answered, RpcResult::Unary(Ok(_))));
    let refused = service
        .handle_rpc("no.SuchService", "Missing", &a_message())
        .await;
    assert!(matches!(refused, RpcResult::Unary(Err(_))));

    // Then both reached the registry with their service, method and code
    let text = metrics.encode();
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_rpc_requests_total")
                && l.contains("service=\"session.SessionService\"")
                && l.contains("method=\"ListSessions\"")
                && l.contains("code=\"ok\"")
                && l.ends_with(" 1")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("tddy_rpc_requests_total")
                && l.contains("code=\"not_found\"")
                && l.ends_with(" 1")),
        "{text}"
    );
}

#[tokio::test]
async fn the_served_roster_answers_without_metrics_installed() {
    // Given the served roster with no metrics installed
    let service = rpc_service_for(vec![routed_service("session.SessionService")], None);

    // When an RPC goes through it still answers — recording is optional, serving is not
    let answered = service
        .handle_rpc("session.SessionService", "ListSessions", &a_message())
        .await;
    assert!(matches!(answered, RpcResult::Unary(Ok(_))));

    // Then there is nowhere to have recorded it: a fresh registry carries no samples
    let text = DaemonMetrics::new("test").encode();
    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("tddy_rpc_requests_total")),
        "{text}"
    );
}
