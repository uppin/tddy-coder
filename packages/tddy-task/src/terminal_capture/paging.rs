//! Forward paging over a retained window of output.
//!
//! The live [`super::TerminalCapture`] and its on-disk [`crate::CaptureRecording`] hold the same
//! window, so they page it with this one function: a stopped session's history answers the same
//! chunks, with the same offsets, as the live terminal did.

use super::CaptureChunk;

/// Up to `max_bytes` from `from_offset` forward, bounded above by `until_offset` (0 = the window's
/// end), over `window` — the bytes at absolute offsets `window_start..window_end`.
///
/// `from_offset` is clamped up to `window_start` when older bytes are no longer retained.
pub(crate) fn forward_chunk(
    window: &[u8],
    window_start: u64,
    window_end: u64,
    from_offset: u64,
    until_offset: u64,
    max_bytes: usize,
) -> CaptureChunk {
    let chunk_start = from_offset.max(window_start);
    let cap = if until_offset == 0 {
        window_end
    } else {
        until_offset.min(window_end)
    };
    // Nothing to return once the fill has reached the upper bound.
    if chunk_start >= cap {
        return CaptureChunk {
            data: Vec::new(),
            start_offset: chunk_start,
            end_offset: chunk_start,
            at_oldest: chunk_start == window_start,
            at_end: true,
        };
    }
    let chunk_end = (chunk_start.saturating_add(max_bytes as u64)).min(cap);
    let buffer_start = (chunk_start - window_start) as usize;
    let buffer_end = (chunk_end - window_start) as usize;
    CaptureChunk {
        data: window[buffer_start..buffer_end].to_vec(),
        start_offset: chunk_start,
        end_offset: chunk_end,
        at_oldest: chunk_start == window_start,
        at_end: chunk_end >= cap,
    }
}
