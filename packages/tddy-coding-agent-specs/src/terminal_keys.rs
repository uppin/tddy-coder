//! The terminal key-name table: the bytes pressing a named key types (`Enter`, `C-c`, `F1` …).
//!
//! One list, read by two parties: `SendSessionKeys` encodes the names it is given, and an agent
//! spec's `turn_input.submit_key` is validated against it at load (`#orchestrators` 4/11). Names
//! are matched exactly: case matters, and nothing outside the table is accepted.
//!
//! **Key names:** `Enter`, `Escape`, `Tab`, `BTab`, `Backspace`, `Delete`, `Up`, `Down`, `Left`,
//! `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Space`, `C-a` … `C-z`, `F1` … `F12`.

/// Whether `name` is a key name in the table.
pub fn is_key_name(name: &str) -> bool {
    named_key_bytes(name).is_some()
}

/// The bytes pressing the key `name` types, or `None` for a name outside the table.
pub fn named_key_bytes(name: &str) -> Option<Vec<u8>> {
    let fixed: &[u8] = match name {
        "Enter" => b"\r",
        "Escape" => b"\x1b",
        "Tab" => b"\t",
        "BTab" => b"\x1b[Z",
        "Backspace" => b"\x7f",
        "Delete" => b"\x1b[3~",
        "Space" => b" ",
        "Up" => b"\x1b[A",
        "Down" => b"\x1b[B",
        "Right" => b"\x1b[C",
        "Left" => b"\x1b[D",
        "Home" => b"\x1b[H",
        "End" => b"\x1b[F",
        "PageUp" => b"\x1b[5~",
        "PageDown" => b"\x1b[6~",
        "F1" => b"\x1bOP",
        "F2" => b"\x1bOQ",
        "F3" => b"\x1bOR",
        "F4" => b"\x1bOS",
        "F5" => b"\x1b[15~",
        "F6" => b"\x1b[17~",
        "F7" => b"\x1b[18~",
        "F8" => b"\x1b[19~",
        "F9" => b"\x1b[20~",
        "F10" => b"\x1b[21~",
        "F11" => b"\x1b[23~",
        "F12" => b"\x1b[24~",
        _ => return control_letter_bytes(name),
    };
    Some(fixed.to_vec())
}

/// The control byte of `C-a` through `C-z`: the letter's position in the alphabet, from 0x01.
fn control_letter_bytes(name: &str) -> Option<Vec<u8>> {
    let letter = name.strip_prefix("C-")?;
    match letter.as_bytes() {
        [letter @ b'a'..=b'z'] => Some(vec![letter - b'a' + 1]),
        _ => None,
    }
}
