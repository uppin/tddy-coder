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

    async fn echo_client_stream(&self, requests: Vec<EchoRequest>) -> Result<EchoResponse, Status> {
        let joined = requests
            .iter()
            .map(|r| r.message.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(EchoResponse {
            message: joined,
            timestamp,
        })
    }

    async fn echo_bidi_stream(
        &self,
        requests: Vec<EchoRequest>,
    ) -> Result<mpsc::Receiver<Result<EchoResponse, Status>>, Status> {
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            for req in requests {
                let _ = tx
                    .send(Ok(EchoResponse {
                        message: req.message,
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
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "test.EchoService" && method == "EchoBidiStream"
    }

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

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[crate::proto::RpcRequest],
    ) -> crate::bridge::RpcResult {
        if service != "test.EchoService" {
            return RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown service: {}",
                service
            ))));
        }

        match method {
            "EchoClientStream" => {
                let mut requests = Vec::with_capacity(messages.len());
                for msg in messages {
                    if msg.request_message.is_empty() {
                        continue;
                    }
                    match EchoRequest::decode(&msg.request_message[..]) {
                        Ok(r) => requests.push(r),
                        Err(e) => {
                            return RpcResult::Unary(Err(Status::invalid_argument(e.to_string())))
                        }
                    }
                }
                match self.echo_client_stream(requests).await {
                    Ok(resp) => RpcResult::Unary(Ok(resp.encode_to_vec())),
                    Err(e) => RpcResult::Unary(Err(e)),
                }
            }
            "EchoBidiStream" => {
                let mut requests = Vec::with_capacity(messages.len());
                for msg in messages {
                    if msg.request_message.is_empty() {
                        continue;
                    }
                    match EchoRequest::decode(&msg.request_message[..]) {
                        Ok(r) => requests.push(r),
                        Err(e) => {
                            return RpcResult::ServerStream(Err(Status::invalid_argument(
                                e.to_string(),
                            )))
                        }
                    }
                }
                match self.echo_bidi_stream(requests).await {
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
