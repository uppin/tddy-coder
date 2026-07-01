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

/// Build an `RpcResponse` from a result. Success: response_message + end_of_stream.
/// Error: `RpcError` with code and message from `Status`.
pub fn response_from_result(request_id: i32, result: Result<Vec<u8>, Status>) -> RpcResponse {
    match result {
        Ok(bytes) => RpcResponse {
            request_id,
            response_message: bytes,
            metadata: None,
            end_of_stream: true,
            error: None,
            trailers: None,
        },
        Err(status) => RpcResponse {
            request_id,
            response_message: vec![],
            metadata: None,
            end_of_stream: true,
            error: Some(RpcError {
                code: status.code.as_str().to_string(),
                message: status.message,
                details: std::collections::HashMap::new(),
            }),
            trailers: None,
        },
    }
}
