//! Where the function an extraction introduced sits in the file, and the byte scanning every rule
//! over it shares. Everything is read over text with comments and literals masked, so a `fn` or a
//! brace written inside one is not seen.

use crate::backends::rust::early_return::{
    masked_to_code, matching_open_brace, opens_a_function_body,
};

/// Byte offsets of a function's `fn` keyword, the `{` that opens its body and the `}` that closes
/// it, read over text with comments and literals masked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FnSpan {
    pub keyword: usize,
    pub open: usize,
    pub close: usize,
}

/// The span of the one `fn <name>` declared in `text`, or `None` when there is none.
pub(super) fn function_span(text: &str, name: &str) -> Option<FnSpan> {
    let code = masked_to_code(text);
    let bytes = code.as_bytes();
    let mut search = 0;
    while let Some(found) = code[search..].find("fn") {
        let keyword = search + found;
        search = keyword + "fn".len();
        let named_at = skip_whitespace(bytes, search);
        let whole_keyword = (keyword == 0 || !is_word(bytes[keyword - 1])) && named_at > search;
        if !whole_keyword || &code[named_at..word_end(bytes, named_at)] != name {
            continue;
        }
        let open = body_open(bytes, word_end(bytes, named_at))?;
        return Some(FnSpan {
            keyword,
            open,
            close: matching_close(bytes, open)?,
        });
    }
    None
}

/// The function whose body holds byte `at` of `code`: the innermost enclosing body that is a
/// function's, so a closure's or an `if`'s is looked through.
pub(super) fn enclosing_function(code: &[u8], at: usize) -> Option<FnSpan> {
    let mut cursor = at;
    loop {
        let open = matching_open_brace(code, cursor)?;
        if opens_a_function_body(code, open) {
            let start = segment_start(code, open)?;
            let keyword = (start..open).find(|at| keyword_at(code, *at, "fn"))?;
            return Some(FnSpan {
                keyword,
                open,
                close: matching_close(code, open)?,
            });
        }
        cursor = open;
    }
}

/// Where the statement or item whose `{` is at `open` begins: just past the previous `;`, `{` or
/// `}` outside any parenthesis. `None` when the brace sits inside an unclosed one, as an argument.
pub(super) fn segment_start(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for at in (0..open).rev() {
        match code[at] {
            b')' | b']' => depth += 1,
            b'(' | b'[' if depth == 0 => return None,
            b'(' | b'[' => depth -= 1,
            b';' | b'{' | b'}' if depth == 0 => return Some(at + 1),
            _ => {}
        }
    }
    Some(0)
}

/// Whether the whole word `word` starts at `at`, and is followed by something an identifier can
/// follow it with: whitespace and then a letter for `fn`, any non-identifier byte otherwise.
pub(super) fn keyword_at(code: &[u8], at: usize, word: &str) -> bool {
    let end = at + word.len();
    code.get(at..end) == Some(word.as_bytes())
        && (at == 0 || !is_word(code[at - 1]))
        && code.get(end).is_none_or(|byte| !is_word(*byte))
        && (word != "fn" || next_is_identifier(code, end))
}

/// Whether the next token after `at` is an identifier — `fn name`, not the pointer type `fn(`.
fn next_is_identifier(code: &[u8], at: usize) -> bool {
    code.get(skip_whitespace(code, at))
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
}

/// The `{` opening the body of the function whose signature runs from `from`, or `None` for a
/// declaration with no body.
fn body_open(code: &[u8], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(from) {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'{' if depth == 0 => return Some(at),
            b';' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

/// The `}` that closes the `{` at `open`.
pub(super) fn matching_close(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `)` that closes the `(` at `open`.
pub(super) fn matching_parenthesis(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// Where the identifier-like run starting at `from` ends.
pub(super) fn word_end(code: &[u8], from: usize) -> usize {
    code[from..]
        .iter()
        .position(|byte| !is_word(*byte))
        .map_or(code.len(), |length| from + length)
}

/// The first offset at or after `from` that is not whitespace.
pub(super) fn skip_whitespace(code: &[u8], from: usize) -> usize {
    code[from..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map_or(code.len(), |length| from + length)
}

/// A top-level statement of a region of code: from its first character to its terminator.
pub(super) struct Statement {
    pub start: usize,
    pub end: usize,
    /// Whether it ends in `;` or a closing brace, rather than being an expression left as a tail.
    pub terminated: bool,
}

/// The top-level statements of masked `code`, whose first byte is at `base` in the file. A closing
/// brace ends one when nothing continues it: a `;`, a method call, an `else` do.
pub(super) fn statements(code: &[u8], base: usize) -> Vec<Statement> {
    let mut found = Vec::new();
    let (mut depth, mut start) = (0usize, None);
    for (at, byte) in code.iter().enumerate() {
        if start.is_none() && !byte.is_ascii_whitespace() {
            start = Some(at);
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'}' | b';' => {
                depth = if *byte == b'}' {
                    depth.saturating_sub(1)
                } else {
                    depth
                };
                if depth == 0 && start.is_some() && (*byte == b';' || ends_a_block(code, at + 1)) {
                    found.push(Statement {
                        start: base + start.take().unwrap_or(at),
                        end: base + at + 1,
                        terminated: true,
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(start) = start {
        let end = code.iter().rposition(|byte| !byte.is_ascii_whitespace());
        found.push(Statement {
            start: base + start,
            end: base + end.map_or(start, |at| at + 1),
            terminated: false,
        });
    }
    found
}

/// Whether the code after a closing brace starts something else, rather than continuing the
/// statement the brace closed.
fn ends_a_block(code: &[u8], after: usize) -> bool {
    let next = skip_whitespace(code, after);
    match code.get(next) {
        None => true,
        Some(b';' | b'.' | b'?' | b',' | b')') => false,
        Some(_) => &code[next..word_end(code, next)] != b"else",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_FUNCTIONS: &str = "fn caller() -> u32 {\n    kept(1)\n}\n\n\
                                 fn kept(x: u32) -> u32 {\n    // `fn kept` in a comment\n    x + 1\n}\n";

    #[test]
    fn spans_the_named_function_from_its_keyword_to_its_closing_brace() {
        // Given a file declaring `kept`, which a comment and a call also mention
        // When the function's span is found
        let span = function_span(TWO_FUNCTIONS, "kept").expect("`kept` is declared");

        // Then it runs from `fn` to the closing brace, and its `open` is the body's `{`
        assert_eq!(
            &TWO_FUNCTIONS[span.keyword..=span.close],
            "fn kept(x: u32) -> u32 {\n    // `fn kept` in a comment\n    x + 1\n}"
        );
        assert_eq!(&TWO_FUNCTIONS[span.open..span.open + 1], "{");
    }

    #[test]
    fn a_name_only_a_comment_or_a_call_mentions_has_no_span() {
        // Given a name nothing declares, and a name only a comment and a call mention
        let commented_and_called = "fn a() {\n    // fn b() {}\n    b();\n}\n";

        // When their spans are looked for
        let undeclared = function_span(TWO_FUNCTIONS, "missing");
        let mentioned = function_span(commented_and_called, "b");

        // Then neither has one
        assert_eq!(undeclared, None);
        assert_eq!(mentioned, None);
    }
}
