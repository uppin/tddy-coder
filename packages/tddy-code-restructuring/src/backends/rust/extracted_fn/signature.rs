//! Reading a function's signature over masked code: its parameters and its return type, as byte
//! ranges of the text they were read from, so a rule can rewrite one without touching the rest.

use super::span::{is_word, keyword_at, matching_parenthesis, skip_whitespace, word_end, FnSpan};
use std::ops::Range;

/// One parameter of a function.
pub(super) struct Parameter {
    /// The bound name (`dir` of `dir: &PathBuf`); `self` for a receiver.
    pub name: String,
    /// The bytes of its type. Empty for a receiver written without one (`&self`).
    pub ty: Range<usize>,
}

/// The parameters and the return type of a function.
pub(super) struct Signature {
    pub parameters: Vec<Parameter>,
    /// The bytes of the return type, without the `->`, or `None` for a function returning `()`.
    pub returns: Option<Range<usize>>,
}

/// The signature of the function at `span` in masked `code`, or `None` when it does not read as one.
pub(super) fn signature(code: &str, span: FnSpan) -> Option<Signature> {
    let bytes = code.as_bytes();
    let name_end = word_end(bytes, skip_whitespace(bytes, span.keyword + "fn".len()));
    let open = (past_generics(bytes, name_end)..span.open).find(|at| bytes[*at] == b'(')?;
    let close = matching_parenthesis(bytes, open)?;
    let parameters = split_top_level(bytes, open + 1..close)
        .into_iter()
        .filter(|piece| !code[piece.clone()].trim().is_empty())
        .map(|piece| parameter(code, piece))
        .collect();

    let after = skip_whitespace(bytes, close + 1);
    let returns = bytes[after..span.open].starts_with(b"->").then(|| {
        let from = skip_whitespace(bytes, after + 2);
        let end = (from..span.open)
            .find(|at| keyword_at(bytes, *at, "where"))
            .unwrap_or(span.open);
        from..from + code[from..end].trim_end().len()
    });
    Some(Signature {
        parameters,
        returns,
    })
}

fn parameter(code: &str, piece: Range<usize>) -> Parameter {
    let bytes = code.as_bytes();
    let colon = (piece.start..piece.end).find(|at| {
        bytes[*at] == b':'
            && bytes.get(at + 1) != Some(&b':')
            && (*at == 0 || bytes[at - 1] != b':')
    });
    let Some(colon) = colon else {
        return Parameter {
            name: "self".to_string(),
            ty: piece.end..piece.end,
        };
    };
    let name = code[piece.start..colon]
        .trim()
        .trim_start_matches("mut ")
        .trim()
        .to_string();
    let from = skip_whitespace(bytes, colon + 1);
    Parameter {
        name,
        ty: from..from + code[from..piece.end].trim_end().len(),
    }
}

/// Where the generic parameter list starting at or after `from` ends; `from` when there is none.
fn past_generics(code: &[u8], from: usize) -> usize {
    let start = skip_whitespace(code, from);
    if code.get(start) != Some(&b'<') {
        return from;
    }
    let mut depth = 0usize;
    for (at, byte) in code.iter().enumerate().skip(start) {
        match byte {
            b'<' => depth += 1,
            b'>' if code[at - 1] != b'-' => {
                depth -= 1;
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
    }
    from
}

/// The ranges between the commas at depth zero of `within`.
fn split_top_level(code: &[u8], within: Range<usize>) -> Vec<Range<usize>> {
    let (mut pieces, mut depth, mut from) = (Vec::new(), 0isize, within.start);
    for at in within.clone() {
        match code[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'<' => depth += 1,
            b'>' if at > 0 && code[at - 1] != b'-' => depth -= 1,
            b',' if depth == 0 => {
                pieces.push(from..at);
                from = at + 1;
            }
            _ => {}
        }
    }
    pieces.push(from..within.end);
    pieces
}

/// The return type a function header — `pub fn f(..) -> T {`, up to and including the `{` — declares.
pub(super) fn header_return_type(header: &str) -> Option<String> {
    let masked = crate::backends::rust::early_return::masked_to_code(header);
    let keyword = (0..masked.len()).find(|at| keyword_at(masked.as_bytes(), *at, "fn"))?;
    let open = masked.rfind('{')?;
    let returns = signature(
        &masked,
        FnSpan {
            keyword,
            open,
            close: open,
        },
    )?
    .returns?;
    Some(header[returns].to_string())
}

/// A generic type read apart: `Result<(u32, u32), String>` is the path `Result` and the arguments
/// `(u32, u32)` and `String`.
pub(super) struct Generic<'a> {
    pub path: &'a str,
    pub arguments: Vec<&'a str>,
}

/// `ty` as a path with generic arguments, or `None` for a type that has none.
pub(super) fn generic(ty: &str) -> Option<Generic<'_>> {
    let ty = ty.trim();
    let open = ty.find('<')?;
    let path = ty[..open].trim();
    let well_formed = ty.ends_with('>') && !path.is_empty() && path.bytes().all(is_path_byte);
    well_formed.then(|| Generic {
        path,
        arguments: split_top_level(ty.as_bytes(), open + 1..ty.len() - 1)
            .into_iter()
            .map(|argument| ty[argument].trim())
            .filter(|argument| !argument.is_empty())
            .collect(),
    })
}

fn is_path_byte(byte: u8) -> bool {
    is_word(byte) || byte == b':'
}
