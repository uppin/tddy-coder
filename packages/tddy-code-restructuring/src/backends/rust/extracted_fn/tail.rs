//! Rule 13: whether a range is the value of the function around it.
//!
//! A range that ends a match arm's body, an `if`/`else` branch or a plain block whose construct is
//! itself the tail of a block in tail position, up to the function body, keeps its `return`s: they
//! still return from the caller there. rust-analyzer writes the call as the arm's value.

use super::span::{keyword_at, matching_close, segment_start, skip_whitespace, word_end};
use crate::backends::rust::early_return::{
    byte_offset, masked_to_code, matching_open_brace, opens_a_function_body,
};
use crate::edit::Range;

/// Whether `range` ends at the end of a block's tail expression whose block — a match arm body, an
/// `if`/`else` branch or a plain block — is itself in tail position, up to the function body.
pub(super) fn in_tail_position(text: &str, range: Range) -> bool {
    let (Some(from), Some(to)) = (byte_offset(text, range.start), byte_offset(text, range.end))
    else {
        return false;
    };
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let ends_a_statement = code[..to].trim_ascii_end().last() == Some(&b';');
    let close = skip_whitespace(code, to);
    from < to
        && !ends_a_statement
        && balanced(&code[from..to])
        && code.get(close) == Some(&b'}')
        && ends_a_tail(code, close)
}

/// Whether every brace of `code` is closed within it, and none closes one it did not open.
fn balanced(code: &[u8]) -> bool {
    let mut depth = 0usize;
    for byte in code {
        match byte {
            b'{' => depth += 1,
            b'}' => match depth.checked_sub(1) {
                Some(rest) => depth = rest,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0
}

/// What a `{` opens, as far as tail position is concerned.
#[derive(PartialEq, Eq)]
enum Block {
    Function,
    /// A bare block statement.
    Plain,
    /// A match arm's body.
    Arm,
    If,
    Else,
    Other,
}

fn block_at(code: &[u8], open: usize) -> Block {
    if opens_a_function_body(code, open) {
        return Block::Function;
    }
    let Some(start) = segment_start(code, open) else {
        return Block::Other;
    };
    let header = String::from_utf8_lossy(&code[start..open]).into_owned();
    let header = header.trim();
    if header.is_empty() {
        return Block::Plain;
    }
    if header.ends_with("=>") {
        return Block::Arm;
    }
    let after_arm = header.rsplit("=>").next().unwrap_or(header).trim_start();
    match &after_arm[..word_end(after_arm.as_bytes(), 0)] {
        "if" => Block::If,
        "else" => Block::Else,
        _ => Block::Other,
    }
}

/// Whether the block closed by the `}` at `close` is, as a whole, the value of the function around
/// it: it is the function's body, or the construct it ends is the tail of a block that is.
fn ends_a_tail(code: &[u8], close: usize) -> bool {
    let Some(open) = matching_open_brace(code, close) else {
        return false;
    };
    match block_at(code, open) {
        Block::Function => true,
        Block::Plain => tail_after(code, close),
        Block::Arm => matching_open_brace(code, open)
            .filter(|construct| opens_a_match(code, *construct))
            .and_then(|construct| matching_close(code, construct))
            .is_some_and(|end| tail_after(code, end)),
        Block::If | Block::Else => {
            starts_an_if(code, open)
                && chain_end(code, close).is_some_and(|end| tail_after(code, end))
        }
        Block::Other => false,
    }
}

/// Whether what follows the construct that ends at `end` is the closing brace of a block that is
/// itself in tail position.
fn tail_after(code: &[u8], end: usize) -> bool {
    let next = skip_whitespace(code, end + 1);
    code.get(next) == Some(&b'}') && ends_a_tail(code, next)
}

/// Whether the `{` at `open` opens the block of a `match`.
fn opens_a_match(code: &[u8], open: usize) -> bool {
    let Some(start) = segment_start(code, open) else {
        return false;
    };
    let header = String::from_utf8_lossy(&code[start..open]).into_owned();
    let header = header.rsplit("=>").next().unwrap_or("").trim_start();
    &header[..word_end(header.as_bytes(), 0)] == "match"
}

/// Whether the branch whose `{` is at `open` belongs to an `if` that begins its statement, rather
/// than being a value bound or combined with something.
fn starts_an_if(code: &[u8], open: usize) -> bool {
    match block_at(code, open) {
        Block::If => true,
        Block::Else => {
            let before = segment_start(code, open).and_then(|start| start.checked_sub(1));
            before
                .filter(|at| code[*at] == b'}')
                .and_then(|at| matching_open_brace(code, at))
                .is_some_and(|previous| starts_an_if(code, previous))
        }
        _ => false,
    }
}

/// The `}` that ends the whole `if … else …` chain the block closed at `close` belongs to.
fn chain_end(code: &[u8], close: usize) -> Option<usize> {
    let mut end = close;
    loop {
        let next = skip_whitespace(code, end + 1);
        if !keyword_at(code, next, "else") {
            return Some(end);
        }
        let mut open = skip_whitespace(code, next + "else".len());
        if keyword_at(code, open, "if") {
            open = (open..code.len()).find(|at| code[*at] == b'{')?;
        }
        if code.get(open) != Some(&b'{') {
            return Some(end);
        }
        end = matching_close(code, open)?;
    }
}
