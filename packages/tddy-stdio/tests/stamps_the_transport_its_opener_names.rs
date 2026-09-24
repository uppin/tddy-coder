//! A framed endpoint stamps every request it hosts with the transport whoever opened its channel
//! named, and a frame written onto that channel cannot change the stamp.
//!
//! The endpoint cannot tell a Unix socket from a pipe — it only sees bytes — so the transport is
//! named where the channel is opened (`StdioEndpoint::from_duplex`). The embedded daemon's agent
//! tool socket names `UnixSocket`, which is how a co-located process's sign-in is told apart from
//! the desktop window's.

use std::collections::HashMap;

use async_trait::async_trait;
use tddy_rpc::envelope::{self, CallMetadata, Metadata, RpcRequest, RpcResponse};
use tddy_rpc::transport::{encode_frame, FrameDecoder, FrameKind};
use tddy_rpc::{RequestTransport, RpcMessage, RpcResult, RpcService};
use tddy_stdio::StdioEndpoint;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

#[tokio::test]
async fn a_request_is_stamped_with_the_transport_its_channel_was_opened_as() {
    // Given an endpoint over a channel its opener named a Unix socket
    let mut peer = a_peer_of_an_endpoint_over(RequestTransport::UnixSocket);

    // When the peer asks which transport its call arrived over
    let answer = peer.call(a_which_transport_request(None)).await;

    // Then the service was told the socket
    assert_eq!(answer.response_message, b"UnixSocket");
}

#[tokio::test]
async fn a_frame_claiming_the_in_process_bridge_is_stamped_with_the_channels_transport() {
    // Given an endpoint over a channel its opener named a Unix socket
    let mut peer = a_peer_of_an_endpoint_over(RequestTransport::UnixSocket);

    // When the peer's frame claims, in every field it writes, to be the in-process bridge
    let answer = peer
        .call(a_which_transport_request(Some("InProcess")))
        .await;

    // Then the claim is ignored
    assert_eq!(answer.response_message, b"UnixSocket");
}

/// Answers every call with the name of the transport it was stamped with.
struct TransportNamer;

#[async_trait]
impl RpcService for TransportNamer {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(
            format!("{:?}", message.metadata.transport()).into_bytes()
        ))
    }
}

/// The far end of a channel an endpoint hosting [`TransportNamer`] was opened over, as
/// `transport`. Speaks raw frames, so a test can write fields no well-behaved client would.
struct Peer {
    channel: DuplexStream,
    decoder: FrameDecoder,
}

fn a_peer_of_an_endpoint_over(transport: RequestTransport) -> Peer {
    let (endpoint_side, peer_side) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(endpoint_side);
    let (_client, endpoint) = StdioEndpoint::from_duplex(reader, writer, TransportNamer, transport);
    tokio::spawn(endpoint.run());
    Peer {
        channel: peer_side,
        decoder: FrameDecoder::new(),
    }
}

impl Peer {
    /// Write `request` as one frame and read back the response frame that answers it.
    async fn call(&mut self, request: RpcRequest) -> RpcResponse {
        let bytes = envelope::encode_request(request).expect("the request encodes");
        self.channel
            .write_all(&encode_frame(FrameKind::Request, &bytes))
            .await
            .expect("the frame is written");
        let mut buffer = [0_u8; 4096];
        loop {
            if let Some((FrameKind::Response, payload)) = self.decoder.next_frame() {
                return envelope::decode_response(&payload).expect("the response decodes");
            }
            let read = self
                .channel
                .read(&mut buffer)
                .await
                .expect("the channel is readable");
            assert!(
                read > 0,
                "the endpoint closed the channel without answering"
            );
            self.decoder.feed(&buffer[..read]);
        }
    }
}

/// A unary call asking which transport it arrived over, its sender-written fields claiming
/// `claimed` when given.
fn a_which_transport_request(claimed: Option<&str>) -> RpcRequest {
    RpcRequest {
        request_id: 1,
        request_message: Vec::new(),
        call_metadata: Some(CallMetadata {
            service: "test.TransportService".to_string(),
            method: "WhichTransport".to_string(),
        }),
        metadata: claimed.map(|claimed| Metadata {
            values: HashMap::from([("transport".to_string(), claimed.to_string())]),
        }),
        end_of_stream: true,
        abort: false,
        sender_identity: claimed.map(str::to_string),
        client_epoch: 1,
    }
}
