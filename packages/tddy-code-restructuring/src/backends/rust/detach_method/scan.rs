//! The byte-level reading `detach_method` needs over a file: identifiers, brackets, macro
//! invocations and the positions the server reports.
//!
//! Every reading takes the masked text (comments and literals blanked, byte for byte, see
//! [`masked_to_code`]), so an offset into it is an offset into the file.

use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::super::item_move::text::line_start;
use crate::edit::Position;

/// The masked text of a file: the code, with comments and literals blanked.
pub(super) fn masked(text: &str) -> String {
    masked_to_code(text)
}

/// Whether `byte` can be part of an identifier.
pub(super) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The first offset at or after `at` that is not whitespace.
pub(super) fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// The offset just past the last non-whitespace byte before `at`.
pub(super) fn skip_whitespace_back(bytes: &[u8], mut at: usize) -> usize {
    while at > 0 && bytes[at - 1].is_ascii_whitespace() {
        at -= 1;
    }
    at
}

/// The offset just past the identifier starting at `at`.
pub(super) fn word_end(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && is_word(bytes[at]) {
        at += 1;
    }
    at
}

/// The offset the identifier ending at `end` starts at.
pub(super) fn word_start(bytes: &[u8], end: usize) -> usize {
    let mut at = end;
    while at > 0 && is_word(bytes[at - 1]) {
        at -= 1;
    }
    at
}

/// The offset of the bracket closing the `(`, `[` or `{` at `open`, counting brackets of its own
/// kind only.
pub(super) fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let (opening, closing) = match bytes.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (at, byte) in bytes.iter().enumerate().skip(open) {
        if *byte == opening {
            depth += 1;
        } else if *byte == closing {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// Whether the offset lies inside the arguments of a macro invocation (`format!(…)`, `vec![…]`):
/// some enclosing bracket is opened right after a `name!`.
pub(super) fn inside_a_macro(masked: &str, offset: usize) -> bool {
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    for at in (0..offset.min(bytes.len())).rev() {
        match bytes[at] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
            b'(' | b'[' | b'{' => {
                let before = skip_whitespace_back(bytes, at);
                let bang = before > 0 && bytes[before - 1] == b'!';
                if bang && before >= 2 && is_word(bytes[before - 2]) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether the identifier at `at..end` of `masked` names a local or a function: not a field or
/// method (after `.`), not a path segment (beside `::`).
pub(super) fn is_a_plain_name(masked: &str, at: usize, end: usize) -> bool {
    let bytes = masked.as_bytes();
    let before = skip_whitespace_back(bytes, at);
    let after = skip_whitespace(bytes, end);
    let after_a_path = bytes[after..].starts_with(b"::");
    let before_a_path = before >= 2 && &bytes[before - 2..before] == b"::";
    let before_a_field = before > 0 && bytes[before - 1] == b'.';
    !(after_a_path || before_a_path || before_a_field)
}

/// The offsets of every whole identifier `word` in `masked[range]`.
pub(super) fn occurrences(masked: &str, range: Range<usize>, word: &str) -> Vec<usize> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    let mut at = range.start;
    while at < range.end.min(bytes.len()) {
        let starts_word = is_word(bytes[at]) && (at == 0 || !is_word(bytes[at - 1]));
        if !starts_word {
            at += 1;
            continue;
        }
        let end = word_end(bytes, at);
        if end <= range.end && &masked[at..end] == word {
            found.push(at);
        }
        at = end;
    }
    found
}

/// The one-based line `offset` of `text` is on.
pub(super) fn line_of(text: &str, offset: usize) -> u32 {
    text[..offset].matches('\n').count() as u32 + 1
}

/// The one-based position (line, and column in characters) of a byte offset of `text`.
pub(super) fn position_of(text: &str, offset: usize) -> Position {
    let line_begins = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    Position {
        line: line_of(text, offset),
        col: text[line_begins..offset].chars().count() as u32 + 1,
    }
}

/// The byte offset of a one-based position (line, and column in characters) of `text`.
pub(super) fn offset_of(text: &str, at: Position) -> usize {
    let begins = line_start(text, at.line);
    text[begins..]
        .char_indices()
        .nth(at.col.saturating_sub(1) as usize)
        .map_or(text.len(), |(index, _)| begins + index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_inside_a_macro_is_inside_one_and_a_call_in_a_block_is_not() {
        let text = "fn f(h: &Host) {\n    let n = h.total();\n    format!(\"{}\", h.total());\n}\n";
        let masked = masked(text);
        let plain = text.find("h.total").expect("the call is written");
        let in_macro = text.rfind("h.total").expect("the call is written");

        assert!(!inside_a_macro(&masked, plain));
        assert!(inside_a_macro(&masked, in_macro));
    }

    #[test]
    fn positions_count_lines_and_characters_one_based() {
        let text = "ab\nçd\n";
        let at = offset_of(text, Position { line: 2, col: 2 });

        assert_eq!(&text[at..at + 2], "d\n");
        assert_eq!(position_of(text, at), Position { line: 2, col: 2 });
    }
}
