//! Minimal LSP JSON-RPC framing (`Content-Length` headers) over byte streams.
//!
//! This is a deterministic codec — the transport itself is a `tddy-task` channel. It is
//! shared by the client and by the test fake server.

use serde_json::Value;

/// Encode a JSON-RPC message with an LSP `Content-Length` header.
pub fn encode_message(msg: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("serialize json-rpc message");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Incremental reader that extracts complete `Content-Length`-framed JSON-RPC messages
/// from a byte stream, buffering partial frames across pushes.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    /// A fresh, empty reader.
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append freshly-read bytes to the internal buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pop the next complete message, or `None` if the buffer holds no full frame yet.
    pub fn next_message(&mut self) -> Option<Value> {
        let header_end = find_subsequence(&self.buf, b"\r\n\r\n")?;
        let content_length = parse_content_length(&self.buf[..header_end])?;
        let body_start = header_end + 4;
        if self.buf.len() < body_start + content_length {
            return None;
        }
        let body = self.buf[body_start..body_start + content_length].to_vec();
        self.buf.drain(..body_start + content_length);
        serde_json::from_slice(&body).ok()
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn parse_content_length(header: &[u8]) -> Option<usize> {
    let header = std::str::from_utf8(header).ok()?;
    for line in header.split("\r\n") {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            return rest.trim().parse().ok();
        }
    }
    None
}
