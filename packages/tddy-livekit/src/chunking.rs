//! Chunking codec for the LiveKit RPC transport.
//!
//! LiveKit's SCTP data channel negotiates a maximum message size (~64 KB); a single
//! `publish_data` call with a larger payload is rejected outright ("data packet size exceeds the
//! negotiated maximum message size"), so an oversized RPC envelope — e.g. a `StreamSessionActivity`
//! snapshot carrying a huge tool result — can never be delivered and the transport wedges,
//! retrying the same doomed publish forever.
//!
//! This module splits an encoded envelope into ordered wire frames that each fit within the
//! per-packet budget, and reassembles the received frames back into the original bytes. Frames of
//! concurrent logical messages (distinct `message_id`s) are grouped independently.
//!
//! # Wire format and back-compat
//!
//! A payload that already fits in one packet is sent **raw** — byte-for-byte the encoded envelope,
//! exactly as before chunking existed — so a peer that predates this codec still decodes small
//! messages unchanged. Only an oversized payload is split, and each of its frames begins with a
//! one-byte [`CHUNK_FRAME_MAGIC`] followed by the header:
//! `[magic: u8 = 0x00][message_id: u32 LE][total_chunks: u32 LE][index: u32 LE][data...]`.
//!
//! The receiver tells the two apart with [`is_chunk_frame`]: a valid `prost`-encoded
//! `RpcRequest`/`RpcResponse` can never begin with `0x00` (protobuf field number 0 is illegal, and
//! the envelopes' lowest fields are `request_id`/`response_message` at field numbers 1 and 2), so a
//! leading `0x00` unambiguously marks a chunk frame.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Safe upper bound for a single LiveKit data-channel message. Stays under the ~64 KB SCTP
/// negotiated maximum with headroom for the frame header.
pub const MAX_CHUNK_FRAME_BYTES: usize = 60_000;

/// Monotonic source of outbound message ids for this process. A `message_id` only needs to be
/// unique among one sender's concurrently in-flight messages (the receiver keeps a separate
/// reassembler per sender), and every send from this process shares one local participant identity,
/// so a single process-wide counter suffices. Wrap-around at `u32::MAX` is harmless — a collision
/// would require ~4 billion messages in flight at once.
static NEXT_MESSAGE_ID: AtomicU32 = AtomicU32::new(0);

