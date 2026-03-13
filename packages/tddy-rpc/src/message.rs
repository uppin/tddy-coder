//! Protocol-agnostic RPC message types.

/// Metadata attached to an incoming RPC request.
#[derive(Debug, Clone, Default)]
pub struct RequestMetadata {
    /// Sender identity from the transport envelope (e.g. LiveKit participant identity).
    pub sender_identity: Option<String>,
}

/// Protocol-agnostic incoming RPC message.
/// The transport layer (e.g. LiveKit participant) decodes the envelope,
/// extracts service/method, and converts to this form before calling the bridge.
#[derive(Debug, Clone)]
pub struct RpcMessage {
    /// Raw encoded protobuf payload (the service-specific request message).
    pub payload: Vec<u8>,
    /// Request metadata (sender identity, etc.).
    pub metadata: RequestMetadata,
}

impl RpcMessage {
    pub fn new(payload: Vec<u8>, metadata: RequestMetadata) -> Self {
        Self { payload, metadata }
    }
}
