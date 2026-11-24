//! Typing into a live terminal: the text, then — once the application has echoed it — the keys, as
//! a write of their own.
//!
//! Cursor Agent's TUI holds every byte that follows a paste's end marker (`ESC[201~`) in the same
//! read until the *next* input arrives, so `ESC[200~text ESC[201~ \r` written at once leaves the
//! text in its input box unsubmitted. A second write alone does not cure it: a transport between
//! the daemon and the application (tmux, `coder ssh`) merges two writes made back to back into one
//! read. Only a write made after the application has *read* the paste is a read of its own, and
//! the application's echo is what says it has.
//!
//! The application here is a fake TUI with exactly those two properties: it drains everything
//! written since its last read as one read, every [`READ_EVERY`], and it applies Cursor's rule.
//! Its echo moves the output offset as a real terminal's capture would.
//!
//! Evidence (2026-11-24, Cursor Agent 2026.11.19/2026.11.22, bundled `5548.index.js`): the paste
//! handler stores the remainder after `ESC[201~` (`xt.current = n`) and prepends it to the next
//! input event; the same message is submitted over a raw pty, local tmux and `coder ssh` to tmux
//! only when Enter is a read of its own.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_tool_engine::terminal_keys::{
    encode_keys, encode_typing, type_into, EchoWait, Typed, TypingError, TypingTerminal,
};

/// How often the fake TUI reads its input: everything written in between is one read.
const READ_EVERY: Duration = Duration::from_millis(20);
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const MESSAGE: &str = "rebase onto the new base and re-run the tests";

fn enter() -> Vec<String> {
    vec!["Enter".to_string()]
}

// ── the fake TUI ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct TuiState {
    /// Written, not yet read.
    unread: Vec<u8>,
    /// Bytes that followed a paste end in one read, held until the next read (Cursor's rule).
    held: Vec<u8>,
    input_box: String,
    submitted: Vec<String>,
    /// The output sequence point: moved by every echo.
    output_offset: u64,
    /// Every write, as it was made.
    writes: Vec<Vec<u8>>,
}

/// A TUI that reads its input in batches and holds what trails a paste end.
#[derive(Clone)]
struct DeferringTui {
    state: Arc<Mutex<TuiState>>,
    echoes: bool,
}

impl DeferringTui {
    /// A TUI that echoes what it reads, reading every [`READ_EVERY`].
    fn echoing() -> Self {
        Self::start(true)
    }

    /// A TUI that reads but never draws (its screen does not move).
    fn silent() -> Self {
        Self::start(false)
    }

    /// An echoing TUI whose screen also never holds still: a spinner redraws it every few ms.
    fn echoing_with_a_spinner() -> Self {
        let tui = Self::start(true);
        let spinner = tui.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(5)).await;
                spinner.state.lock().unwrap().output_offset += 1;
            }
        });
        tui
    }

    fn start(echoes: bool) -> Self {
        let tui = Self {
            state: Arc::new(Mutex::new(TuiState::default())),
            echoes,
        };
        let reader = tui.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(READ_EVERY).await;
                reader.read();
            }
        });
        tui
    }

    /// One read: everything written since the last one.
    fn read(&self) {
        let mut state = self.state.lock().unwrap();
        if state.unread.is_empty() {
            return;
        }
        let chunk = std::mem::take(&mut state.unread);
        let echoed = chunk.len() as u64;
        match find(&chunk, PASTE_START) {
            Some(start) => {
                let body = &chunk[start + PASTE_START.len()..];
                let end = find(body, PASTE_END).expect("the fake reads whole pastes");
                state
                    .input_box
                    .push_str(&String::from_utf8_lossy(&body[..end]));
                // Cursor's rule: the rest of this read waits for the next input.
                state.held = body[end + PASTE_END.len()..].to_vec();
            }
            None => {
                let mut typed = std::mem::take(&mut state.held);
                typed.extend_from_slice(&chunk);
                for byte in typed {
                    if byte == b'\r' {
                        if !state.input_box.is_empty() {
                            let line = std::mem::take(&mut state.input_box);
                            state.submitted.push(line);
                        }
                    } else {
                        state.input_box.push(byte as char);
                    }
                }
            }
        }
        if self.echoes {
            state.output_offset += echoed;
        }
    }

    fn submitted(&self) -> Vec<String> {
        self.state.lock().unwrap().submitted.clone()
    }

    fn input_box(&self) -> String {
        self.state.lock().unwrap().input_box.clone()
    }

    fn writes(&self) -> Vec<Vec<u8>> {
        self.state.lock().unwrap().writes.clone()
    }

    /// Long enough for every pending read to have happened.
    async fn settle(&self) {
        tokio::time::sleep(READ_EVERY * 5).await;
    }
}