/// Allocate the next process-unique `message_id` for an outbound message.
pub fn next_message_id() -> u32 {
    NEXT_MESSAGE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Leading byte marking a frame as a chunk frame rather than a raw envelope. `0x00` is safe because
/// a valid protobuf envelope never starts with it (see the module docs).
pub const CHUNK_FRAME_MAGIC: u8 = 0x00;

/// Encoded length of the chunk header prepended to every frame:
/// `[magic: u8][message_id: u32 LE][total_chunks: u32 LE][index: u32 LE]`.
const HEADER_LEN: usize = 13;

/// Number of payload bytes that fit in one frame of `max_frame_bytes` (the budget minus the frame
/// header). Callers split their payload into runs of at most this many bytes.
pub fn max_data_bytes_per_frame(max_frame_bytes: usize) -> usize {
    max_frame_bytes.saturating_sub(HEADER_LEN)
}

/// Prepare `payload` for the transport under the standard [`MAX_CHUNK_FRAME_BYTES`] budget: a
/// payload that fits in a single packet is returned **raw** (one frame, no header) for wire
/// compatibility with pre-chunking peers; a larger one is split into ordered chunk frames.
///
/// A short payload that happens to begin with [`CHUNK_FRAME_MAGIC`] is forced through the framed
/// path anyway, so the receiver never mistakes it for a chunk frame — callers may pass arbitrary
/// bytes, though in practice `payload` is always an encoded envelope (which never leads with the
/// magic).
pub fn frame_for_transport(message_id: u32, payload: &[u8]) -> Vec<Vec<u8>> {
    let fits_raw =
        payload.len() <= MAX_CHUNK_FRAME_BYTES && payload.first() != Some(&CHUNK_FRAME_MAGIC);
    if fits_raw {
        vec![payload.to_vec()]
    } else {
        split_into_frames(message_id, payload, MAX_CHUNK_FRAME_BYTES)
    }
}

/// Split `payload` into ordered chunk frames tagged with `message_id`. Each returned frame's encoded
/// length is at most `max_frame_bytes`; a payload that already fits in one frame's data budget
/// yields a single frame. Every frame carries the magic-prefixed chunk header so the receiver
/// reassembles uniformly — prefer [`frame_for_transport`] on the send path, which sends small
/// payloads raw for back-compat.
pub fn split_into_frames(message_id: u32, payload: &[u8], max_frame_bytes: usize) -> Vec<Vec<u8>> {
    let data_budget = max_data_bytes_per_frame(max_frame_bytes).max(1);

    // An empty payload still yields a single (header-only) frame; treat it as one empty chunk.
    let data_chunks: Vec<&[u8]> = if payload.is_empty() {
        vec![&[]]
    } else {
        payload.chunks(data_budget).collect()
    };
    let total_chunks = data_chunks.len();

    data_chunks
        .into_iter()
        .enumerate()
        .map(|(index, data)| {
            let mut frame = Vec::with_capacity(HEADER_LEN + data.len());
            frame.push(CHUNK_FRAME_MAGIC);
            frame.extend_from_slice(&message_id.to_le_bytes());
            frame.extend_from_slice(&(total_chunks as u32).to_le_bytes());
            frame.extend_from_slice(&(index as u32).to_le_bytes());
            frame.extend_from_slice(data);
            frame
        })
        .collect()
}

/// Whether `bytes` is a chunk frame (to be reassembled) rather than a raw envelope (to be decoded
/// directly). See the module docs for why a leading [`CHUNK_FRAME_MAGIC`] is unambiguous.
pub fn is_chunk_frame(bytes: &[u8]) -> bool {
    bytes.first() == Some(&CHUNK_FRAME_MAGIC)
}

/// Error decoding or reassembling a chunk frame.
#[derive(Debug, PartialEq, Eq)]
pub enum ChunkError {
    /// The frame bytes could not be parsed as a chunk frame (too short, wrong magic, or an
    /// inconsistent header).
    Malformed(String),
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkError::Malformed(msg) => write!(f, "malformed chunk frame: {msg}"),
        }
    }
}

impl std::error::Error for ChunkError {}

/// Chunks buffered for one in-flight message, indexed by their `index` in the original payload.
struct PendingMessage {
    total_chunks: usize,
    chunks: HashMap<u32, Vec<u8>>,
}

/// Stateful reassembler. Feed each received chunk frame (those for which [`is_chunk_frame`] is true)
/// via [`ChunkReassembler::accept`]; it returns `Some(payload)` once the final frame of a message
/// arrives, `None` while chunks are still outstanding. Concurrent messages (distinct `message_id`s)
/// reassemble independently.
#[derive(Default)]
pub struct ChunkReassembler {
    pending: HashMap<u32, PendingMessage>,
}

