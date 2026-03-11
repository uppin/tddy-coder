//! Echo service implementation for testing.

use async_trait::async_trait;
use prost::Message;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

use crate::bridge::{RpcBridge, RpcResult, RpcService};
use crate::proto::test::{EchoRequest, EchoResponse, EchoService};
use crate::status::Status;

/// Echo service implementation.
pub struct EchoServiceImpl;

#[async_trait]
impl EchoService for EchoServiceImpl {
    async fn echo(&self, request: EchoRequest) -> Result<EchoResponse, Status> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(EchoResponse {
            message: request.message,
            timestamp,
        })
    }

    async fn echo_server_stream(
        &self,
        request: EchoRequest,
    ) -> Result<mpsc::Receiver<Result<EchoResponse, Status>>, Status> {
        let (tx, rx) = mpsc::channel(16);
        let message = request.message;
        tokio::spawn(async move {
            for i in 0u32..3 {
                let _ = tx
                    .send(Ok(EchoResponse {
                        message: format!("{} #{}", message, i + 1),
                        timestamp: 0,
                    }))
                    .await;
            }
        });
        Ok(rx)
    }
}

#[async_trait]
impl RpcService for EchoServiceImpl {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        request: &crate::proto::RpcRequest,
    ) -> RpcResult {
        if service != "test.EchoService" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown service: {}",
                service
            ))));
        }

        match method {
            "Echo" => {
                let req = match EchoRequest::decode(&request.request_message[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(e.to_string())))
                    }
                };
                match self.echo(req).await {
                    Ok(resp) => RpcResult::Unary(Ok(resp.encode_to_vec())),
                    Err(e) => RpcResult::Unary(Err(e)),
                }
            }
            "EchoServerStream" => {
                let req = match EchoRequest::decode(&request.request_message[..]) {
                    Ok(r) => r,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(e.to_string())))
                    }
                };
                match self.echo_server_stream(req).await {
                    Ok(rx) => {
                        let (tx, new_rx) = mpsc::channel(16);
                        tokio::spawn(async move {
                            let mut rx = rx;
                            while let Some(item) = rx.recv().await {
                                let bytes = item.map(|r| r.encode_to_vec());
                                let _ = tx.send(bytes).await;
                            }
                        });
                        RpcResult::ServerStream(Ok(new_rx))
                    }
                    Err(e) => RpcResult::ServerStream(Err(e)),
                }
            }
            _ => RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown method: {}",
                method
            )))),
        }
    }
}

/// Create an RpcBridge with EchoServiceImpl.
pub fn create_echo_bridge() -> RpcBridge<EchoServiceImpl> {
    RpcBridge::new(EchoServiceImpl)
}
