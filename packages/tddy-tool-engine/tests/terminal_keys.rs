//! What `SendSessionKeys` types: the bytes of text, then of each named key, in order
//! (`#home-inbox` 5/8).
//!
//! The encoding is a pure table, so every rule is read straight off `encode_keys`.

use pretty_assertions::assert_eq;
use tddy_tool_engine::terminal_keys::encode_keys;

const ESC: u8 = 0x1b;

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// The bytes pressing the single key `name` types.
fn pressing(name: &str) -> Vec<u8> {
    encode_keys(None, &keys(&[name]), false).expect("a known key encodes")
}

/// The bytes typing `text` produces, with bracketed paste `on` or off.
fn typing(text: &str, bracketed_paste: bool) -> Vec<u8> {
    encode_keys(Some(text), &[], bracketed_paste).expect("text encodes")
}

fn refusal_of(text: Option<&str>, names: &[&str]) -> String {
    encode_keys(text, &keys(names), false).expect_err("the request is refused")
}

// ── the named keys ───────────────────────────────────────────────────────────────────────────

#[test]
fn editing_keys_type_their_control_bytes() {
    // Given the editing keys
    // When each is pressed
    // Then each types the byte a terminal sends for it
    assert_eq!(pressing("Enter"), b"\r");
    assert_eq!(pressing("Escape"), vec![ESC]);
    assert_eq!(pressing("Tab"), b"\t");
    assert_eq!(pressing("BTab"), b"\x1b[Z");
    assert_eq!(pressing("Backspace"), vec![0x7f]);
    assert_eq!(pressing("Delete"), b"\x1b[3~");
    assert_eq!(pressing("Space"), b" ");
}

#[test]
fn navigation_keys_type_their_escape_sequences() {
    assert_eq!(pressing("Up"), b"\x1b[A");
    assert_eq!(pressing("Down"), b"\x1b[B");
    assert_eq!(pressing("Right"), b"\x1b[C");
    assert_eq!(pressing("Left"), b"\x1b[D");
    assert_eq!(pressing("Home"), b"\x1b[H");
    assert_eq!(pressing("End"), b"\x1b[F");
    assert_eq!(pressing("PageUp"), b"\x1b[5~");
    assert_eq!(pressing("PageDown"), b"\x1b[6~");
}

#[test]
fn control_letters_type_their_control_bytes() {
    // Given C-a through C-z
    // Then C-a is 0x01, C-c interrupts with 0x03, and C-z is 0x1a
    assert_eq!(pressing("C-a"), vec![0x01]);
    assert_eq!(pressing("C-c"), vec![0x03]);
    assert_eq!(pressing("C-d"), vec![0x04]);
    assert_eq!(pressing("C-l"), vec![0x0c]);
    assert_eq!(pressing("C-z"), vec![0x1a]);
}

#[test]
fn every_control_letter_is_one_byte_after_the_one_before() {
    // Given all twenty-six control letters
    let names: Vec<String> = ('a'..='z').map(|letter| format!("C-{letter}")).collect();

    // When they are pressed in one request
    let typed = encode_keys(None, &names, false).expect("every control letter encodes");

    // Then they type 0x01 through 0x1a in order
    assert_eq!(typed, (1u8..=26).collect::<Vec<u8>>());
}

#[test]
fn function_keys_type_their_escape_sequences() {
    assert_eq!(pressing("F1"), b"\x1bOP");
    assert_eq!(pressing("F2"), b"\x1bOQ");
    assert_eq!(pressing("F3"), b"\x1bOR");
    assert_eq!(pressing("F4"), b"\x1bOS");
    assert_eq!(pressing("F5"), b"\x1b[15~");
    assert_eq!(pressing("F6"), b"\x1b[17~");
    assert_eq!(pressing("F7"), b"\x1b[18~");
    assert_eq!(pressing("F8"), b"\x1b[19~");
    assert_eq!(pressing("F9"), b"\x1b[20~");
    assert_eq!(pressing("F10"), b"\x1b[21~");
    assert_eq!(pressing("F11"), b"\x1b[23~");
    assert_eq!(pressing("F12"), b"\x1b[24~");
}

