//! Submit is acknowledged on the wire immediately after storing results; the presenter only
//! receives an activity-log notification. If the presenter never polls, `tddy-tools` still gets
//! `{"status":"ok",...}` without waiting on the UI loop.
//!
//! End-to-end CLI coverage: `packages/tddy-tools/tests/submit_relay_no_poll.rs` (same invariant).
//!
//! The listener started by `start_toolcall_listener` serves connections over
//! `tddy-rpc`/`tddy-stdio`'s length-prefixed frame protocol (see `tddy_core::toolcall::listener`
//! and `tddy_tools::toolcall_client`) rather than raw newline-delimited JSON, so the client here
//! must speak the same framing — a bare `UnixStream` write/read-line pair (the old bespoke
//! protocol's shape) is never decoded by the server's `FrameDecoder` and the read hangs forever.

use serde_json::json;
use serde_json::Value;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;
use tddy_rpc::RpcClientTransport;
use tokio::net::UnixStream;
use tokio::time::timeout;

/// The relay never initiates calls into this test's client — any inbound request here would be
/// a bug, so it fails loudly rather than silently no-op'ing (mirrors
/// `tddy_tools::toolcall_client::NoCallbackToolcallService`).
struct NoCallbackService;

#[async_trait::async_trait]
impl tddy_rpc::RpcService for NoCallbackService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unimplemented(format!(
            "test process hosts no callback service, got {service}/{method}"
        ))))
    }
}

#[tokio::test]
#[cfg(unix)]
async fn relay_accepts_submit_when_presenter_never_polls() {
    // Given
    let tddy_data_dir =
        std::env::temp_dir().join(format!("tddy-toolcall-relay-{}", std::process::id()));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let (socket_path, _hold_tool_rx) =
        start_toolcall_listener(None, None, tddy_data_dir).expect("start listener");

    let path = socket_path.clone();
    let client = tokio::spawn(async move {
        let stream = UnixStream::connect(path).await.expect("connect");
        let (read_half, write_half) = tokio::io::split(stream);
        let (rpc_client, endpoint) =
            tddy_stdio::StdioEndpoint::from_duplex(read_half, write_half, NoCallbackService);
        tokio::spawn(endpoint.run());

        let request = json!({
            "type": "submit",
            "goal": "plan",
            "data": {"goal": "plan", "prd": "# x"}
        });
        let payload = serde_json::to_vec(&request).expect("encode request");
        rpc_client
            .call_unary("tddy.toolcall.ToolcallService", "Submit", payload)
            .await
            .expect("Submit call")
    });

    let deadline = Duration::from_secs(2);
    let wrapped = timeout(deadline, client).await;

    // Then
    assert!(
        wrapped.is_ok(),
        "relay must return a response within {:?} when presenter never polls (stuck case); client hung waiting for response",
        deadline
    );
    let response_bytes = wrapped.unwrap().expect("join client task");

    // When
    let v: Value = serde_json::from_slice(&response_bytes).expect("response is JSON");
    assert_eq!(v["status"], "ok", "expected ok status, got: {v}");
    assert_eq!(v["goal"], "plan");
}
