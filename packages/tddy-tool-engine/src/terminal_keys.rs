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

// ── typing into a live terminal: the text, its echo, then the keys ───────────────────────────────

/// What typing a request writes, as the two writes it is typed in: the text (a paste when the
/// terminal takes one), then the keys. Either may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typing {
    pub text: Vec<u8>,
    pub keys: Vec<u8>,
}

impl Typing {
    /// The number of bytes both writes carry.
    pub fn len(&self) -> usize {
        self.text.len() + self.keys.len()
    }

    /// Whether the request types nothing.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// [`encode_keys`], kept as the two writes [`type_into`] types it in.
pub fn encode_typing(
    text: Option<&str>,
    keys: &[String],
    bracketed_paste: bool,
) -> Result<Typing, String> {
    let _ = (text, keys, bracketed_paste);
    todo!("encode_typing")
}

/// The terminal [`type_into`] types into: where its output stands, and a writer to its input.
#[async_trait::async_trait]
pub trait TypingTerminal: Send + Sync {
    /// The terminal's output sequence point (`TextGrid::offset`): it moves whenever the
    /// application writes to its screen.
    async fn output_offset(&self) -> Result<u64, String>;
    /// Write `bytes` to the terminal's input as one write.
    async fn write(&self, bytes: Vec<u8>) -> Result<(), String>;
}

/// How long [`type_into`] waits for the application to echo the text before it presses the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EchoWait {
    /// Give up waiting for any echo after this long, and press the keys anyway.
    pub no_echo_timeout: std::time::Duration,
    /// Once the echo starts, the output must stay this long without moving to count as settled.
    pub quiet: std::time::Duration,
    /// Press the keys this long after the echo started even if the output never settles (a TUI
    /// animating a spinner).
    pub settle_cap: std::time::Duration,
    /// How often the output offset is read.
    pub poll: std::time::Duration,
}

impl Default for EchoWait {
    fn default() -> Self {
        Self {
            no_echo_timeout: std::time::Duration::from_secs(5),
            quiet: std::time::Duration::from_millis(30),
            settle_cap: std::time::Duration::from_millis(500),
            poll: std::time::Duration::from_millis(10),
        }
    }
}

/// What [`type_into`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Typed {
    /// The bytes written, text and keys together.
    pub sent_bytes: usize,
    /// Whether the application echoed the text before the keys were pressed: `None` when nothing
    /// was waited on (no text, or no keys after it).
    pub echo_seen: Option<bool>,
}

/// Type `typing` into `terminal`: the text as one write, then — once the application has echoed
/// it — the keys as a write of their own.
pub async fn type_into(
    terminal: &dyn TypingTerminal,
    typing: Typing,
    wait: &EchoWait,
) -> Result<Typed, String> {
    let _ = (terminal, typing, wait);
    todo!("type_into")
}
