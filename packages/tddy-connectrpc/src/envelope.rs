//! Connect streaming envelope framing.
//!
//! Format: [flags:1][length:4be][payload]
//! Flags: 0x00 = message, 0x01 = compressed, 0x02 = end-stream

pub mod flags {
    pub const MESSAGE: u8 = 0x00;
    pub const COMPRESSED: u8 = 0x01;
    pub const END_STREAM: u8 = 0x02;
}

pub const ENVELOPE_HEADER_SIZE: usize = 5;

/// Wrap payload in a Connect streaming frame envelope.
pub fn wrap_envelope(payload: &[u8], compressed: bool) -> Vec<u8> {
    let flags = if compressed {
        flags::COMPRESSED
    } else {
        flags::MESSAGE
    };
    let mut frame = Vec::with_capacity(ENVELOPE_HEADER_SIZE + payload.len());
    frame.push(flags);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Create end-stream frame. Payload is typically empty or JSON metadata.
pub fn wrap_end_stream(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(ENVELOPE_HEADER_SIZE + payload.len());
    frame.push(flags::END_STREAM);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Parse envelope header from bytes. Returns (flags, length) or None if insufficient bytes.
pub fn parse_envelope_header(data: &[u8]) -> Option<(u8, u32)> {
    if data.len() < ENVELOPE_HEADER_SIZE {
        return None;
    }
    let flags = data[0];
    let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    Some((flags, length))
}

/// Parse envelope-framed body into message payloads.
/// Skips end-stream frames. Returns error message if framing is invalid.
pub fn parse_envelope_frames(body: &[u8]) -> Result<Vec<Vec<u8>>, &'static str> {
    let mut messages = Vec::new();
    let mut pos = 0;
    while pos < body.len() {
        let (flags, length) = match parse_envelope_header(&body[pos..]) {
            Some(h) => h,
            None => return Err("incomplete envelope header"),
        };
        pos += ENVELOPE_HEADER_SIZE;
        let end = pos + length as usize;
        if end > body.len() {
            return Err("envelope payload extends past body");
        }
        let payload = &body[pos..end];
        pos = end;
        if flags == flags::END_STREAM {
            break;
        }
        if flags == flags::MESSAGE || flags == flags::COMPRESSED {
            messages.push(payload.to_vec());
        }
        // Unknown flags: skip (or could error; Connect spec says end-stream only)
    }
    Ok(messages)
}
