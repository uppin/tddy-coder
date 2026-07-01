//! Envelope encode/decode helpers — thin re-exports of `tddy_rpc::envelope`, which owns the
//! actual `RpcRequest`/`RpcResponse` proto and encode/decode logic shared by every transport.

pub use tddy_rpc::envelope::{
    decode_request, decode_response, encode_request, encode_response, response_from_result,
};

#[cfg(test)]
mod tests {
    use crate::proto::CallMetadata;
    use tddy_rpc::envelope::{decode_request, encode_request, RpcRequest};

    #[test]
    fn envelope_encode_decode_roundtrip() {
        // Given an RpcRequest with all standard fields set
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

        // When encoding then decoding
        let encoded = encode_request(request).expect("encode");
        let decoded = decode_request(&encoded).expect("decode");

        // Then all fields survive the roundtrip
        assert_eq!(decoded.request_id, 42);
        assert_eq!(decoded.request_message, b"hello");
        assert_eq!(
            decoded.call_metadata.as_ref().unwrap().service,
            "test.EchoService"
        );
    }

    #[test]
    fn envelope_encode_decode_preserves_sender_identity() {
        // Given a request with sender_identity set
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

        // When encoding then decoding
        let encoded = encode_request(request).expect("encode");
        let decoded = decode_request(&encoded).expect("decode");

        // Then sender_identity is preserved
        assert_eq!(decoded.sender_identity.as_deref(), Some("client"));
    }
}
