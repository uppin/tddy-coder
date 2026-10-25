//! Proof: an approval waiting on its conversation is not cut at the remote block cap.
//!
//! `MAX_REMOTE_BLOCK_MS` (20 s) clamps the block time of `Await` and `Shell` tool arguments, and
//! nothing in this crate or in the RPC transports below it gives a unary call a deadline. So a
//! `RequestApproval` that the daemon holds for its conversation's answer ends only with that answer,
//! or with the approval's own bound. The first test holds an answer past the cap and expects the
//! answer; the second expects an approval nobody answers to be denied at its bound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use prost::Message as _;
use tddy_rpc::{RpcClientTransport, Status};
use tddy_service::proto::exec_tools::ApprovalResponse;
use tddy_session_tool_client::{
    request_approval_via_rpc_transport, request_approval_within, SessionToolEnvelope,
    MAX_REMOTE_BLOCK_MS,
};

/// A daemon that holds every unary answer for `held`, then allows it. Stream calls are never made
/// by an approval, so they are refused.
struct HoldingDaemon {
    held: Duration,
}

#[async_trait]
impl RpcClientTransport for HoldingDaemon {
    async fn call_unary(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        tokio::time::sleep(self.held).await;
        Ok(ApprovalResponse {
            allow: true,
            message: String::new(),
        }
        .encode_to_vec())
    }

    async fn call_server_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }
}

/// A daemon that never answers.
struct SilentDaemon;

#[async_trait]
impl RpcClientTransport for SilentDaemon {
    async fn call_unary(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        std::future::pending().await
    }

    async fn call_server_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes: Vec<u8>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_client_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }

    async fn call_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        Err(Status::unimplemented("an approval is a unary call"))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_held_past_the_remote_block_cap_is_still_answered() {
    // Given: a daemon that holds its answer for longer than the cap before it allows the approval.
    let held = Duration::from_millis(MAX_REMOTE_BLOCK_MS + 5_000);
    let client: Arc<dyn RpcClientTransport> = Arc::new(HoldingDaemon { held });

    // When: the approval is asked over that daemon.
    let started = Instant::now();
    let answer = request_approval_via_rpc_transport(
        &client,
        &SessionToolEnvelope::default(),
        "conv-1",
        "Bash",
        &serde_json::json!({"command": "ls"}),
    )
    .await;

    // Then: the allow arrives, after the daemon has really held the call past the cap.
    assert_eq!(
        answer,
        Ok(true),
        "the approval held past the cap is answered"
    );
    assert!(
        started.elapsed() >= held,
        "the call was held for the daemon's full wait, not cut short"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_approval_nobody_answers_is_denied_at_its_bound() {
    // Given: a daemon that never answers, and an approval whose bound is 200 ms.
    let client: Arc<dyn RpcClientTransport> = Arc::new(SilentDaemon);

    // When: the approval is asked and waits out its bound.
    let answer = request_approval_within(
        &client,
        &SessionToolEnvelope::default(),
        "conv-1",
        "Bash",
        &serde_json::json!({"command": "ls"}),
        Duration::from_millis(200),
    )
    .await;

    // Then: the approval is refused, and the refusal says why.
    let reason = answer.expect_err("an approval nobody answers is refused");
    assert!(
        reason.contains("not answered"),
        "the refusal names the unanswered approval: {reason}"
    );
}
