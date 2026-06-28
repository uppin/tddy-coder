//! Red: the shared host-side relay dispatches tool requests to the injected handler and fulfills
//! CONNECT tunnels by dialing the real upstream and acking the result.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Captured, Mode};
use tddy_sandbox_runner::{run_host_relay, ExecuteToolResponse, HostRelayConfig, HostToolHandler};
use tddy_service::tonic_sandbox::sandbox_service_client::SandboxServiceClient;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const SESSION_ID: &str = "host-relay-dispatch-session";

/// Records every tool the relay asked it to run and echoes the tool name back.
#[derive(Clone, Default)]
struct RecordingToolHandler {
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl HostToolHandler for RecordingToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> ExecuteToolResponse {
        self.calls.lock().unwrap().push(tool_name.to_string());
        ExecuteToolResponse {
            result_json: format!(r#"{{"tool":"{tool_name}"}}"#),
            is_error: false,
            ..Default::default()
        }
    }
}

async fn connect(endpoint: String) -> SandboxServiceClient<tonic::transport::Channel> {
    SandboxServiceClient::connect(endpoint)
        .await
        .expect("connect fake sandbox grpc")
}

fn relay_config() -> (HostRelayConfig, mpsc::UnboundedReceiver<Bytes>) {
    let (terminal_tx, terminal_rx) = mpsc::unbounded_channel::<Bytes>();
    (HostRelayConfig::new(SESSION_ID, terminal_tx), terminal_rx)
}

/// Poll `captured` until `done` is satisfied or the bounded deadline elapses.
async fn await_captured(captured: &Arc<Mutex<Captured>>, done: impl Fn(&Captured) -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if done(&captured.lock().unwrap()) || tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Bind then drop a listener to obtain a port nothing is listening on.
async fn unused_loopback_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// **dispatches_a_tool_request_to_the_injected_handler**: a `ToolRequest` from the jail is routed
/// to the `HostToolHandler` and its response is sent back over the channel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatches_a_tool_request_to_the_injected_handler() {
    // Given
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushToolRequest {
        tool_name: "Read".to_string(),
    })
    .await;
    let handler = RecordingToolHandler::default();
    let calls = Arc::clone(&handler.calls);
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay(connect(endpoint).await, handler, config, stdin_rx)
        .await
        .expect("start host relay");
    await_captured(&captured, |c| !c.tool_responses.is_empty()).await;

    // Then
    assert_eq!(calls.lock().unwrap().as_slice(), ["Read"]);
    let responses = &captured.lock().unwrap().tool_responses;
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].result_json, r#"{"tool":"Read"}"#);
}

/// **dials_the_upstream_and_acks_a_connect_tunnel**: a `TunnelOpen` to a reachable host makes the
/// relay open the real socket and reply `TunnelOpenAck{ok=true}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dials_the_upstream_and_acks_a_connect_tunnel() {
    // Given — a live upstream the relay can dial.
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_port = upstream.local_addr().unwrap().port();
    tokio::spawn(async move { while upstream.accept().await.is_ok() {} });
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushTunnelOpen {
        host: "127.0.0.1".to_string(),
        port: upstream_port,
    })
    .await;
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay(
        connect(endpoint).await,
        RecordingToolHandler::default(),
        config,
        stdin_rx,
    )
    .await
    .expect("start host relay");
    await_captured(&captured, |c| !c.tunnel_acks.is_empty()).await;

    // Then
    let acks = &captured.lock().unwrap().tunnel_acks;
    assert_eq!(acks.len(), 1);
    assert!(
        acks[0].ok,
        "expected tunnel ack ok, error: {}",
        acks[0].error
    );
}

/// **acks_a_connect_tunnel_failure_when_the_upstream_is_unreachable**: a `TunnelOpen` to a dead
/// port makes the relay reply `TunnelOpenAck{ok=false}` with a non-empty error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acks_a_connect_tunnel_failure_when_the_upstream_is_unreachable() {
    // Given — a port with nothing listening.
    let dead_port = unused_loopback_port().await;
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushTunnelOpen {
        host: "127.0.0.1".to_string(),
        port: dead_port,
    })
    .await;
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay(
        connect(endpoint).await,
        RecordingToolHandler::default(),
        config,
        stdin_rx,
    )
    .await
    .expect("start host relay");
    await_captured(&captured, |c| !c.tunnel_acks.is_empty()).await;

    // Then
    let acks = &captured.lock().unwrap().tunnel_acks;
    assert_eq!(acks.len(), 1);
    assert!(!acks[0].ok, "expected tunnel ack failure for a dead port");
    assert!(!acks[0].error.is_empty(), "failure ack must carry an error");
}