#[async_trait]
impl TypingTerminal for DeferringTui {
    async fn output_offset(&self) -> Result<u64, String> {
        Ok(self.state.lock().unwrap().output_offset)
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.unread.extend_from_slice(&bytes);
        state.writes.push(bytes);
        Ok(())
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn pasted(text: &str) -> Vec<u8> {
    [PASTE_START, text.as_bytes(), PASTE_END].concat()
}

/// An echo wait short enough for a test that waits it out.
fn a_short_echo_wait() -> EchoWait {
    EchoWait {
        no_echo_timeout: Duration::from_millis(300),
        ..EchoWait::default()
    }
}

// ── the fake reproduces what was seen ────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn a_paste_and_enter_written_at_once_stay_in_the_input_box() {
    // Given a TUI that holds what trails a paste's end
    let tui = DeferringTui::echoing();

    // When the message and Enter are written as one buffer, as delivery did before
    let bytes = encode_keys(Some(MESSAGE), &enter(), true).unwrap();
    tui.write(bytes).await.unwrap();
    tui.settle().await;

    // Then the message sits in the input box, unsubmitted
    assert_eq!(tui.submitted(), Vec::<String>::new());
    assert_eq!(tui.input_box(), MESSAGE);
}

#[tokio::test(start_paused = true)]
async fn two_writes_made_back_to_back_are_one_read() {
    // Given a TUI behind a transport that merges writes (it reads in batches)
    let tui = DeferringTui::echoing();

    // When the paste and Enter are two writes with nothing waited on between them
    tui.write(pasted(MESSAGE)).await.unwrap();
    tui.write(b"\r".to_vec()).await.unwrap();
    tui.settle().await;

    // Then they arrive as one read, and the message is still not submitted
    assert_eq!(tui.submitted(), Vec::<String>::new());
}

// ── typing waits for the echo ────────────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn typed_text_is_submitted_once_the_application_has_echoed_it() {
    // Given a TUI that holds what trails a paste's end, and echoes what it reads
    let tui = DeferringTui::echoing();

    // When the message and Enter are typed
    let typing = encode_typing(Some(MESSAGE), &enter(), true).unwrap();
    let typed = type_into(&tui, typing, &EchoWait::default()).await.unwrap();
    tui.settle().await;

    // Then the message is submitted, the paste and Enter having been written apart, and the echo
    // was seen
    assert_eq!(tui.submitted(), vec![MESSAGE.to_string()]);
    assert_eq!(tui.writes(), vec![pasted(MESSAGE), b"\r".to_vec()]);
    assert_eq!(
        typed,
        Typed {
            sent_bytes: pasted(MESSAGE).len() + 1,
            echo_seen: Some(true),
        }
    );
}

#[tokio::test(start_paused = true)]
async fn the_keys_are_pressed_after_the_timeout_when_no_echo_comes() {
    // Given a TUI that reads its input but never draws
    let tui = DeferringTui::silent();

    // When the message and Enter are typed
    let typing = encode_typing(Some(MESSAGE), &enter(), true).unwrap();
    let typed = type_into(&tui, typing, &a_short_echo_wait()).await.unwrap();
    tui.settle().await;

    // Then Enter is still pressed, on its own, once the wait for an echo runs out — and the
    // answer says no echo was seen
    assert_eq!(tui.writes(), vec![pasted(MESSAGE), b"\r".to_vec()]);
    assert_eq!(tui.submitted(), vec![MESSAGE.to_string()]);
    assert_eq!(typed.echo_seen, Some(false));
}

#[tokio::test(start_paused = true)]
async fn keys_alone_are_pressed_without_waiting() {
    // Given a TUI that never draws
    let tui = DeferringTui::silent();

    // When only keys are typed
    let typing = encode_typing(None, &["C-c".to_string()], true).unwrap();
    let started = tokio::time::Instant::now();
    let typed = type_into(&tui, typing, &a_short_echo_wait()).await.unwrap();

    // Then they are one write, made at once, and nothing was waited on
    assert_eq!(tui.writes(), vec![vec![0x03]]);
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_eq!(
        typed,
        Typed {
            sent_bytes: 1,
            echo_seen: None
        }
    );
}

