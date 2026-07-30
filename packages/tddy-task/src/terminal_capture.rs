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
    /// Absolute byte offset of `buffer[0]` within the cumulative output stream. Rises monotonically
    /// as eviction drains the oldest retained bytes; never exceeds `end_offset`.
    start_offset: u64,
    /// Absolute byte offset just past `buffer[len-1]` — the total bytes ever appended. Rises
    /// monotonically with every `append`; the live tip a late subscriber attaches to.
    end_offset: u64,
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

    /// How far past the limit eviction may chase the end of a cut escape sequence.
    ///
    /// A sequence a client can render is at most a few dozen bytes, so this bound is only ever
    /// reached by a payload the application never terminated (an OSC missing its `ST`, say).
    /// Without it the chase would run to the end of the buffer and empty the ring, leaving a late
    /// subscriber a blank screen; giving up leaves a fragment instead, which is the better trade.
    pub const MAX_SEQUENCE_CHASE_BYTES: usize = 1024;

    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            start_offset: 0,
            end_offset: 0,
            enabled_mouse_modes: BTreeSet::new(),
            sniffer: EscapeParser::new(),
            evicted: EscapeParser::new(),
        }
    }

    /// Absolute byte offset of the oldest retained byte in the ring. Rises as eviction drains
    /// the front of the buffer; the lower bound for lazy history requests.
    pub fn start_offset(&self) -> u64 {
        self.start_offset
    }

    /// Absolute byte offset just past the newest retained byte — the total bytes ever appended,
    /// and the live tip a reconnecting subscriber attaches to.
    pub fn end_offset(&self) -> u64 {
        self.end_offset
    }

    /// Record freshly produced output, updating the mode state and evicting the oldest bytes.
    pub fn append(&mut self, data: &[u8]) {
        self.sniff_mouse_modes(data);
        self.buffer.extend_from_slice(data);
        self.end_offset = self.end_offset.saturating_add(data.len() as u64);
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

    /// Track the latest DECSET/DECRST for every mouse-tracking mode the application touches, and
    /// forget them all when the application hard-resets the terminal.
    fn sniff_mouse_modes(&mut self, data: &[u8]) {
        for &byte in data {
            let Some(event) = self.sniffer.feed(byte) else {
                continue;
            };
            match event {
                StreamEvent::PrivateMode(update) => {
                    apply_mouse_modes(&mut self.enabled_mouse_modes, update)
                }
                StreamEvent::FullReset => self.enabled_mouse_modes.clear(),
            }
        }
    }

    /// Evict the oldest bytes down to the limit, then keep going to the end of an escape sequence
    /// the eviction cut in half — an orphan fragment would be rendered as text by the client.
    ///
    /// The chase stops after [`Self::MAX_SEQUENCE_CHASE_BYTES`] so an unterminated payload cannot
    /// consume the whole ring.
    fn evict_to_limit(&mut self) {
        let overflow = self.buffer.len().saturating_sub(Self::CAPTURE_LIMIT_BYTES);
        if overflow == 0 {
            return;
        }
        for &byte in &self.buffer[..overflow] {
            self.evicted.feed(byte);
        }
        let chase_limit = (overflow + Self::MAX_SEQUENCE_CHASE_BYTES).min(self.buffer.len());
        let mut evict = overflow;
        while !self.evicted.at_sequence_boundary() && evict < chase_limit {
            self.evicted.feed(self.buffer[evict]);
            evict += 1;
        }
        self.buffer.drain(..evict);
        self.start_offset = self.start_offset.saturating_add(evict as u64);
    }

    /// The most recent `min(max_bytes, buffered_len)` bytes of output, with their absolute offsets
    /// and whether the chunk reaches the ring's oldest retained byte (no older history to load).
    ///
    /// This is the "current last frame" a reconnecting subscriber sees first; older bytes are
    /// fetched on demand via [`Self::replay_from`] (forward fill) as the user scrolls up.
    pub fn replay_last(&self, max_bytes: usize) -> CaptureChunk {
        let buffered_len = self.buffer.len();
        let take = max_bytes.min(buffered_len);
        let start = buffered_len - take;
        let data = self.buffer[start..].to_vec();
        let chunk_start = self.start_offset + start as u64;
        let chunk_end = self.end_offset;
        CaptureChunk {
            data,
            start_offset: chunk_start,
            end_offset: chunk_end,
            at_oldest: start == 0,
            // The last frame always reaches the capture tip, so it terminates a forward fill.
            at_end: true,
        }
    }

    /// Up to `max_bytes` of output ending just before `before_offset`, clamped to the ring's
    /// [`Self::start_offset`]. `before_offset = 0` means "from the live tip" (uses `end_offset`).
    ///
    /// Returns an empty `at_oldest` chunk when no older bytes are retained below `before_offset`
    /// — the signal that terminates an infinite-scroll-up. `at_end` is `false` on backward chunks
    /// (they never terminate a forward fill); the forward-fill path uses [`Self::replay_from`].
    pub fn replay_before(&self, before_offset: u64, max_bytes: usize) -> CaptureChunk {
        let upper = before_offset.min(self.end_offset);
        if upper <= self.start_offset {
            return CaptureChunk {
                data: Vec::new(),
                start_offset: self.start_offset,
                end_offset: self.start_offset,
                at_oldest: true,
                at_end: false,
            };
        }
        let chunk_end = upper;
        let chunk_start = chunk_start_clamped(self.start_offset, chunk_end, max_bytes);
        let buffer_start = (chunk_start - self.start_offset) as usize;
        let buffer_end = (chunk_end - self.start_offset) as usize;
        CaptureChunk {
            data: self.buffer[buffer_start..buffer_end].to_vec(),
            start_offset: chunk_start,
            end_offset: chunk_end,
            at_oldest: chunk_start == self.start_offset,
            at_end: false,
        }
    }

    /// Up to `max_bytes` of output starting at `from_offset` and going FORWARD, bounded above by
    /// `until_offset` (the anchor learned from the initial replay frame; 0 means "until the capture
    /// tip"). `from_offset` is clamped UP to the ring's [`Self::start_offset`] when older bytes have
    /// been evicted.
    ///
    /// This serves the progressive, append-only forward fill of older history: the client advances
    /// `from_offset` to the previous chunk's `end_offset` and calls again until a chunk arrives with
    /// `at_end = true` (its `end_offset` reached `until_offset` or the capture tip). `at_oldest` is
    /// `true` on a chunk whose start sits at the ring's oldest retained byte (no bytes below it).
    pub fn replay_from(
        &self,
        from_offset: u64,
        until_offset: u64,
        max_bytes: usize,
    ) -> CaptureChunk {
        let chunk_start = from_offset.max(self.start_offset);
        let cap = if until_offset == 0 {
            self.end_offset
        } else {
            until_offset.min(self.end_offset)
        };
        // Nothing to return once the fill has reached the upper bound.
        if chunk_start >= cap {
            return CaptureChunk {
                data: Vec::new(),
                start_offset: chunk_start,
                end_offset: chunk_start,
                at_oldest: chunk_start == self.start_offset,
                at_end: true,
            };
        }
        let chunk_end = (chunk_start.saturating_add(max_bytes as u64)).min(cap);
        let buffer_start = (chunk_start - self.start_offset) as usize;
        let buffer_end = (chunk_end - self.start_offset) as usize;
        CaptureChunk {
            data: self.buffer[buffer_start..buffer_end].to_vec(),
            start_offset: chunk_start,
            end_offset: chunk_end,
            at_oldest: chunk_start == self.start_offset,
            at_end: chunk_end >= cap,
        }
    }
}