impl ChunkReassembler {
    /// Accept one received chunk frame. Returns the completed payload when the final outstanding
    /// chunk of its message arrives, otherwise `None`.
    pub fn accept(&mut self, frame: &[u8]) -> Result<Option<Vec<u8>>, ChunkError> {
        if frame.len() < HEADER_LEN {
            return Err(ChunkError::Malformed(format!(
                "frame is {} bytes, shorter than the {HEADER_LEN}-byte header",
                frame.len()
            )));
        }
        if frame[0] != CHUNK_FRAME_MAGIC {
            return Err(ChunkError::Malformed(format!(
                "frame does not begin with the chunk magic (0x{:02x} != 0x{CHUNK_FRAME_MAGIC:02x})",
                frame[0]
            )));
        }

        let message_id = u32::from_le_bytes(frame[1..5].try_into().expect("4 bytes"));
        let total_chunks = u32::from_le_bytes(frame[5..9].try_into().expect("4 bytes")) as usize;
        let index = u32::from_le_bytes(frame[9..13].try_into().expect("4 bytes"));
        let data = frame[HEADER_LEN..].to_vec();

        let pending = self
            .pending
            .entry(message_id)
            .or_insert_with(|| PendingMessage {
                total_chunks,
                chunks: HashMap::new(),
            });
        pending.chunks.insert(index, data);

        if pending.chunks.len() < pending.total_chunks {
            return Ok(None);
        }

        let mut pending = self.pending.remove(&message_id).expect("just inserted");
        let mut payload = Vec::new();
        for index in 0..pending.total_chunks as u32 {
            let chunk = pending.chunks.remove(&index).ok_or_else(|| {
                ChunkError::Malformed(format!("message {message_id} is missing chunk {index}"))
            })?;
            payload.extend_from_slice(&chunk);
        }
        Ok(Some(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A deterministic payload of `len` bytes whose value varies per position (period 251, a prime
    /// that won't align with chunk boundaries), so any dropped, duplicated, or reordered chunk
    /// changes the reassembled bytes. Models an oversized RPC envelope — the ~370 KB
    /// `StreamSessionActivity` snapshot that wedged the transport. Starts at 1 (never the chunk
    /// magic), matching a real encoded envelope's leading protobuf tag.
    fn a_payload_of(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251 + 1) as u8).collect()
    }

    /// A stand-in for a raw encoded `RpcResponse`: begins with `0x08`, the protobuf tag for the
    /// `request_id` field (field 1, varint) that `prost` emits first.
    fn an_encoded_envelope() -> Vec<u8> {
        vec![0x08, 0x2a, 0x12, 0x03, b'h', b'i', b'!']
    }

    fn assert_frames_fit(frames: &[Vec<u8>], budget: usize) {
        for (i, frame) in frames.iter().enumerate() {
            assert!(
                frame.len() <= budget,
                "frame {i} is {} bytes, over the {budget}-byte budget",
                frame.len()
            );
        }
    }

    /// Feed frames into `reassembler`, returning every payload that completed (in completion order).
    fn feed_all(reassembler: &mut ChunkReassembler, frames: &[Vec<u8>]) -> Vec<Vec<u8>> {
        frames
            .iter()
            .filter_map(|f| reassembler.accept(f).expect("valid frame"))
            .collect()
    }

    /// Round-robins two frame sequences so chunks of both messages arrive intermixed, exercising the
    /// reassembler's per-`message_id` grouping.
    fn interleave(a: Vec<Vec<u8>>, b: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut ai = a.into_iter();
        let mut bi = b.into_iter();
        loop {
            let x = ai.next();
            let y = bi.next();
            if x.is_none() && y.is_none() {
                break;
            }
            out.extend(x);
            out.extend(y);
        }
        out
    }

    #[test]
    fn keeps_a_payload_that_already_fits_in_a_single_frame() {
        // Given a payload well under the per-frame budget
        let payload = a_payload_of(500);

        // When splitting it with a 60_000-byte budget
        let frames = split_into_frames(7, &payload, 60_000);

        // Then it is not chunked
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn splits_an_oversized_payload_into_frames_that_each_fit_the_budget() {
        // Given a 370 KB payload — the size that overflowed LiveKit's 64 KB packet limit
        let payload = a_payload_of(370_000);

        // When splitting it into 60_000-byte frames
        let frames = split_into_frames(1, &payload, 60_000);

        // Then every frame fits within the budget
        assert_frames_fit(&frames, 60_000);
    }

    #[test]
    fn splits_a_payload_into_the_expected_number_of_frames() {
        // Given a payload just over two data-chunks long for a 1_000-byte budget
        let per_frame = max_data_bytes_per_frame(1_000);
        let payload = a_payload_of(per_frame * 2 + 1);

        // When splitting it
        let frames = split_into_frames(1, &payload, 1_000);

        // Then it produces exactly three frames
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn reassembles_a_split_payload_back_into_the_original_bytes() {
        // Given a 370 KB payload split into 60_000-byte frames
        let payload = a_payload_of(370_000);
        let frames = split_into_frames(1, &payload, 60_000);

        // When the frames are reassembled through a fresh reassembler
        let mut reassembler = ChunkReassembler::default();
        let completed = feed_all(&mut reassembler, &frames);

        // Then the original bytes are recovered exactly
        assert_eq!(completed, vec![payload]);
    }

    #[test]
    fn yields_nothing_until_the_final_chunk_of_a_message_arrives() {
        // Given a payload that splits into more than one frame
        let payload = a_payload_of(2_500);
        let frames = split_into_frames(5, &payload, 1_000);
        let mut reassembler = ChunkReassembler::default();

        // When all but the final frame are fed in
        let before_final = feed_all(&mut reassembler, &frames[..frames.len() - 1]);
        // And then the final frame arrives
        let on_final = reassembler
            .accept(frames.last().expect("at least one frame"))
            .expect("valid frame");

        // Then nothing completed before the final frame, which completes the message
        assert_eq!(before_final, Vec::<Vec<u8>>::new());
        assert_eq!(on_final, Some(payload));
    }

    #[test]
    fn reassembles_two_interleaved_messages_independently() {
        // Given two distinct payloads chunked under a small budget
        let first = a_payload_of(2_500);
        let second = a_payload_of(3_100);
        let interleaved = interleave(
            split_into_frames(11, &first, 1_000),
            split_into_frames(22, &second, 1_000),
        );

        // When the interleaved frames are all fed to one reassembler
        let mut reassembler = ChunkReassembler::default();
        let completed = feed_all(&mut reassembler, &interleaved);

        // Then both messages reassemble to their own original bytes
        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&first), "first message not reassembled");
        assert!(
            completed.contains(&second),
            "second message not reassembled"
        );
    }

    #[test]
    fn rejects_a_frame_too_short_to_contain_a_header() {
        // Given a frame with fewer bytes than the chunk header requires
        let truncated = vec![CHUNK_FRAME_MAGIC; 1];

        // When it is fed to the reassembler
        let result = ChunkReassembler::default().accept(&truncated);

        // Then it is rejected as malformed
        assert!(matches!(result, Err(ChunkError::Malformed(_))));
    }

    #[test]
    fn rejects_a_frame_that_lacks_the_chunk_magic() {
        // Given a full-length frame whose leading byte is not the chunk magic
        let mut not_a_chunk = split_into_frames(1, &a_payload_of(2_000), 1_000)[0].clone();
        not_a_chunk[0] = 0x08;

        // When it is fed to the reassembler
        let result = ChunkReassembler::default().accept(&not_a_chunk);

        // Then it is rejected as malformed
        assert!(matches!(result, Err(ChunkError::Malformed(_))));
    }

    #[test]
    fn detects_split_frames_as_chunk_frames() {
        // Given the frames of an oversized payload
        let frames = split_into_frames(3, &a_payload_of(200_000), 60_000);

        // Then every frame is recognised as a chunk frame
        assert!(frames.iter().all(|f| is_chunk_frame(f)));
    }

    #[test]
    fn does_not_detect_a_raw_envelope_as_a_chunk_frame() {
        // Given the bytes of a raw encoded envelope
        let envelope = an_encoded_envelope();

        // Then it is not mistaken for a chunk frame
        assert!(!is_chunk_frame(&envelope));
    }

    #[test]
    fn sends_a_payload_that_fits_as_a_single_raw_frame() {
        // Given an encoded envelope that fits within one packet
        let envelope = an_encoded_envelope();

        // When preparing it for the transport
        let frames = frame_for_transport(1, &envelope);

        // Then it is sent raw — one frame, unchanged bytes, no chunk header
        assert_eq!(frames, vec![envelope.clone()]);
        assert!(!is_chunk_frame(&frames[0]));
    }

    #[test]
    fn frames_an_oversized_payload_into_detectable_chunks_that_reassemble() {
        // Given a 370 KB envelope that overflows a single packet
        let payload = a_payload_of(370_000);

        // When preparing it for the transport
        let frames = frame_for_transport(9, &payload);

        // Then it is split into fitting chunk frames that reassemble to the original
        assert!(frames.len() > 1);
        assert_frames_fit(&frames, MAX_CHUNK_FRAME_BYTES);
        assert!(frames.iter().all(|f| is_chunk_frame(f)));
        let completed = feed_all(&mut ChunkReassembler::default(), &frames);
        assert_eq!(completed, vec![payload]);
    }

    #[test]
    fn hands_out_distinct_message_ids() {
        // When several outbound message ids are allocated
        let ids = [next_message_id(), next_message_id(), next_message_id()];

        // Then no two collide
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            3
        );
    }

    #[test]
    fn frames_a_small_payload_that_would_look_like_a_chunk_frame() {
        // Given a short payload whose first byte collides with the chunk magic
        let payload = vec![CHUNK_FRAME_MAGIC, 1, 2, 3, 4];

        // When preparing it for the transport
        let frames = frame_for_transport(2, &payload);

        // Then it is framed (not sent raw) so the receiver won't misread it, and it round-trips
        assert!(is_chunk_frame(&frames[0]));
        let completed = feed_all(&mut ChunkReassembler::default(), &frames);
        assert_eq!(completed, vec![payload]);
    }
}
