//! Acceptance: `run_server` takes its wiring as a `RunServerOptions` struct and serves exactly
//! the surface it served as twelve positional arguments — the bundle, the SPA fallback,
//! `/api/config` and the ConnectRPC route.
//!
//! The `docs/dev/todo/` entry that asked for this is closed and removed; the reasoning lives in
//! `docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md`.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tddy_coder::web_server::ClientAllowedAgent;
use tddy_daemon::server::{run_server, RunServerOptions};
use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry};
use tddy_testing_commons::wait::eventually_awaiting;
use tempfile::TempDir;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// A bundle directory with the two files a served page is made of: the SPA entry point and one
/// asset it would load. Both carry a marker so a response can be attributed to a specific file.
fn a_web_bundle() -> TempDir {
    let bundle = tempfile::tempdir().expect("no temp directory for the bundle");
    std::fs::create_dir(bundle.path().join("assets"))
        .expect("the assets directory was not created");
    std::fs::write(
        bundle.path().join("index.html"),
        "<!doctype html><title>tddy</title>",
    )
    .expect("index.html was not written");
    std::fs::write(
        bundle.path().join("assets/app.js"),
        "export const app = 'tddy';",
    )
    .expect("the bundle asset was not written");
    bundle
}

/// Options that serve `bundle` on loopback with the full client-visible configuration a daemon
/// with LiveKit switched on would carry, so `/api/config` has something meaningful to serve.
fn options_serving(bundle: &Path, port: u16) -> RunServerOptions {
    RunServerOptions {
        host: "127.0.0.1".to_string(),
        port,
        bundle_path: bundle.to_path_buf(),
        rpc_entries: vec![],
        livekit_url: Some("wss://livekit.example".to_string()),
        common_room: Some("tddy-lobby".to_string()),
        livekit_enabled: true,
        daemon_instance_id: "udoo".to_string(),
        allowed_agents: vec![ClientAllowedAgent {
            id: "codex-acp".to_string(),
            label: "Codex ACP".to_string(),
        }],
        debug: Some("tddy:term:*".to_string()),
        lifecycle_telegram: None,
        shutdown_rx: None,
    }
}

/// A port that is free at this moment, so the server under test binds where the test looks.
async fn a_free_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available");
    listener
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

/// A running `run_server`: where to reach it, and how to stop it.
struct RunningServer {
    host: String,
    port: u16,
    stop: oneshot::Sender<()>,
    served: JoinHandle<anyhow::Result<()>>,
}

impl RunningServer {
    fn url(&self, path: &str) -> String {
        format!("http://{}:{}{}", self.host, self.port, path)
    }

    /// Fires the shutdown channel the options carried and asserts the server exited cleanly.
    async fn stop(self) {
        self.stop
            .send(())
            .expect("the server stopped listening for shutdown");
        self.served
            .await
            .expect("the server task panicked")
            .expect("run_server did not exit cleanly");
    }
}

/// `run_server` running with `options`, once it answers on the port they name.
///
/// The harness replaces `options.shutdown_rx` with its own channel, because stopping the server
/// is how every test in this file ends.
async fn a_server_running_with(options: RunServerOptions) -> RunningServer {
    let (stop, shutdown_rx) = oneshot::channel();
    let host = options.host.clone();
    let port = options.port;
    let served = tokio::spawn(run_server(RunServerOptions {
        shutdown_rx: Some(shutdown_rx),
        ..options
    }));

    // 5s: a safety net for a loaded machine's first bind, not an expected duration — the probe
    // returns as soon as the listener accepts, which is milliseconds locally.
    let address = format!("{host}:{port}");
    eventually_awaiting(
        "the server to accept connections",
        Duration::from_secs(5),
        || {
            let address = address.clone();
            async move {
                let reached: Result<(), String> = TcpStream::connect(&address)
                    .await
                    .map(|_| ())
                    .map_err(|error| format!("connecting to {address} failed: {error}"));
                reached
            }
        },
    )
    .await;

    RunningServer {
        host,
        port,
        stop,
        served,
    }
}

/// A service that answers every method with the request body, so the RPC route can be proven
/// without standing up a real daemon service.
struct EchoService;

#[async_trait]
impl RpcService for EchoService {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(message.payload.clone()))
    }
}

async fn body_of(url: String) -> String {
    reqwest::get(&url)
        .await
        .unwrap_or_else(|error| panic!("GET {url} failed: {error}"))
        .text()
        .await
        .expect("the response had no readable body")
}

#[tokio::test]
async fn serves_the_bundle_index_at_the_root_path() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(options_serving(bundle.path(), port)).await;

    // When
    let page = body_of(server.url("/")).await;

    // Then
    assert_eq!(page, "<!doctype html><title>tddy</title>");
    server.stop().await;
}

#[tokio::test]
async fn serves_a_bundle_asset_at_its_own_path() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(options_serving(bundle.path(), port)).await;

    // When
    let asset = body_of(server.url("/assets/app.js")).await;

    // Then
    assert_eq!(asset, "export const app = 'tddy';");
    server.stop().await;
}

#[tokio::test]
async fn falls_back_to_the_bundle_index_for_a_client_side_route() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(options_serving(bundle.path(), port)).await;

    // When
    let page = body_of(server.url("/sessions/some-session-id")).await;

    // Then
    assert_eq!(page, "<!doctype html><title>tddy</title>");
    server.stop().await;
}

#[tokio::test]
async fn serves_the_client_configuration_the_options_carry_at_api_config() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(options_serving(bundle.path(), port)).await;

    // When
    let config: serde_json::Value = serde_json::from_str(&body_of(server.url("/api/config")).await)
        .expect("/api/config did not serve JSON");

    // Then
    assert_eq!(
        config,
        json!({
            "livekit_url": "wss://livekit.example",
            "common_room": "tddy-lobby",
            "livekit_enabled": true,
            "daemon_mode": true,
            "daemon_instance_id": "udoo",
            "allowed_agents": [{ "id": "codex-acp", "label": "Codex ACP" }],
            "debug": "tddy:term:*",
        })
    );
    server.stop().await;
}

#[tokio::test]
async fn tells_the_page_livekit_is_off_when_the_options_disable_it() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(RunServerOptions {
        livekit_enabled: false,
        ..options_serving(bundle.path(), port)
    })
    .await;

    // When
    let config: serde_json::Value = serde_json::from_str(&body_of(server.url("/api/config")).await)
        .expect("/api/config did not serve JSON");

    // Then
    assert_eq!(config.get("livekit_enabled"), Some(&json!(false)));
    server.stop().await;
}

#[tokio::test]
async fn answers_the_rpc_route_with_a_service_the_options_carry() {
    // Given
    let bundle = a_web_bundle();
    let port = a_free_tcp_port().await;
    let server = a_server_running_with(RunServerOptions {
        rpc_entries: vec![ServiceEntry {
            name: "tddy.EchoService",
            service: std::sync::Arc::new(EchoService),
        }],
        ..options_serving(bundle.path(), port)
    })
    .await;

    // When
    let answer = reqwest::Client::new()
        .post(server.url("/rpc/tddy.EchoService/Echo"))
        .header("content-type", "application/json")
        .body(r#"{"ping":"hello"}"#)
        .send()
        .await
        .expect("the RPC request failed")
        .text()
        .await
        .expect("the RPC response had no readable body");

    // Then
    assert_eq!(answer, r#"{"ping":"hello"}"#);
    server.stop().await;
}
