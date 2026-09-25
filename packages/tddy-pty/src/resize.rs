//! The in-band resize a terminal client writes into a PTY's input: the OSC sequence
//! `\x1b]resize;{cols};{rows}\x07`.
//!
//! Every host that pumps client bytes into a PTY decodes it the same way — the daemon's CLI
//! sessions, `tddy-coder`'s session participant and the sandbox runner inside each jail — so the
//! decoder lives here, beside the PTY it resizes.

use bytes::Bytes;

/// Strip an OSC resize sequence (`\x1b]resize;{cols};{rows}\x07`) from `data`.
///
/// Returns `(Some((cols, rows)), remaining)` when found, or `(None, original)` otherwise.
/// The escape sequence is removed from the returned bytes so it is not forwarded to the PTY stdin.
#[must_use]
pub fn strip_resize(data: &[u8]) -> (Option<(u16, u16)>, Bytes) {
    let prefix = b"\x1b]resize;";
    let start = match (0..data.len().saturating_sub(prefix.len()))
        .find(|&i| data[i..].starts_with(prefix))
    {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let after = &data[start + prefix.len()..];
    let bel = match after.iter().position(|&b| b == 0x07) {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let inner = &after[..bel];
    let semi = match inner.iter().position(|&b| b == b';') {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let parsed = std::str::from_utf8(&inner[..semi])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .zip(
            std::str::from_utf8(&inner[semi + 1..])
                .ok()
                .and_then(|s| s.parse::<u16>().ok()),
        );
    match parsed {
        Some((cols, rows)) => {
            let end = start + prefix.len() + bel + 1;
            let mut remaining = data[..start].to_vec();
            remaining.extend_from_slice(&data[end..]);
            (Some((cols, rows)), Bytes::from(remaining))
        }
        None => (None, Bytes::copy_from_slice(data)),
    }
}
