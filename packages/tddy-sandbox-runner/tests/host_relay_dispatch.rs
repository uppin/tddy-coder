//! Red: the shared host-side relay dispatches tool requests to the injected handler and fulfills
//! CONNECT tunnels by dialing the real upstream and acking the result.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use common::{serve_fake_over_tcp, Captured, Mode};
use tddy_sandbox_runner::{
    run_host_relay, run_host_relay_with_rpc, ExecuteToolResponse, HostRelayConfig, HostRpcHandler,
    HostToolHandler,
};
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

/// A `HostRpcHandler` that returns a fixed unary body for one method and a fixed server stream
/// for another, so the bridge's two response shapes both get exercised.
struct StubRpcHandler {
    unary_body: Vec<u8>,
    stream_frames: Vec<Vec<u8>>,
}

#[async_trait::async_trait]
impl HostRpcHandler for StubRpcHandler {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _payload: &[u8],
    ) -> tddy_rpc::RpcResult {
        match (service, method) {
            ("test.Service", "Unary") => {
                tddy_rpc::RpcResult::Unary(Ok(self.unary_body.clone()))
            }
            ("test.Service", "Stream") => {
                let (tx, rx) = tokio::sync::mpsc::channel(8);
                let frames = self.stream_frames.clone();
                tokio::spawn(async move {
                    for frame in frames {
                        if tx.send(Ok(frame)).await.is_err() {
                            return;
                        }
                    }
                });
                tddy_rpc::RpcResult::ServerStream(Ok(rx))
            }
            _ => tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "no stub for {service}/{method}"
            )))),
        }
    }
}

/// **dispatches_a_unary_rpc_to_the_injected_handler**: an `RpcRequest` for a unary method is routed
/// to the `HostRpcHandler` and its response is sent back as one terminal `RpcStreamFrame`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatches_a_unary_rpc_to_the_injected_handler() {
    // Given
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushRpcRequest {
        request_id: "rpc-unary-1".to_string(),
        service: "test.Service".to_string(),
        method: "Unary".to_string(),
        payload: Vec::new(),
    })
    .await;
    let handler = Arc::new(StubRpcHandler {
        unary_body: b"unary-ok".to_vec(),
        stream_frames: Vec::new(),
    });
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay_with_rpc(
        connect(endpoint).await,
        RecordingToolHandler::default(),
        handler,
        config,
        stdin_rx,
    )
    .await
    .expect("start host relay");
    await_captured(&captured, |c| !c.rpc_stream_frames.is_empty()).await;

    // Then — exactly one terminal frame carrying the unary body.
    let frames = std::mem::take(&mut captured.lock().unwrap().rpc_stream_frames);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].request_id, "rpc-unary-1");
    assert_eq!(frames[0].payload, b"unary-ok");
    assert!(frames[0].end_of_stream);
    assert!(frames[0].error.is_empty());
}

/// **dispatches_a_server_stream_rpc_to_the_injected_handler**: an `RpcRequest` for a streaming
/// method is routed to the `HostRpcHandler` and each frame is sent back as a `RpcStreamFrame`,
/// followed by one terminal frame with `end_of_stream`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatches_a_server_stream_rpc_to_the_injected_handler() {
    // Given
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushRpcRequest {
        request_id: "rpc-stream-1".to_string(),
        service: "test.Service".to_string(),
        method: "Stream".to_string(),
        payload: Vec::new(),
    })
    .await;
    let handler = Arc::new(StubRpcHandler {
        unary_body: Vec::new(),
        stream_frames: vec![b"chunk-a".to_vec(), b"chunk-b".to_vec()],
    });
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay_with_rpc(
        connect(endpoint).await,
        RecordingToolHandler::default(),
        handler,
        config,
        stdin_rx,
    )
    .await
    .expect("start host relay");
    await_captured(&captured, |c| {
        c.rpc_stream_frames.iter().any(|f| f.end_of_stream)
    })
    .await;

    // Then — two payload frames then one terminal marker, in order, all addressed to the request.
    let frames = std::mem::take(&mut captured.lock().unwrap().rpc_stream_frames);
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0].request_id, "rpc-stream-1");
    assert_eq!(frames[0].payload, b"chunk-a");
    assert!(!frames[0].end_of_stream);
    assert_eq!(frames[1].payload, b"chunk-b");
    assert!(!frames[1].end_of_stream);
    assert!(frames[2].end_of_stream);
    assert!(frames[2].error.is_empty());
    assert!(frames[2].payload.is_empty());
}

/// **refuses_a_rpc_the_handler_does_not_serve**: an `RpcRequest` for an unknown method is sent back
/// as one terminal `RpcStreamFrame` carrying the handler's error message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refuses_a_rpc_the_handler_does_not_serve() {
    // Given
    let (endpoint, captured) = serve_fake_over_tcp(Mode::PushRpcRequest {
        request_id: "rpc-unknown-1".to_string(),
        service: "test.Service".to_string(),
        method: "Nope".to_string(),
        payload: Vec::new(),
    })
    .await;
    let handler = Arc::new(StubRpcHandler {
        unary_body: Vec::new(),
        stream_frames: Vec::new(),
    });
    let (config, _terminal_rx) = relay_config();
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();

    // When
    let _relay = run_host_relay_with_rpc(
        connect(endpoint).await,
        RecordingToolHandler::default(),
        handler,
        config,
        stdin_rx,
    )
    .await
    .expect("start host relay");
    await_captured(&captured, |c| !c.rpc_stream_frames.is_empty()).await;

    // Then — one terminal frame carrying the not-found error message.
    let frames = std::mem::take(&mut captured.lock().unwrap().rpc_stream_frames);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].request_id, "rpc-unknown-1");
    assert!(frames[0].end_of_stream);
    assert!(
        frames[0].error.contains("no stub for test.Service/Nope"),
        "expected the handler's error message, got: {}",
        frames[0].error
    );
}
