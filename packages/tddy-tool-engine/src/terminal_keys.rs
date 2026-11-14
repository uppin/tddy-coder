//! The bytes `SendSessionKeys` types into a terminal: text, then named keys, in order
//! (`#home-inbox` 5/8).
//!
//! Pure: no terminal, no lease. The tool reads whether the target has enabled bracketed paste
//! (`tddy_task::TextGrid::bracketed_paste`, `#home-inbox` 2/8) and hands it in, so a multi-line
//! text arrives as one paste rather than as a series of Enter presses.
//!
//! **Key names:** `Enter`, `Escape`, `Tab`, `BTab`, `Backspace`, `Delete`, `Up`, `Down`, `Left`,
//! `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Space`, `C-a` … `C-z`, `F1` … `F12`. Names are
//! matched exactly: case matters, and nothing outside the table is accepted.
//!
//! The key-name table itself lives in `tddy-coding-agent-specs` ([`named_key_bytes`]), so an agent
//! spec's `turn_input.submit_key` is validated at load against this same list.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use tddy_coding_agent_specs::terminal_keys::named_key_bytes;

/// Start and end of a bracketed paste, as a terminal that enabled mode 2004 expects them.
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// The bytes for `text` (when given) followed by each of `keys`, or why none are produced.
///
/// `text` goes first: wrapped in the bracketed-paste markers when `bracketed_paste` is true, raw
/// otherwise, where each line break is typed as Enter. A paste is carried verbatim, so a `text`
/// holding the paste-end sequence is refused rather than altered. Every key name is checked before any byte is
/// returned, so an unknown name is an error naming it and nothing is half-encoded. A request with no
/// text and no keys is refused, since it would type nothing.
pub fn encode_keys(
    text: Option<&str>,
    keys: &[String],
    bracketed_paste: bool,
) -> Result<Vec<u8>, String> {
    let text = text.filter(|text| !text.is_empty());
    if text.is_none() && keys.is_empty() {
        return Err("nothing to send: give text, keys or both".to_string());
    }
    if bracketed_paste && text.is_some_and(contains_paste_end) {
        return Err(
            "text contains the paste-end sequence ESC[201~, which would end the paste early and \
             type the rest raw: send that part as keys instead"
                .to_string(),
        );
    }
    let key_bytes = keys
        .iter()
        .map(|name| {
            named_key_bytes(name).ok_or_else(|| {
                format!(
                    "unknown key name {name:?}: key names are case-sensitive, as in Enter, C-c or F1"
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut typed = Vec::new();
    if let Some(text) = text {
        typed.extend(text_bytes(text, bracketed_paste));
    }
    for bytes in key_bytes {
        typed.extend(bytes);
    }
    Ok(typed)
}

/// Whether `text` carries [`PASTE_END`], which a paste cannot hold verbatim.
fn contains_paste_end(text: &str) -> bool {
    text.as_bytes()
        .windows(PASTE_END.len())
        .any(|window| window == PASTE_END)
}

/// The bytes typing `text` produces: one paste carrying it verbatim when the terminal takes one,
/// otherwise the text as typed, each line break as the Enter that ends it.
fn text_bytes(text: &str, bracketed_paste: bool) -> Vec<u8> {
    if bracketed_paste {
        let mut bytes = Vec::with_capacity(PASTE_START.len() + text.len() + PASTE_END.len());
        bytes.extend_from_slice(PASTE_START);
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(PASTE_END);
        bytes
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}
