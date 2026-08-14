//! Acceptance tests: `tddy-tools` collecting a tool result over `StreamExecuteTool`.
//!
//! The unary `ExecuteTool` returns `result_json` as one string. Over LiveKit anything above
//! `MAX_CHUNK_FRAME_BYTES` (60 000) is chunk-framed by the transport, and that reassembly is
//! best-effort and index-keyed: a lost frame wedges the call permanently with **no error**. Reading
//! a large file or running a broad `Grep` in a split session crosses that on day one.
//!
//! `StreamExecuteTool` carries the same result in bounded frames instead, so truncation is
//! detectable rather than silent — but only if the client actually reads it that way. These tests
//! pin the client half: reassembly in order, and the truncation detection that is the whole point.
//!
//! The peer is an in-process duplex endpoint rather than a real LiveKit room: the dispatch takes an
//! `Arc<dyn RpcClientTransport>`, so the framing behaviour is transport-agnostic and a room would
//! only add a container to the test. The wire itself is covered by the daemon's cross-host suite.

use std::sync::Arc;

use async_trait::async_trait;
use prost::Message as _;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tddy_service::proto::connection::ExecuteToolChunk;
use tddy_tools::session_tool_client::{dispatch_via_streaming_rpc, SessionToolEnvelope};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

/// Bounded safety net around a channel-driven call; the peer is in-process.
const CALL_TIMEOUT: Duration = Duration::from_millis(900);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A peer that emits a fixed sequence of frames for `StreamExecuteTool`, so a test can state
/// exactly what arrives on the wire — including sequences a healthy daemon would never produce.
struct FramePlayingExecuteTool {
    frames: Vec<ExecuteToolChunk>,
}

#[async_trait]
impl RpcService for FramePlayingExecuteTool {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "connection.ConnectionService");
        assert_eq!(method, "StreamExecuteTool");
        let (tx, rx) = mpsc::channel(self.frames.len().max(1));
        for frame in &self.frames {
            tx.send(Ok(frame.encode_to_vec()))
                .await
                .expect("frame channel must accept every queued frame");
        }
        RpcResult::ServerStream(Ok(rx))
    }
}

struct NoCallbackService;

#[async_trait]
impl RpcService for NoCallbackService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

async fn a_transport_playing(frames: Vec<ExecuteToolChunk>) -> Arc<dyn RpcClientTransport> {
    let (client_side, server_side) = tokio::io::duplex(256 * 1024);
    let (server_read, server_write) = tokio::io::split(server_side);
    let (_unused, server_endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
        server_read,
        server_write,
        FramePlayingExecuteTool { frames },
    );
    tokio::spawn(server_endpoint.run());

    let (client_read, client_write) = tokio::io::split(client_side);
    let (client, client_endpoint) =
        tddy_stdio::StdioEndpoint::from_duplex(client_read, client_write, NoCallbackService);
    tokio::spawn(client_endpoint.run());
    client
}

/// A non-final frame carrying `body`.
fn a_frame(body: &str) -> ExecuteToolChunk {
    ExecuteToolChunk {
        result_chunk: body.as_bytes().to_vec(),
        last: false,
        ..Default::default()
    }
}

/// The terminal frame, carrying the outcome.
fn a_final_frame(body: &str) -> ExecuteToolChunk {
    ExecuteToolChunk {
        result_chunk: body.as_bytes().to_vec(),
        last: true,
        ..Default::default()
    }
}

async fn dispatch_against(frames: Vec<ExecuteToolChunk>) -> String {
    let client = a_transport_playing(frames).await;
    timeout(
        CALL_TIMEOUT,
        dispatch_via_streaming_rpc(
            &client,
            &SessionToolEnvelope::default(),
            "Read",
            &serde_json::json!({ "path": "src/main.rs" }),
        ),
    )
    .await
    .expect("the peer must answer within the call timeout")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_result_split_across_frames_reassembles_in_arrival_order() {
    // Given a result delivered in three frames
    let frames = vec![
        a_frame(r#"{"content":"first "#),
        a_frame("second "),
        a_final_frame(r#"third"}"#),
    ];

    // When
    let result = dispatch_against(frames).await;

    // Then — the concatenation is the result the unary call would have returned in one piece
    assert_eq!(result, r#"{"content":"first second third"}"#);
}

#[tokio::test]
async fn an_empty_result_arrives_as_a_single_final_frame() {
    // Given
    let frames = vec![a_final_frame("{}")];

    // When
    let result = dispatch_against(frames).await;

    // Then — a consumer never has to distinguish "empty result" from "nothing arrived"
    assert_eq!(result, "{}");
}

#[tokio::test]
async fn a_stream_that_ends_without_its_final_frame_is_reported_as_an_error() {
    // Given a stream that stops mid-result — a lost frame, or a peer that died partway
    let frames = vec![a_frame(r#"{"content":"half a"#), a_frame(" file")];

    // When
    let result = dispatch_against(frames).await;

    // Then — this is the entire reason the streaming RPC exists. Returning the partial text would
    // hand the agent a truncated file that reads as a complete one.
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(parsed["is_error"], serde_json::Value::Bool(true));
    assert!(
        parsed["error"]
            .as_str()
            .expect("an error result must carry an error string")
            .contains("truncated"),
        "the error must say the result was truncated rather than describe a transport fault; got {}",
        parsed["error"]
    );
}

#[tokio::test]
async fn a_tool_error_on_the_final_frame_surfaces_as_a_tool_error() {
    // Given a tool that failed, reported the way unary ExecuteTool reports it
    let frames = vec![ExecuteToolChunk {
        result_chunk: Vec::new(),
        is_error: true,
        error_message: "Write: missing 'contents' argument".to_string(),
        last: true,
        ..Default::default()
    }];

    // When
    let result = dispatch_against(frames).await;

    // Then — same shape the unary path produces, so an agent cannot tell the transports apart
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(parsed["is_error"], serde_json::Value::Bool(true));
    assert_eq!(parsed["error"], "Write: missing 'contents' argument");
}

#[tokio::test]
async fn a_background_job_handle_survives_the_final_frame() {
    // Given a Shell dispatched with block_until_ms:0, which returns a job handle rather than output
    let frames = vec![ExecuteToolChunk {
        result_chunk: br#"{"job_id":"job-7"}"#.to_vec(),
        job_id: "job-7".to_string(),
        job_running: true,
        last: true,
        ..Default::default()
    }];

    // When
    let result = dispatch_against(frames).await;

    // Then — long-running tools are driven through this handle rather than one long blocking call,
    // which is what keeps them inside the remote block budget on a transport that carries no
    // deadline of its own
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("result must be JSON");
    assert_eq!(parsed["job_id"], "job-7");
}
