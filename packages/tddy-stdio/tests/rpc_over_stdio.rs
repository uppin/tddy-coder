//! Acceptance tests for the stdio/IPC RPC transport (`tddy-stdio`). See
//! `docs/dev/1-WIP/rpc-multi-transport.md` for the design this validates, against the
//! `stdio-echo-fixture` companion binary (`tests/fixtures/echo_child.rs`).

use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService};
use tddy_stdio::spawn_child_endpoint;
use tokio::process::Command;
use tokio::time::timeout;

/// Bounded safety net around calls that are otherwise driven entirely by async channels/futures
/// (no polling) — see fluent-tests "Testing Async Code" guidelines. Generous to absorb child
/// process startup latency under CI load.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Path to the `stdio-echo-fixture` binary built alongside these tests.
fn echo_fixture_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_stdio-echo-fixture"))
}

/// Hosted by the *parent* (this test process) so the spawned child can call back into it
/// (principle: bidirectional calls over one stdio pipe). `Ping` deterministically upper-cases
/// the request payload so the reverse call is verifiable byte-perfect.
struct PingService;

#[async_trait]
impl RpcService for PingService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "parent.PingService");
        assert_eq!(method, "Ping");
        RpcResult::Unary(Ok(message.payload.to_ascii_uppercase()))
    }
}

#[tokio::test]
async fn calls_a_unary_echo_method_on_a_spawned_child_process() {
    // Given a child process hosting test.EchoService over its own stdin/stdout
    let endpoint = spawn_child_endpoint(echo_fixture_command(), PingService)
        .await
        .expect("spawn stdio-echo-fixture");

    // When calling its unary Echo method
    let response = timeout(
        CALL_TIMEOUT,
        endpoint
            .client
            .call_unary("test.EchoService", "Echo", b"hello-stdio".to_vec()),
    )
    .await
    .expect("Echo call timed out")
    .expect("Echo call failed");

    // Then the exact payload is echoed back
    assert_eq!(response, b"hello-stdio");
}

#[tokio::test]
async fn handles_five_concurrent_unary_calls_without_cross_talk() {
    // Given a running child endpoint
    let endpoint = spawn_child_endpoint(echo_fixture_command(), PingService)
        .await
        .expect("spawn stdio-echo-fixture");

    // When issuing 5 concurrent Echo calls with distinct deterministic payloads, multiplexed
    // over the same stdio pipe pair
    let calls = (0..5).map(|i| {
        let client = endpoint.client.clone();
        let payload = format!("echo-payload-{i}").into_bytes();
        async move {
            let response = timeout(
                CALL_TIMEOUT,
                client.call_unary("test.EchoService", "Echo", payload.clone()),
            )
            .await
            .expect("Echo call timed out")
            .expect("Echo call failed");
            (payload, response)
        }
    });
    let results = futures::future::join_all(calls).await;

    // Then every response matches its own request exactly — no cross-talk between concurrent
    // calls sharing one request-id-multiplexed channel
    for (payload, response) in results {
        assert_eq!(response, payload);
    }
}

#[tokio::test]
async fn receives_an_ordered_response_stream_for_one_server_streaming_call() {
    // Given a running child endpoint
    let endpoint = spawn_child_endpoint(echo_fixture_command(), PingService)
        .await
        .expect("spawn stdio-echo-fixture");

    // When calling a server-streaming method that splits the payload into 2-byte chunks
    let mut stream = timeout(
        CALL_TIMEOUT,
        endpoint
            .client
            .call_server_stream("test.EchoService", "EchoStream", b"ABCDEFGH".to_vec()),
    )
    .await
    .expect("EchoStream call timed out")
    .expect("EchoStream call failed");

    let mut chunks = Vec::new();
    while let Some(chunk) = timeout(CALL_TIMEOUT, stream.recv())
        .await
        .expect("stream item timed out")
    {
        chunks.push(chunk.expect("stream item error"));
    }

    // Then the chunks arrive in order, byte-perfect, and the stream closes cleanly
    assert_eq!(
        chunks,
        vec![
            b"AB".to_vec(),
            b"CD".to_vec(),
            b"EF".to_vec(),
            b"GH".to_vec(),
        ]
    );
}

#[tokio::test]
async fn exchanges_interleaved_messages_over_one_bidirectional_streaming_call() {
    // Given a running child endpoint
    let endpoint = spawn_child_endpoint(echo_fixture_command(), PingService)
        .await
        .expect("spawn stdio-echo-fixture");
    let (mut sender, mut responses) = endpoint
        .client
        .start_bidi_stream("test.EchoService", "EchoBidi")
        .expect("start bidi stream");

    // When sending messages one at a time, awaiting each echo before sending the next
    // (real-time streaming: the server processes each message as it arrives)
    let messages: [&[u8]; 3] = [b"first", b"second", b"third"];
    let mut echoed = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        let is_last = i == messages.len() - 1;
        sender
            .send(message.to_vec(), is_last)
            .await
            .expect("send bidi message");
        let response = timeout(CALL_TIMEOUT, responses.recv())
            .await
            .expect("bidi response timed out")
            .expect("bidi stream closed early")
            .expect("bidi response error");
        echoed.push(response);
    }

    // Then each message was echoed back in the order sent, before the next was sent
    assert_eq!(
        echoed,
        vec![b"first".to_vec(), b"second".to_vec(), b"third".to_vec()]
    );
}

#[tokio::test]
async fn lets_the_child_call_back_into_a_service_hosted_by_the_parent() {
    // Given a child that, on startup, calls back into the parent's PingService (over the same
    // stdio pipe pair the parent used to reach it) and stores the deterministic result
    let endpoint = spawn_child_endpoint(echo_fixture_command(), PingService)
        .await
        .expect("spawn stdio-echo-fixture");

    // When asking the child to report what it received from calling the parent
    let response = timeout(
        CALL_TIMEOUT,
        endpoint
            .client
            .call_unary("test.EchoService", "PingResult", Vec::new()),
    )
    .await
    .expect("PingResult call timed out")
    .expect("PingResult call failed");

    // Then it reports the exact value the parent's PingService produced for the child's ping —
    // proving the child successfully acted as an RPC client over the same channel it serves on
    assert_eq!(response, b"PING-FROM-CHILD".to_vec());
}
