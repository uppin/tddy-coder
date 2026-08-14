//! Regression tests: every response must carry the identity of the call it answers.
//!
//! A client correlates responses by `request_id`, but request ids restart whenever a connection is
//! rebuilt (a browser reload, a process restart) while the peer keeps serving streams opened by the
//! previous one. Without an echo of the caller's `client_epoch`, a client cannot tell a frame of its
//! own call from a frame of a dead connection's call that happens to share the id — and delivers
//! the wrong bytes to be decoded as the wrong message type.
//!
//! The server is the only party that can supply the echo, so it must copy both `client_epoch` and
//! `call_metadata` from the request onto every response it emits for that request.

use async_trait::async_trait;
use std::time::Duration;
use tddy_rpc::envelope::{CallMetadata, RpcRequest};
use tddy_rpc::server_engine::ServerEngine;
use tddy_rpc::{BidiStreamOutput, RpcMessage, RpcResult, RpcService, Status};
use tokio::sync::mpsc;
use tokio::time::timeout;

const A_CLIENT_EPOCH: u32 = 0x5f3a_91c2;
const A_PEER: &str = "web-alice";

/// Echoes the request payload back unchanged, and streams three items for the streaming method.
/// A fake, not a mock — real behavior, per the fluent-tests guidelines.
struct EchoStub;

#[async_trait]
impl RpcService for EchoStub {
    fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool {
        false
    }

    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        if method == "StreamTerminalOutput" {
            let (tx, rx) = mpsc::channel(8);
            let payload = message.payload.clone();
            tokio::spawn(async move {
                for _ in 0..2 {
                    if tx.send(Ok(payload.clone())).await.is_err() {
                        break;
                    }
                }
            });
            return RpcResult::ServerStream(Ok(rx));
        }
        RpcResult::Unary(Ok(message.payload.clone()))
    }

    async fn start_bidi_stream(
        &self,
        _service: &str,
        _method: &str,
        _input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        Err(Status::internal("not used by these tests"))
    }
}

fn a_request(request_id: i32, method: &str, client_epoch: u32) -> RpcRequest {
    RpcRequest {
        request_id,
        request_message: b"hello".to_vec(),
        call_metadata: Some(CallMetadata {
            service: "connection.ConnectionService".to_string(),
            method: method.to_string(),
        }),
        metadata: None,
        end_of_stream: true,
        abort: false,
        sender_identity: Some(A_PEER.to_string()),
        client_epoch,
    }
}

async fn next_response(
    rx: &mut mpsc::Receiver<(String, tddy_rpc::envelope::RpcResponse)>,
) -> tddy_rpc::envelope::RpcResponse {
    let (_peer, response) = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("a response must be published within 1s")
        .expect("the outgoing channel must not close");
    response
}

#[tokio::test]
async fn echoes_the_callers_client_epoch_on_a_unary_response() {
    // Given
    let engine = ServerEngine::new(EchoStub);
    let (tx, mut rx) = mpsc::channel(8);

    // When
    engine
        .on_request(
            A_PEER,
            a_request(7, "ClaimTerminalControl", A_CLIENT_EPOCH),
            tx,
        )
        .await;

    // Then — the caller can tell this answers *its* call, not a dead connection's id 7
    assert_eq!(next_response(&mut rx).await.client_epoch, A_CLIENT_EPOCH);
}

#[tokio::test]
async fn echoes_the_callers_call_metadata_on_a_unary_response() {
    // Given
    let engine = ServerEngine::new(EchoStub);
    let (tx, mut rx) = mpsc::channel(8);

    // When
    engine
        .on_request(
            A_PEER,
            a_request(7, "ClaimTerminalControl", A_CLIENT_EPOCH),
            tx,
        )
        .await;

    // Then
    let metadata = next_response(&mut rx)
        .await
        .call_metadata
        .expect("a response must name the call it answers");
    assert_eq!(metadata.method, "ClaimTerminalControl");
}

#[tokio::test]
async fn echoes_the_callers_client_epoch_on_every_streamed_frame() {
    // Given — a streaming call, whose frames are the ones that leak across connections
    let engine = ServerEngine::new(EchoStub);
    let (tx, mut rx) = mpsc::channel(8);

    // When
    engine
        .on_request(
            A_PEER,
            a_request(3, "StreamTerminalOutput", A_CLIENT_EPOCH),
            tx,
        )
        .await;

    // Then — the second frame carries it too, not just the first
    assert_eq!(next_response(&mut rx).await.client_epoch, A_CLIENT_EPOCH);
    assert_eq!(next_response(&mut rx).await.client_epoch, A_CLIENT_EPOCH);
}

#[tokio::test]
async fn echoes_the_callers_client_epoch_on_an_error_response() {
    // Given — a service that fails every call, so the error path is exercised
    struct FailingStub;

    #[async_trait]
    impl RpcService for FailingStub {
        fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool {
            false
        }
        async fn handle_rpc(
            &self,
            _service: &str,
            _method: &str,
            _message: &RpcMessage,
        ) -> RpcResult {
            RpcResult::Unary(Err(Status::internal("boom")))
        }
        async fn start_bidi_stream(
            &self,
            _service: &str,
            _method: &str,
            _input_rx: mpsc::Receiver<RpcMessage>,
        ) -> Result<BidiStreamOutput, Status> {
            Err(Status::internal("not used by this test"))
        }
    }

    let engine = ServerEngine::new(FailingStub);
    let (tx, mut rx) = mpsc::channel(8);

    // When
    engine
        .on_request(
            A_PEER,
            a_request(7, "ClaimTerminalControl", A_CLIENT_EPOCH),
            tx,
        )
        .await;

    // Then — an error must be attributable to its caller too, or it resolves a stranger's call
    assert_eq!(next_response(&mut rx).await.client_epoch, A_CLIENT_EPOCH);
}
