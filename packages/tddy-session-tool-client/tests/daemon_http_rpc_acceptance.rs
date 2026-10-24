//! `DaemonHttpRpcClient` — a `tddy-rpc` client over the daemon's HTTP `/rpc` endpoint (Connect
//! protocol, protobuf payloads), which remote-codebase mode's `tddy-tools` reaches the relay with.
//!
//! The daemon is a raw TCP stub that records what it was asked and answers bytes the test chose.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md

use std::sync::{Arc, Mutex};

use tddy_rpc::{Code, RpcClientTransport};
use tddy_session_tool_client::daemon_http_rpc::DaemonHttpRpcClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One HTTP request the stub received: its request line, headers and body.
#[derive(Debug, Clone)]
struct Received {
    request_line: String,
    content_type: String,
    body: Vec<u8>,
}

/// An HTTP daemon that answers every request with `status`, `content_type` and `body`.
async fn a_daemon_answering(
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
) -> (String, Arc<Mutex<Vec<Received>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let received = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&received);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let request = read_request(&mut stream).await;
            log.lock().expect("log").push(request);
            let head = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        }
    });
    (format!("http://127.0.0.1:{port}/rpc"), received)
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Received {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await.expect("read");
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = find(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]).to_string();
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap_or(0))
                })
                .unwrap_or(0);
            while buf.len() < end + 4 + length {
                let n = stream.read(&mut chunk).await.expect("read body");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let content_type = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-type:")
                        .map(|v| v.trim().to_string())
                })
                .unwrap_or_default();
            return Received {
                request_line: head.lines().next().unwrap_or_default().to_string(),
                content_type,
                body: buf[end + 4..end + 4 + length].to_vec(),
            };
        }
    }
    panic!("the request ended before its headers");
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One Connect streaming envelope: a flags byte, a big-endian length, the payload.
fn envelope(flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![flags];
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[tokio::test]
async fn a_unary_call_posts_the_request_bytes_to_the_service_method_and_returns_the_answer() {
    // Given
    let (base_url, received) =
        a_daemon_answering("200 OK", "application/proto", vec![1, 2, 3]).await;
    let client = DaemonHttpRpcClient::new(base_url);

    // When
    let answer = client
        .call_unary(
            "session_agents.SessionAgentService",
            "OpenAgentConversation",
            vec![9, 8],
        )
        .await
        .expect("the call is answered");

    // Then
    assert_eq!(answer, vec![1, 2, 3]);
    let request = received.lock().expect("log")[0].clone();
    assert_eq!(
        request.request_line,
        "POST /rpc/session_agents.SessionAgentService/OpenAgentConversation HTTP/1.1"
    );
    assert_eq!(request.content_type, "application/proto");
    assert_eq!(request.body, vec![9, 8]);
}

#[tokio::test]
async fn a_server_stream_yields_each_enveloped_message_until_end_of_stream() {
    // Given
    let mut body = envelope(0, &[1]);
    body.extend(envelope(0, &[2, 2]));
    body.extend(envelope(2, b"{}"));
    let (base_url, received) =
        a_daemon_answering("200 OK", "application/connect+proto", body).await;
    let client = DaemonHttpRpcClient::new(base_url);

    // When
    let mut stream = client
        .call_server_stream(
            "session_agents.SessionAgentService",
            "PromptAgentConversation",
            vec![7],
        )
        .await
        .expect("the stream opens");
    let mut messages = Vec::new();
    while let Some(message) = stream.recv().await {
        messages.push(message.expect("a message, not an error"));
    }

    // Then
    assert_eq!(messages, vec![vec![1], vec![2, 2]]);
    let request = received.lock().expect("log")[0].clone();
    assert_eq!(request.content_type, "application/connect+proto");
    assert_eq!(
        request.body,
        envelope(0, &[7]),
        "the request is enveloped too"
    );
}

#[tokio::test]
async fn a_connect_error_answer_is_the_status_it_names() {
    // Given
    let (base_url, _) = a_daemon_answering(
        "404 Not Found",
        "application/json",
        br#"{"code":"not_found","message":"no conversation conv-9"}"#.to_vec(),
    )
    .await;
    let client = DaemonHttpRpcClient::new(base_url);

    // When
    let status = client
        .call_unary(
            "session_agents.SessionAgentService",
            "CancelAgentConversation",
            Vec::new(),
        )
        .await
        .expect_err("the daemon refused");

    // Then
    assert_eq!(status.code, Code::NotFound);
    assert!(status.message.contains("conv-9"), "{}", status.message);
}

#[tokio::test]
async fn an_error_at_the_end_of_a_stream_is_delivered_as_its_last_item() {
    // Given
    let mut body = envelope(0, &[1]);
    body.extend(envelope(
        2,
        br#"{"error":{"code":"failed_precondition","message":"clone is provisioning"}}"#,
    ));
    let (base_url, _) = a_daemon_answering("200 OK", "application/connect+proto", body).await;
    let client = DaemonHttpRpcClient::new(base_url);

    // When
    let mut stream = client
        .call_server_stream(
            "session_agents.SessionAgentService",
            "PromptAgentConversation",
            Vec::new(),
        )
        .await
        .expect("the stream opens");
    let first = stream.recv().await.expect("first").expect("a message");
    let last = stream.recv().await.expect("an error item");

    // Then
    assert_eq!(first, vec![1]);
    let status = last.expect_err("the stream ended with an error");
    assert_eq!(status.code, Code::FailedPrecondition);
}
