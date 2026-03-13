//! RPC bridge for async service dispatch.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::envelope::decode_request;
use crate::proto::RpcRequest;
use crate::rpc_trace;
use crate::status::Status;

/// Result of an RPC call - either unary or server stream.
pub enum RpcResult {
    Unary(Result<Vec<u8>, Status>),
    ServerStream(Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>),
}

/// Trait for services that can handle RPC calls.
#[async_trait]
pub trait RpcService: Send + Sync + 'static {
    /// Handle an RPC call by service and method name (single message, unary or server-stream start).
    async fn handle_rpc(&self, service: &str, method: &str, request: &RpcRequest) -> RpcResult;

    /// Handle a stream of RPC messages (client streaming or bidi). Default treats single-message as unary.
    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcRequest],
    ) -> RpcResult {
        if messages.len() == 1 {
            self.handle_rpc(service, method, &messages[0]).await
        } else {
            RpcResult::Unary(Err(Status::unimplemented("streaming not supported")))
        }
    }
}

/// Bridge that routes RPC requests to a service.
pub struct RpcBridge<S: RpcService> {
    service: Arc<S>,
}

impl<S: RpcService> RpcBridge<S> {
    pub fn new(service: S) -> Self {
        Self {
            service: Arc::new(service),
        }
    }

    /// Handle a raw request payload. Returns Ok(bytes) on success, Err(Status) on RPC or decode error.
    /// Server streaming is not yet supported and returns Status::Unimplemented.
    pub async fn handle_request(&self, payload: &[u8]) -> Result<Vec<Vec<u8>>, Status> {
        let request = decode_request(payload).map_err(Status::invalid_argument)?;
        self.handle_decoded_request(&request).await
    }

    /// Handle a decoded RpcRequest. Returns response bytes (one for unary, multiple for streaming).
    #[allow(clippy::cloned_ref_to_slice_refs)]
    pub async fn handle_decoded_request(
        &self,
        request: &RpcRequest,
    ) -> Result<Vec<Vec<u8>>, Status> {
        self.handle_decoded_requests(&[request.clone()]).await
    }

    /// Handle a stream of decoded RpcRequests (client streaming or bidi).
    pub async fn handle_decoded_requests(
        &self,
        messages: &[RpcRequest],
    ) -> Result<Vec<Vec<u8>>, Status> {
        let service = messages
            .first()
            .and_then(|m| m.call_metadata.as_ref())
            .map(|m| m.service.as_str())
            .unwrap_or("");
        let method = messages
            .first()
            .and_then(|m| m.call_metadata.as_ref())
            .map(|m| m.method.as_str())
            .unwrap_or("");
        let request_id = messages.first().map(|m| m.request_id).unwrap_or(0);

        rpc_trace!(
            "RpcBridge::handle_decoded_requests request_id={} {}/{} ({} messages)",
            request_id,
            service,
            method,
            messages.len()
        );

        let result = if messages.len() == 1 {
            self.service.handle_rpc(service, method, &messages[0]).await
        } else {
            self.service
                .handle_rpc_stream(service, method, messages)
                .await
        };

        match result {
            RpcResult::Unary(Ok(response_bytes)) => {
                rpc_trace!(
                    "RpcBridge: request_id={} unary OK ({} bytes)",
                    request_id,
                    response_bytes.len()
                );
                Ok(vec![response_bytes])
            }
            RpcResult::Unary(Err(status)) => {
                rpc_trace!(
                    "RpcBridge: request_id={} unary error: {}",
                    request_id,
                    status.message
                );
                Err(status)
            }
            RpcResult::ServerStream(Ok(mut rx)) => {
                rpc_trace!(
                    "RpcBridge: request_id={} collecting server stream chunks",
                    request_id
                );
                let mut chunks = Vec::new();
                while let Some(item) = rx.recv().await {
                    chunks.push(item?);
                }
                rpc_trace!(
                    "RpcBridge: request_id={} stream finished with {} chunk(s)",
                    request_id,
                    chunks.len()
                );
                Ok(chunks)
            }
            RpcResult::ServerStream(Err(status)) => {
                rpc_trace!(
                    "RpcBridge: request_id={} stream error: {}",
                    request_id,
                    status.message
                );
                Err(status)
            }
        }
    }
}
