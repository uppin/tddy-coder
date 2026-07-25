//! Bounded replay capture for terminal byte streams.
//!
//! A [`TerminalCapture`] keeps the most recent [`TerminalCapture::CAPTURE_LIMIT_BYTES`] of output so
//! a late subscriber can be shown what is currently on screen. Bytes are evicted from the front,
//! which means any escape sequence that switched the terminal into a sticky mode is eventually
//! lost — a client attaching afterwards would never learn about it.
//!
//! Mouse tracking is the case that breaks: a terminal only reports clicks, drags and scrolls while
//! the mode is enabled, and a redraw (SIGWINCH) does not re-emit private modes. The capture
//! therefore sniffs DECSET/DECRST as bytes stream through and can re-issue the mouse-tracking modes
//! still in effect as a prologue in front of the retained output.

use std::collections::BTreeSet;

/// Private modes that control mouse reporting: normal tracking (1000), button-event tracking
/// (1002), any-event tracking (1003), and the SGR / urxvt / SGR-pixel encodings (1006, 1015, 1016).
///
/// Other sticky private modes (cursor visibility, alternate screen, bracketed paste, …) are
/// deliberately not replayed: the application redraws them, and re-issuing them out of context
/// would fight the application's own state.
const MOUSE_TRACKING_MODES: [u16; 6] = [1000, 1002, 1003, 1006, 1015, 1016];

/// Rolling window of terminal output plus the mouse-tracking modes currently in effect.
pub struct TerminalCapture {
    /// Retained output, trimmed from the front, never longer than [`Self::CAPTURE_LIMIT_BYTES`].
    buffer: Vec<u8>,
    /// Mouse-tracking modes the application has enabled and not turned back off.
    enabled_mouse_modes: BTreeSet<u16>,
    /// Parses the appended stream to observe DECSET/DECRST across arbitrary chunk boundaries.
    sniffer: EscapeParser,
    /// Parses the evicted prefix so trimming can tell whether it cut an escape sequence in half.
    evicted: EscapeParser,
}

impl TerminalCapture {
    /// Maximum bytes of output retained for replay.
    pub const CAPTURE_LIMIT_BYTES: usize = 64 * 1024;

    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            enabled_mouse_modes: BTreeSet::new(),
            sniffer: EscapeParser::new(),
            evicted: EscapeParser::new(),
        }
    }

    /// Record freshly produced output, updating the mode state and evicting the oldest bytes.
    pub fn append(&mut self, data: &[u8]) {
        self.sniff_mouse_modes(data);
        self.buffer.extend_from_slice(data);
        self.evict_to_limit();
    }

    /// Everything a late subscriber needs: the modes still in effect, then the retained output.
    pub fn replay(&self) -> Vec<u8> {
        let mut replay = self.mode_prologue();
        replay.extend_from_slice(&self.buffer);
        replay
    }

    /// DECSETs that put a fresh terminal back into the mouse-tracking modes now in effect,
    /// in ascending mode order.
    pub fn mode_prologue(&self) -> Vec<u8> {
        let mut prologue = Vec::new();
        for mode in &self.enabled_mouse_modes {
            prologue.extend_from_slice(format!("\x1b[?{mode}h").as_bytes());
        }
        prologue
    }

    /// Retained output bytes, without the mode prologue.
    pub fn buffered_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Track the latest DECSET/DECRST for every mouse-tracking mode the application touches.
    fn sniff_mouse_modes(&mut self, data: &[u8]) {
        for &byte in data {
            let Some(update) = self.sniffer.feed(byte) else {
                continue;
            };
            apply_mouse_modes(&mut self.enabled_mouse_modes, update);
        }
    }

    /// Evict the oldest bytes down to the limit, then keep going to the end of an escape sequence
    /// the eviction cut in half — an orphan fragment would be rendered as text by the client.
    fn evict_to_limit(&mut self) {
        let mut evict = self.buffer.len().saturating_sub(Self::CAPTURE_LIMIT_BYTES);
        for &byte in &self.buffer[..evict] {
            self.evicted.feed(byte);
        }
        while !self.evicted.at_sequence_boundary() && evict < self.buffer.len() {
            self.evicted.feed(self.buffer[evict]);
            evict += 1;
        }
        if evict > 0 {
            self.buffer.drain(..evict);
        }
    }
}

impl Default for TerminalCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// A completed DECSET (`ESC[?<modes>h`) or DECRST (`ESC[?<modes>l`).
struct PrivateModeUpdate<'a> {
    modes: &'a [u16],
    enabled: bool,
}

/// Fold a completed private-mode change into the sticky set, ignoring non-mouse modes.
fn apply_mouse_modes(enabled_modes: &mut BTreeSet<u16>, update: PrivateModeUpdate<'_>) {
    let mouse_modes = update
        .modes
        .iter()
        .filter(|mode| MOUSE_TRACKING_MODES.contains(mode));
    for mode in mouse_modes {
        if update.enabled {
            enabled_modes.insert(*mode);
        } else {
            enabled_modes.remove(mode);
        }
    }
}

/// Where in an escape sequence the byte stream currently is.
enum ParserState {
    /// Between sequences — plain output.
    Ground,
    /// `ESC` seen; the next byte selects the sequence type.
    Escape,
    /// `ESC` plus an intermediate byte, awaiting the final byte.
    EscapeIntermediate,
    /// Inside a CSI sequence (`ESC[…`), collecting parameters.
    Csi,
    /// Inside a string sequence (OSC/DCS/SOS/PM/APC), awaiting `BEL` or `ST`.
    StringPayload,
    /// `ESC` seen inside a string payload; `\` completes the `ST` terminator.
    StringPayloadEscape,
}

