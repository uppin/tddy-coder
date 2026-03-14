//! RPC bridge for async service dispatch.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::message::RpcMessage;
use crate::status::Status;

/// Result of an RPC call - either unary or server stream.
pub enum RpcResult {
    Unary(Result<Vec<u8>, Status>),
    ServerStream(Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>),
}

/// Response body: either complete chunks (unary or finite stream) or a live stream.
pub enum ResponseBody {
    /// All chunks collected (unary or finite stream).
    Complete(Vec<Vec<u8>>),
    /// Live stream - transport must read and send incrementally.
    Streaming(mpsc::Receiver<Result<Vec<u8>, Status>>),
}

/// Trait for services that can handle RPC calls.
/// The transport layer passes already-extracted service/method names and RpcMessage slices.
#[async_trait]
pub trait RpcService: Send + Sync + 'static {
    /// Whether this method is a bidi stream (client and server both stream).
    /// When true, the transport processes each incoming message immediately instead of waiting for end_of_stream.
    fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool {
        false
    }

    /// Handle an RPC call by service and method name (single message, unary or server-stream start).
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult;

    /// Handle a stream of RPC messages (client streaming or bidi). Default treats single-message as unary.
    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        if messages.len() == 1 {
            self.handle_rpc(service, method, &messages[0]).await
        } else {
            RpcResult::Unary(Err(Status::unimplemented("streaming not supported")))
        }
    }
}

/// Entry for MultiRpcService: service name (e.g. "terminal.TerminalService") and the service.
pub struct ServiceEntry {
    pub name: &'static str,
    pub service: Arc<dyn RpcService>,
}

/// Multiplexer that dispatches RPC calls to multiple services by service name.
/// Used when a single participant serves multiple RPC services (e.g. Terminal + Token).
pub struct MultiRpcService {
    entries: Vec<ServiceEntry>,
}

impl MultiRpcService {
    pub fn new(entries: Vec<ServiceEntry>) -> Self {
        Self { entries }
    }

    fn find_service(&self, service: &str) -> Option<&Arc<dyn RpcService>> {
        self.entries
            .iter()
            .find(|e| e.name == service)
            .map(|e| &e.service)
    }
}

#[async_trait]
impl RpcService for MultiRpcService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        self.find_service(service)
            .map(|s| s.is_bidi_stream(service, method))
            .unwrap_or(false)
    }

    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        match self.find_service(service) {
            Some(s) => s.handle_rpc(service, method, message).await,
            None => RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown service: {}",
                service
            )))),
        }
    }

    async fn handle_rpc_stream(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> RpcResult {
        match self.find_service(service) {
            Some(s) => s.handle_rpc_stream(service, method, messages).await,
            None => RpcResult::Unary(Err(Status::not_found(format!(
                "Unknown service: {}",
                service
            )))),
        }
    }
}

/// Bridge that routes RPC messages to a service.
/// Receives already-extracted service/method and RpcMessage slices from the transport layer.
pub struct RpcBridge<S: RpcService> {
    service: Arc<S>,
}

impl<S: RpcService> RpcBridge<S> {
    pub fn new(service: S) -> Self {
        Self {
            service: Arc::new(service),
        }
    }

    /// Returns true if the given service/method is a bidi stream.
    pub fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        self.service.is_bidi_stream(service, method)
    }

    /// Handle a batch of RPC messages.
    /// The transport layer must have already extracted service and method from the envelope.
    pub async fn handle_messages(
        &self,
        service: &str,
        method: &str,
        messages: &[RpcMessage],
    ) -> Result<ResponseBody, Status> {
        let result = if messages.len() == 1 && !self.service.is_bidi_stream(service, method) {
            self.service.handle_rpc(service, method, &messages[0]).await
        } else {
            self.service
                .handle_rpc_stream(service, method, messages)
                .await
        };

        match result {
            RpcResult::Unary(Ok(response_bytes)) => {
                Ok(ResponseBody::Complete(vec![response_bytes]))
            }
            RpcResult::Unary(Err(status)) => Err(status),
            RpcResult::ServerStream(Ok(rx)) => Ok(ResponseBody::Streaming(rx)),
            RpcResult::ServerStream(Err(status)) => Err(status),
        }
    }
}
