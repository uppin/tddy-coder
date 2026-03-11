//! RPC bridge for async service dispatch.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::envelope::decode_request;
use crate::proto::RpcRequest;
use crate::status::Status;

/// Result of an RPC call - either unary or server stream.
pub enum RpcResult {
    Unary(Result<Vec<u8>, Status>),
    ServerStream(Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>),
}

/// Trait for services that can handle RPC calls.
#[async_trait]
pub trait RpcService: Send + Sync + 'static {
    /// Handle an RPC call by service and method name.
    async fn handle_rpc(&self, service: &str, method: &str, request: &RpcRequest) -> RpcResult;
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
        let request = decode_request(payload).map_err(|e| Status::invalid_argument(e))?;
        self.handle_decoded_request(&request).await
    }

    /// Handle a decoded RpcRequest. Returns response bytes (one for unary, multiple for streaming).
    pub async fn handle_decoded_request(
        &self,
        request: &RpcRequest,
    ) -> Result<Vec<Vec<u8>>, Status> {
        let service = request
            .call_metadata
            .as_ref()
            .map(|m| m.service.as_str())
            .unwrap_or("");
        let method = request
            .call_metadata
            .as_ref()
            .map(|m| m.method.as_str())
            .unwrap_or("");

        match self.service.handle_rpc(service, method, request).await {
            RpcResult::Unary(Ok(response_bytes)) => Ok(vec![response_bytes]),
            RpcResult::Unary(Err(status)) => Err(status),
            RpcResult::ServerStream(Ok(mut rx)) => {
                let mut chunks = Vec::new();
                while let Some(item) = rx.recv().await {
                    chunks.push(item?);
                }
                Ok(chunks)
            }
            RpcResult::ServerStream(Err(status)) => Err(status),
        }
    }
}