/// Longest parameter list recorded from a CSI sequence; anything longer is not a mode sequence.
const MAX_CSI_PARAMS: usize = 16;

/// Incremental escape-sequence parser: reports DECSET/DECRST and where sequences end.
///
/// Fed one byte at a time so a sequence split across reads is still recognised.
struct EscapeParser {
    state: ParserState,
    /// Numeric parameters of the CSI sequence being parsed.
    params: Vec<u16>,
    /// Digits of the parameter currently being read.
    current_param: u32,
    /// Whether `current_param` has any digits (distinguishes `0` from an omitted parameter).
    reading_param: bool,
    /// Whether the CSI sequence carried the `?` private-mode marker.
    private: bool,
}

impl EscapeParser {
    fn new() -> Self {
        Self {
            state: ParserState::Ground,
            params: Vec::new(),
            current_param: 0,
            reading_param: false,
            private: false,
        }
    }

    /// Whether the stream is between escape sequences (so it may be cut here safely).
    fn at_sequence_boundary(&self) -> bool {
        matches!(self.state, ParserState::Ground)
    }

    /// Consume the next byte, reporting a private-mode change when one completes.
    fn feed(&mut self, byte: u8) -> Option<PrivateModeUpdate<'_>> {
        match self.state {
            ParserState::Ground => {
                if byte == ESC {
                    self.state = ParserState::Escape;
                }
                None
            }
            ParserState::Escape => {
                self.state = match byte {
                    b'[' => {
                        self.begin_csi();
                        ParserState::Csi
                    }
                    // OSC, DCS, SOS, PM, APC all carry a payload terminated by BEL or ST.
                    b']' | b'P' | b'X' | b'^' | b'_' => ParserState::StringPayload,
                    ESC => ParserState::Escape,
                    b if INTERMEDIATE_BYTES.contains(&b) => ParserState::EscapeIntermediate,
                    // Any other byte is the final byte of a two-byte escape sequence.
                    _ => ParserState::Ground,
                };
                None
            }
            ParserState::EscapeIntermediate => {
                self.state = match byte {
                    ESC => ParserState::Escape,
                    b if INTERMEDIATE_BYTES.contains(&b) => ParserState::EscapeIntermediate,
                    _ => ParserState::Ground,
                };
                None
            }
            ParserState::Csi => self.feed_csi(byte),
            ParserState::StringPayload => {
                self.state = match byte {
                    BEL => ParserState::Ground,
                    ESC => ParserState::StringPayloadEscape,
                    _ => ParserState::StringPayload,
                };
                None
            }
            ParserState::StringPayloadEscape => {
                self.state = match byte {
                    b'\\' => ParserState::Ground,
                    ESC => ParserState::StringPayloadEscape,
                    _ => ParserState::StringPayload,
                };
                None
            }
        }
    }

    fn begin_csi(&mut self) {
        self.params.clear();
        self.current_param = 0;
        self.reading_param = false;
        self.private = false;
    }

    /// Parameter bytes accumulate until a final byte (`0x40..=0x7e`) closes the sequence.
    fn feed_csi(&mut self, byte: u8) -> Option<PrivateModeUpdate<'_>> {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self
                    .current_param
                    .saturating_mul(10)
                    .saturating_add(u32::from(byte - b'0'));
                self.reading_param = true;
                None
            }
            b if PARAM_SEPARATORS.contains(&b) => {
                self.finish_param();
                None
            }
            b'?' => {
                self.private = true;
                None
            }
            ESC => {
                self.state = ParserState::Escape;
                None
            }
            b if CSI_FINAL_BYTES.contains(&b) => {
                self.state = ParserState::Ground;
                let enabled = match byte {
                    b'h' => true,
                    b'l' => false,
                    _ => return None,
                };
                if !self.private {
                    return None;
                }
                self.finish_param();
                Some(PrivateModeUpdate {
                    modes: &self.params,
                    enabled,
                })
            }
            // Private markers, intermediates and in-sequence control bytes leave the state alone.
            _ => None,
        }
    }

    fn finish_param(&mut self) {
        if !self.reading_param {
            return;
        }
        if self.params.len() < MAX_CSI_PARAMS {
            self.params
                .push(self.current_param.min(u32::from(u16::MAX)) as u16);
        }
        self.current_param = 0;
        self.reading_param = false;
    }
}

/// `ESC` — introduces every escape sequence.
const ESC: u8 = 0x1b;
/// `BEL` — one of the two accepted terminators of a string payload.
const BEL: u8 = 0x07;
/// Intermediate bytes of an escape sequence, e.g. the `(` of `ESC ( B`.
const INTERMEDIATE_BYTES: std::ops::RangeInclusive<u8> = 0x20..=0x2f;
/// Bytes that separate CSI parameters (`;`, and `:` inside sub-parameters).
const PARAM_SEPARATORS: std::ops::RangeInclusive<u8> = 0x3a..=0x3b;
/// Bytes that terminate a CSI sequence.
const CSI_FINAL_BYTES: std::ops::RangeInclusive<u8> = 0x40..=0x7e;
