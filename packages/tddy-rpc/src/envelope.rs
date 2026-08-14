//! Wire envelope (`RpcRequest`/`RpcResponse`) shared by every RPC transport — LiveKit data
//! channels, stdio pipes, and whatever comes next. Transports encode/decode this envelope and
//! feed the decoded messages into [`crate::client_engine::ClientEngine`] /
//! [`crate::server_engine::ServerEngine`]; the envelope itself carries no transport knowledge.

use prost::Message;

use crate::status::Status;

include!(concat!(env!("OUT_DIR"), "/rpc.rs"));

/// Decode an `RpcRequest` from bytes.
pub fn decode_request(bytes: &[u8]) -> Result<RpcRequest, String> {
    RpcRequest::decode(bytes).map_err(|e| e.to_string())
}

/// Decode an `RpcResponse` from bytes.
pub fn decode_response(bytes: &[u8]) -> Result<RpcResponse, String> {
    RpcResponse::decode(bytes).map_err(|e| e.to_string())
}

/// Encode an `RpcRequest` to bytes.
pub fn encode_request(request: RpcRequest) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    request.encode(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Encode an `RpcResponse` to bytes.
pub fn encode_response(response: RpcResponse) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    response.encode(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Identifies the call a response answers.
///
/// Exists so a response cannot be built without saying which call it belongs to. `request_id` alone
/// is not an identity: it restarts whenever a client rebuilds its id space (a browser page reload, a
/// process restart) while the peer still serves streams opened by the previous connection and
/// addressed to the same transport identity. Those frames would otherwise resolve whichever call now
/// holds the id, and their payload be decoded as that call's message type — silently, because the
/// engines hand callers raw bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct CallOrigin {
    pub request_id: i32,
    /// The originating client connection (see `RpcRequest.client_epoch`).
    pub client_epoch: u32,
    /// The service/method the caller invoked, echoed so the client can refuse a response that
    /// answers a different call than the one holding the id.
    pub call_metadata: Option<CallMetadata>,
}

impl CallOrigin {
    /// The origin of `request` — every response to it must carry this.
    pub fn of(request: &RpcRequest) -> Self {
        Self {
            request_id: request.request_id,
            client_epoch: request.client_epoch,
            call_metadata: request.call_metadata.clone(),
        }
    }
}

/// Build an `RpcResponse` from a result, attributed to the call that asked for it. Success:
/// response_message + end_of_stream. Error: `RpcError` with code and message from `Status`.
pub fn response_from_result(origin: &CallOrigin, result: Result<Vec<u8>, Status>) -> RpcResponse {
    match result {
        Ok(bytes) => RpcResponse {
            request_id: origin.request_id,
            response_message: bytes,
            metadata: None,
            end_of_stream: true,
            error: None,
            trailers: None,
            client_epoch: origin.client_epoch,
            call_metadata: origin.call_metadata.clone(),
        },
        Err(status) => RpcResponse {
            request_id: origin.request_id,
            response_message: vec![],
            metadata: None,
            end_of_stream: true,
            error: Some(RpcError {
                code: status.code.as_str().to_string(),
                message: status.message,
                details: std::collections::HashMap::new(),
            }),
            trailers: None,
            client_epoch: origin.client_epoch,
            call_metadata: origin.call_metadata.clone(),
        },
    }
}
