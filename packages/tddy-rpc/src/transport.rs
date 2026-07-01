//! Transport abstraction shared by every RPC transport.
//!
//! [`RpcClientTransport`] is the object-safe client-side abstraction: user code calls RPCs
//! through it without depending on which concrete transport (LiveKit `Room`, stdio pipes, ...)
//! implements it. The frame codec ([`FrameKind`], [`encode_frame`], [`FrameDecoder`]) is for
//! byte-stream transports that aren't already message-oriented (LiveKit's data channel delivers
//! whole packets; a stdio pipe does not) — a one-byte kind discriminates `Request` vs `Response`
//! frames on a single duplex channel, which is what lets one peer be both an RPC client and
//! server over the same channel.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::status::Status;

/// Object-safe client abstraction — the same call sites work against any concrete transport.
#[async_trait]
pub trait RpcClientTransport: Send + Sync {
    /// Call a unary RPC method. Returns the raw response bytes.
    async fn call_unary(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<Vec<u8>, Status>;

    /// Call a server-streaming RPC method. Returns a receiver for the response stream.
    async fn call_server_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>;

    /// Call a client-streaming RPC method: send multiple request messages, get one response.
    async fn call_client_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status>;

    /// Call a bidirectional-streaming RPC method with all request messages known up front.
    async fn call_bidi_stream(
        &self,
        service: &str,
        method: &str,
        request_bytes_list: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status>;
}

/// Discriminates which engine a decoded frame belongs to on a single duplex channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Request,
    Response,
}

impl FrameKind {
    fn to_byte(self) -> u8 {
        match self {
            FrameKind::Request => 0,
            FrameKind::Response => 1,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(FrameKind::Request),
            1 => Some(FrameKind::Response),
            _ => None,
        }
    }
}

const LENGTH_PREFIX_BYTES: usize = 4;
const KIND_BYTES: usize = 1;

/// Encode one frame: a 4-byte big-endian length (covering the kind byte and payload), then the
/// kind byte, then the payload.
pub fn encode_frame(kind: FrameKind, payload: &[u8]) -> Vec<u8> {
    let body_len = KIND_BYTES + payload.len();
    let mut buf = Vec::with_capacity(LENGTH_PREFIX_BYTES + body_len);
    buf.extend_from_slice(&(body_len as u32).to_be_bytes());
    buf.push(kind.to_byte());
    buf.extend_from_slice(payload);
    buf
}

/// Accumulates bytes fed from a byte-stream reader and yields complete frames — correctly
/// handling a frame split across multiple `feed` calls and multiple frames delivered in one call.
#[derive(Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append newly read bytes to the decoder's internal buffer.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Pop the next fully-buffered frame, if one is complete.
    pub fn next_frame(&mut self) -> Option<(FrameKind, Vec<u8>)> {
        if self.buffer.len() < LENGTH_PREFIX_BYTES {
            return None;
        }
        // Slice is exactly LENGTH_PREFIX_BYTES long (guarded above), so this can't fail.
        let body_len =
            u32::from_be_bytes(self.buffer[..LENGTH_PREFIX_BYTES].try_into().unwrap()) as usize;
        if body_len < KIND_BYTES {
            return None;
        }
        let frame_len = LENGTH_PREFIX_BYTES + body_len;
        if self.buffer.len() < frame_len {
            return None;
        }
        let kind = FrameKind::from_byte(self.buffer[LENGTH_PREFIX_BYTES])?;
        let payload = self.buffer[LENGTH_PREFIX_BYTES + KIND_BYTES..frame_len].to_vec();
        self.buffer.drain(..frame_len);
        Some((kind, payload))
    }
}
