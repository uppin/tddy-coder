//! Test fixture: a child process hosting `test.EchoService` over its own stdin/stdout, exercised
//! by `tests/rpc_over_stdio.rs`. Not test code itself — support for the acceptance tests, in the
//! spirit of the `serial-comm` `test/fixtures/rpc-server-test/user_test.cpp` reference pattern.
//!
//! On startup, before serving, it calls back into the parent's `parent.PingService/Ping` over
//! the same stdio pipe pair it will serve requests on — proving RPCs flow both ways over one
//! channel. The result is exposed via `test.EchoService/PingResult` once received.

use async_trait::async_trait;
use tddy_rpc::{BidiStreamOutput, RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tokio::sync::{mpsc, watch};

/// `EchoService`: unary `Echo`, server-streaming `EchoStream` (2-byte chunks), bidi `EchoBidi`
/// (echoes each incoming message immediately), and `PingResult` (reports the value received from
/// calling the parent's `PingService` on startup — empty until that call resolves).
struct EchoService {
    ping_result: watch::Receiver<Vec<u8>>,
}

#[async_trait]
impl RpcService for EchoService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "test.EchoService" && method == "EchoBidi"
    }

    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "test.EchoService");
        match method {
            "Echo" => RpcResult::Unary(Ok(message.payload.clone())),
            "PingResult" => {
                let mut rx = self.ping_result.clone();
                if rx.borrow().is_empty() {
                    // Event-based wait for the startup ping call to resolve — no polling.
                    let _ = rx.changed().await;
                }
                let value = rx.borrow().clone();
                RpcResult::Unary(Ok(value))
            }
            "EchoStream" => {
                let (tx, rx) = mpsc::channel(8);
                let payload = message.payload.clone();
                tokio::spawn(async move {
                    for chunk in payload.chunks(2) {
                        if tx.send(Ok(chunk.to_vec())).await.is_err() {
                            break;
                        }
                    }
                });
                RpcResult::ServerStream(Ok(rx))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "unknown method: {other}"
            )))),
        }
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, Status> {
        assert_eq!(service, "test.EchoService");
        assert_eq!(method, "EchoBidi");
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(message) = input_rx.recv().await {
                if tx.send(Ok(message.payload)).await.is_err() {
                    break;
                }
            }
        });
        Ok(BidiStreamOutput {
            output: tddy_rpc::ResponseBody::Streaming(rx),
        })
    }
}

// current_thread: this is a tiny, single-connection test fixture — the single-threaded runtime
// (feature "rt", already a main dependency) is sufficient, avoiding a "rt-multi-thread"
// dependency for a [[bin]] target that only ever runs under `cargo test`.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (ping_tx, ping_rx) = watch::channel(Vec::<u8>::new());
    let service = EchoService {
        ping_result: ping_rx,
    };

    let (client, endpoint) = tddy_stdio::StdioEndpoint::from_process_stdio(service);
    let run_handle = tokio::spawn(endpoint.run());

    let ping_response = client
        .call_unary("parent.PingService", "Ping", b"ping-from-child".to_vec())
        .await
        .expect("ping parent");
    ping_tx.send(ping_response).expect("store ping result");

    let _ = run_handle.await;
}
