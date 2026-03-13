//! Envelope encode/decode helpers.

use tddy_rpc::Status;

use crate::proto::{RpcError, RpcRequest, RpcResponse};
use prost::Message;

/// Decode RpcRequest from bytes.
pub fn decode_request(bytes: &[u8]) -> Result<RpcRequest, String> {
    RpcRequest::decode(bytes).map_err(|e| e.to_string())
}

/// Encode RpcResponse to bytes.
pub fn encode_response(response: RpcResponse) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    response.encode(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Build RpcResponse from a result. Success: response_message + end_of_stream.
/// Error: RpcError with code and message from Status.
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

/// Encode RpcRequest to bytes.
pub fn encode_request(request: RpcRequest) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    request.encode(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::CallMetadata;

    #[test]
    fn envelope_encode_decode_roundtrip() {
        let request = RpcRequest {
            request_id: 42,
            request_message: b"hello".to_vec(),
            call_metadata: Some(CallMetadata {
                service: "test.EchoService".to_string(),
                method: "Echo".to_string(),
            }),
            metadata: None,
            end_of_stream: false,
            abort: false,
            sender_identity: None,
        };
        let encoded = encode_request(request).expect("encode");
        let decoded = decode_request(&encoded).expect("decode");
        assert_eq!(decoded.request_id, 42);
        assert_eq!(decoded.request_message, b"hello");
        assert_eq!(
            decoded.call_metadata.as_ref().unwrap().service,
            "test.EchoService"
        );
    }

    #[test]
    fn envelope_encode_decode_preserves_sender_identity() {
        let request = RpcRequest {
            request_id: 1,
            request_message: b"test".to_vec(),
            call_metadata: Some(CallMetadata {
                service: "test.EchoService".to_string(),
                method: "Echo".to_string(),
            }),
            metadata: None,
            end_of_stream: true,
            abort: false,
            sender_identity: Some("client".to_string()),
        };
        let encoded = encode_request(request).expect("encode");
        let decoded = decode_request(&encoded).expect("decode");
        assert_eq!(decoded.sender_identity.as_deref(), Some("client"));
    }
}
