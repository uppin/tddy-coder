//! Acceptance: `tddy-tools` sends a managed coding agent's exec call for the conversation its launch
//! named, and for no conversation when the launch named none.
//!
//! The daemon names the conversation in `TDDY_CONVERSATION_ID` only for a clone-placed agent (see the
//! daemon's `clone_placement_exec_call_acceptance`). This test stands in for the daemon's exec-tool
//! endpoint with a local listener that records the body of each `ExecuteTool` it receives.

use std::sync::{Arc, Mutex};

use prost::Message;
use serde_json::json;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CONVERSATION_ENV: &str = "TDDY_CONVERSATION_ID";

/// The bytes of one HTTP request's body, read by its `Content-Length`.
async fn read_request_body(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.expect("read request");
        assert!(read > 0, "the request ended before its headers did");
        buf.extend_from_slice(&chunk[..read]);
        let Some(head_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .expect("a Content-Length header");
        let body_start = head_end + 4;
        while buf.len() < body_start + length {
            let read = stream.read(&mut chunk).await.expect("read request body");
            assert!(read > 0, "the request body ended early");
            buf.extend_from_slice(&chunk[..read]);
        }
        return buf[body_start..body_start + length].to_vec();
    }
}

/// A daemon endpoint that records the body of every exec call and answers each with success.
/// Returns its URL and what it recorded.
async fn a_daemon_recording_exec_calls() -> (String, Arc<Mutex<Vec<ExecuteToolRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let body = read_request_body(&mut stream).await;
            let call = ExecuteToolRequest::decode(body.as_slice()).expect("an exec call");
            sink.lock().expect("recorded calls").push(call);
            let answer = ExecuteToolResponse {
                result_json: "{\"ok\":true}".to_string(),
                ..Default::default()
            }
            .encode_to_vec();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/proto\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                answer.len()
            );
            stream
                .write_all(head.as_bytes())
                .await
                .expect("write response head");
            stream
                .write_all(&answer)
                .await
                .expect("write response body");
        }
    });
    (url, recorded)
}

#[tokio::test]
async fn an_exec_call_names_the_conversation_the_launch_named_and_none_otherwise() {
    // Given
    let (daemon_url, recorded) = a_daemon_recording_exec_calls().await;
    std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
    std::env::set_var("TDDY_REMOTE_DAEMON_URL", &daemon_url);
    std::env::set_var("TDDY_REMOTE_SESSION_ID", "1780828020298-dispatch");
    std::env::set_var(CONVERSATION_ENV, "conv-clone");
    let args = json!({ "path": "a.txt", "contents": "a\n" });

    // When
    let in_the_clone = tddy_tools::server::dispatch_dynamic_tool("Write", args.clone()).await;
    std::env::remove_var(CONVERSATION_ENV);
    let in_the_session = tddy_tools::server::dispatch_dynamic_tool("Write", args).await;

    // Then
    let calls = recorded.lock().expect("recorded calls").clone();
    let named = |call: &ExecuteToolRequest| json!(call.conversation_id);
    std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
    std::env::remove_var("TDDY_REMOTE_SESSION_ID");
    let answered = |answer: &str| serde_json::from_str::<serde_json::Value>(answer).expect("JSON");
    assert_eq!(
        (
            calls.len(),
            named(&calls[0]),
            named(&calls[1]),
            answered(&in_the_clone),
            answered(&in_the_session),
        ),
        (
            2,
            json!("conv-clone"),
            json!(""),
            json!({ "ok": true }),
            json!({ "ok": true }),
        )
    );
}