#[tokio::test(start_paused = true)]
async fn text_alone_is_one_write_with_nothing_waited_on() {
    // Given a TUI that never draws
    let tui = DeferringTui::silent();

    // When only text is typed
    let typing = encode_typing(Some(MESSAGE), &[], true).unwrap();
    let typed = type_into(&tui, typing, &a_short_echo_wait()).await.unwrap();

    // Then it is one paste, and there was nothing after it to wait for an echo before
    assert_eq!(tui.writes(), vec![pasted(MESSAGE)]);
    assert_eq!(typed.echo_seen, None);
}

#[test]
fn a_typing_keeps_the_text_and_the_keys_apart() {
    // When a text and keys are encoded for typing
    let typing = encode_typing(Some("ls"), &enter(), true).unwrap();

    // Then the text and the keys are the two writes, which together are what encode_keys types
    assert_eq!(typing.text, pasted("ls"));
    assert_eq!(typing.keys, b"\r".to_vec());
    assert_eq!(
        [typing.text, typing.keys].concat(),
        encode_keys(Some("ls"), &enter(), true).unwrap()
    );
}

#[tokio::test(start_paused = true)]
async fn the_keys_are_pressed_at_the_settle_cap_when_the_screen_never_holds_still() {
    // Given an echoing TUI whose spinner keeps its screen moving
    let tui = DeferringTui::echoing_with_a_spinner();
    let wait = EchoWait::default();

    // When the message and Enter are typed
    let typing = encode_typing(Some(MESSAGE), &enter(), true).unwrap();
    let started = tokio::time::Instant::now();
    let typed = type_into(&tui, typing, &wait).await.unwrap();
    tui.settle().await;

    // Then Enter is pressed once the settle cap has passed, not after the no-echo timeout, and the
    // message is submitted
    assert!(
        started.elapsed() < wait.settle_cap + READ_EVERY * 10,
        "Enter came {:?} after the paste",
        started.elapsed()
    );
    assert_eq!(typed.echo_seen, Some(true));
    assert_eq!(tui.submitted(), vec![MESSAGE.to_string()]);
}

/// A terminal that echoes every write at once, and refuses the write numbered `refuse` (0-based).
struct RefusingTerminal {
    refuse: usize,
    writes: Mutex<usize>,
    offset: Mutex<u64>,
}

impl RefusingTerminal {
    fn refusing_write(refuse: usize) -> Self {
        Self {
            refuse,
            writes: Mutex::new(0),
            offset: Mutex::new(0),
        }
    }
}

#[async_trait]
impl TypingTerminal for RefusingTerminal {
    async fn output_offset(&self) -> Result<u64, String> {
        Ok(*self.offset.lock().unwrap())
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        let mut writes = self.writes.lock().unwrap();
        let this = *writes;
        *writes += 1;
        if this == self.refuse {
            return Err("the session has exited".to_string());
        }
        *self.offset.lock().unwrap() += bytes.len() as u64;
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn a_refused_text_write_sent_nothing() {
    // Given a terminal that refuses the first write
    let terminal = RefusingTerminal::refusing_write(0);

    // When the message and Enter are typed
    let typing = encode_typing(Some(MESSAGE), &enter(), true).unwrap();
    let refused = type_into(&terminal, typing, &EchoWait::default())
        .await
        .unwrap_err();

    // Then the refusal says nothing was sent
    assert_eq!(
        refused,
        TypingError::NothingSent("the session has exited".to_string())
    );
    assert_eq!(
        refused.to_string(),
        "the session has exited; nothing was sent"
    );
}

#[tokio::test(start_paused = true)]
async fn a_refused_keys_write_says_the_text_was_typed() {
    // Given a terminal that takes the text but refuses the next write
    let terminal = RefusingTerminal::refusing_write(1);

    // When the message and Enter are typed
    let typing = encode_typing(Some(MESSAGE), &enter(), true).unwrap();
    let refused = type_into(&terminal, typing, &EchoWait::default())
        .await
        .unwrap_err();

    // Then the refusal says the text was typed and the keys were not
    assert_eq!(
        refused.to_string(),
        "the session has exited; the text was typed but the keys were not"
    );
}

#[test]
fn raw_text_keeps_its_line_breaks_as_enter_in_the_text_write() {
    // When multi-line text is encoded for a terminal that takes no paste, with Enter after it
    let typing = encode_typing(Some("one\ntwo"), &enter(), false).unwrap();

    // Then the text write types each line break as Enter, and the keys are the trailing Enter alone
    assert_eq!(typing.text, b"one\rtwo".to_vec());
    assert_eq!(typing.keys, b"\r".to_vec());
}
