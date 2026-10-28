//! The bytes `SendSessionKeys` types into a terminal: text, then named keys, in order
//! (`#home-inbox` 5/8).
//!
//! Pure: no terminal, no lease. The tool reads whether the target has enabled bracketed paste
//! (`tddy_task::TextGrid::bracketed_paste`, `#home-inbox` 2/8) and hands it in, so a multi-line
//! text arrives as one paste rather than as a series of Enter presses.
//!
//! **Key names:** `Enter`, `Escape`, `Tab`, `BTab`, `Backspace`, `Delete`, `Up`, `Down`, `Left`,
//! `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Space`, `C-a` … `C-z`, `F1` … `F12`.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

/// The bytes for `text` (when given) followed by each of `keys`, or why none are produced.
///
/// `text` goes first: wrapped in the bracketed-paste markers when `bracketed_paste` is true, raw
/// otherwise. Every key name is checked before any byte is returned, so an unknown name is an
/// error naming it and nothing is half-encoded.
pub fn encode_keys(
    text: Option<&str>,
    keys: &[String],
    bracketed_paste: bool,
) -> Result<Vec<u8>, String> {
    // TODO(terminal-keys): implement — the named-key table, `C-a`…`C-z`, `F1`…`F12`, and the
    // bracketed-paste wrapping of `text`.
    let _ = (text, keys, bracketed_paste);
    Err("encode_keys: not implemented".to_string())
}