#[test]
fn keys_are_typed_in_the_order_they_are_named() {
    // Given three keys named out of any natural order
    // When they are pressed in one request
    let typed = encode_keys(None, &keys(&["Down", "Enter", "C-c"]), false).unwrap();

    // Then their bytes follow in that order
    assert_eq!(typed, b"\x1b[B\r\x03");
}

// ── text ─────────────────────────────────────────────────────────────────────────────────────

#[test]
fn text_is_typed_as_it_is_when_the_terminal_has_not_enabled_bracketed_paste() {
    assert_eq!(typing("echo hello", false), b"echo hello");
}

#[test]
fn text_is_wrapped_as_one_paste_when_the_terminal_has_enabled_it() {
    assert_eq!(typing("echo hello", true), b"\x1b[200~echo hello\x1b[201~");
}

#[test]
fn a_newline_in_raw_text_is_typed_as_enter() {
    // Given two lines of text and a terminal that takes no paste
    // Then the line break is the Enter a person would press
    assert_eq!(typing("one\ntwo", false), b"one\rtwo");
    assert_eq!(typing("one\r\ntwo", false), b"one\rtwo");
}

#[test]
fn a_newline_in_a_paste_is_pasted_verbatim() {
    // Given two lines of text and a terminal that has enabled bracketed paste
    // Then the paste carries the text untouched: the terminal's app decides what a pasted newline is
    assert_eq!(
        typing("one\ntwo", true),
        b"\x1b[200~one\ntwo\x1b[201~".to_vec()
    );
}

#[test]
fn text_comes_before_the_keys() {
    // Given text and Enter
    // When they are typed into a terminal with bracketed paste on
    let typed = encode_keys(Some("ls"), &keys(&["Enter"]), true).unwrap();

    // Then the paste is first and Enter follows it, outside the paste
    assert_eq!(typed, b"\x1b[200~ls\x1b[201~\r");
}

// ── what is refused ──────────────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_key_name_is_refused_naming_it() {
    assert!(refusal_of(None, &["Hyper"]).contains("Hyper"));
}

#[test]
fn a_key_name_is_matched_exactly() {
    // Given names that are close to real ones
    // Then each is unknown: case matters, and the ranges end where the table ends
    for near_miss in [
        "enter", "ENTER", "C-A", "C-1", "C-", "F0", "F13", "Ctrl-c", "Page Up",
    ] {
        assert!(
            refusal_of(None, &[near_miss]).contains(near_miss),
            "{near_miss:?} should be refused, naming it"
        );
    }
}

#[test]
fn one_unknown_key_among_known_ones_produces_no_bytes() {
    // Given text, a known key, an unknown key and another known key
    // When they are encoded
    let outcome = encode_keys(Some("hello"), &keys(&["Enter", "Hyper", "C-c"]), true);

    // Then the whole request is refused, naming the unknown key, and no bytes are produced
    let refusal = outcome.expect_err("one unknown name refuses the request");
    assert!(refusal.contains("Hyper"), "{refusal}");
}

#[test]
fn a_request_with_nothing_to_type_is_refused() {
    // Given no text and no keys, or text that is empty
    // Then it is refused: a send that types nothing is a mistake the agent should hear about
    assert!(refusal_of(None, &[]).contains("nothing to send"));
    assert!(refusal_of(Some(""), &[]).contains("nothing to send"));
}

#[test]
fn keys_alone_need_no_text_and_text_alone_needs_no_keys() {
    assert_eq!(encode_keys(None, &keys(&["Enter"]), true).unwrap(), b"\r");
    assert_eq!(encode_keys(Some("y"), &[], false).unwrap(), b"y");
}