/// A contiguous slice of the capture ring returned by [`TerminalCapture::replay_last`],
/// [`TerminalCapture::replay_before`], or [`TerminalCapture::replay_from`], tagged with its
/// absolute offsets in the cumulative output stream and whether it reaches the oldest retained
/// byte / the forward-fill upper bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureChunk {
    /// The retained output bytes for this slice.
    pub data: Vec<u8>,
    /// Absolute byte offset of `data[0]` in the cumulative output stream.
    pub start_offset: u64,
    /// Absolute byte offset just past `data[len-1]`.
    pub end_offset: u64,
    /// True when no older retained bytes exist below this chunk — terminates lazy scroll-up.
    pub at_oldest: bool,
    /// True when this chunk reaches `until_offset` (or the capture tip) — terminates a forward fill.
    pub at_end: bool,
}

/// The absolute offset of the first byte in a `replay_before` chunk: as far back as `max_bytes`
/// allows, but never below the ring's `start_offset`.
fn chunk_start_clamped(start_offset: u64, chunk_end: u64, max_bytes: usize) -> u64 {
    let desired = chunk_end.saturating_sub(max_bytes as u64);
    desired.max(start_offset)
}

impl Default for TerminalCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Something the parser saw complete in the stream that changes the terminal's mode state.
enum StreamEvent<'a> {
    /// A DECSET (`ESC[?<modes>h`) or DECRST (`ESC[?<modes>l`).
    PrivateMode(PrivateModeUpdate<'a>),
    /// RIS (`ESC c`) — a hard terminal reset, which turns every private mode back off.
    FullReset,
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

    /// Consume the next byte, reporting a mode-changing sequence when one completes.
    fn feed(&mut self, byte: u8) -> Option<StreamEvent<'_>> {
        match self.state {
            ParserState::Ground => {
                if byte == ESC {
                    self.state = ParserState::Escape;
                }
                None
            }
            ParserState::Escape => {
                if byte == RIS_FINAL {
                    self.state = ParserState::Ground;
                    return Some(StreamEvent::FullReset);
                }
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
    fn feed_csi(&mut self, byte: u8) -> Option<StreamEvent<'_>> {
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
                Some(StreamEvent::PrivateMode(PrivateModeUpdate {
                    modes: &self.params,
                    enabled,
                }))
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
/// Final byte of RIS (`ESC c`), the hard terminal reset.
const RIS_FINAL: u8 = b'c';
/// Intermediate bytes of an escape sequence, e.g. the `(` of `ESC ( B`.
const INTERMEDIATE_BYTES: std::ops::RangeInclusive<u8> = 0x20..=0x2f;
/// Bytes that separate CSI parameters (`;`, and `:` inside sub-parameters).
const PARAM_SEPARATORS: std::ops::RangeInclusive<u8> = 0x3a..=0x3b;
/// Bytes that terminate a CSI sequence.
const CSI_FINAL_BYTES: std::ops::RangeInclusive<u8> = 0x40..=0x7e;
