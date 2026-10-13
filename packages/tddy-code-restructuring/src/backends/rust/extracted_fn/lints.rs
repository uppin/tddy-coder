//! Rules 2-4: the lexical clean-ups of an extracted function, each safe by construction.

use super::signature::{generic, signature};
use super::span::{function_span, is_word, matching_close, statements, word_end};
use crate::backends::rust::early_return::masked_to_code;

/// The most parameters clippy accepts before `too_many_arguments` fires.
const MOST_PARAMETERS: usize = 7;

/// `text` with the tail of `name` taken out of `Ok(…)` / `Some(…)` when the function returns
/// `Result<(), …>` / `Option<()>` and the tail wraps an `if`, `match`, loop or block: the
/// expression becomes a statement, followed by `Ok(())` / `Some(())` (`clippy::unit_arg`).
pub(super) fn unit_tail_unwrapped(text: &str, name: &str) -> String {
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let Some(span) = function_span(text, name) else {
        return text.to_string();
    };
    let returns_unit = signature(&masked, span)
        .and_then(|signature| signature.returns)
        .and_then(|returns| generic(&masked[returns]).map(|ty| ty.arguments.first() == Some(&"()")))
        .unwrap_or(false);
    let body = statements(&code[span.open + 1..span.close], span.open + 1);
    let Some(tail) = body.last().filter(|tail| !tail.terminated && returns_unit) else {
        return text.to_string();
    };
    let Some(wrapper) = ["Ok", "Some"]
        .into_iter()
        .find(|wrapper| masked[tail.start..tail.end].starts_with(&format!("{wrapper}(")))
    else {
        return text.to_string();
    };
    let inner_from = tail.start + wrapper.len() + 1;
    let wraps_one_expression = matching_parenthesis(code, inner_from - 1) == Some(tail.end - 1);
    let inner = text[inner_from..tail.end - 1].trim();
    if !wraps_one_expression || !is_a_unit_statement(&masked[inner_from..tail.end - 1]) {
        return text.to_string();
    }

    let indent = &text[line_start(text, tail.start)..tail.start];
    format!(
        "{}{inner}\n{indent}{wrapper}(()){}",
        &text[..tail.start],
        &text[tail.end..]
    )
}

/// Whether masked `expression` begins with something that is a statement as it stands: a block-like
/// expression whose value is `()` here.
fn is_a_unit_statement(expression: &str) -> bool {
    let expression = expression.trim_start();
    let word = &expression[..word_end(expression.as_bytes(), 0)];
    let block_like = ["if", "match", "loop", "while", "for"].contains(&word)
        || (word == "unsafe" && expression[word.len()..].trim_start().starts_with('{'));
    block_like || expression.starts_with('{')
}

/// The `)` closing the `(` at `open`.
fn matching_parenthesis(code: &[u8], open: usize) -> Option<usize> {
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

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

/// `text` with every `field: field` in a struct literal inside `name` written `field`
/// (`clippy::redundant_field_names`).
pub(super) fn field_shorthand(text: &str, name: &str) -> String {
    let Some(span) = function_span(text, name) else {
        return text.to_string();
    };
    let masked = masked_to_code(text);
    let mut redundant: Vec<(usize, usize)> = Vec::new();
    for open in span.open + 1..span.close {
        if masked.as_bytes()[open] == b'{' && opens_a_struct_literal(masked.as_bytes(), open) {
            let close = matching_close(masked.as_bytes(), open).unwrap_or(open);
            redundant.extend(redundant_fields(&masked, open + 1..close));
        }
    }
    let mut written = text.to_string();
    for (from, to) in redundant.into_iter().rev() {
        let field = written[from..to].split(':').next().unwrap_or("").trim().to_string();
        written.replace_range(from..to, &field);
    }
    written
}

/// Whether the `{` at `open` follows the name of a struct: a path ending in a word that has a
/// lower-case letter and a capital, as `Lookup` and `Self` do and a constant (`MAX {`) or a
/// keyword does not.
fn opens_a_struct_literal(code: &[u8], open: usize) -> bool {
    let before = &code[..open];
    let end = before
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(0, |at| at + 1);
    let start = before[..end]
        .iter()
        .rposition(|byte| !is_word(*byte))
        .map_or(0, |at| at + 1);
    let word = &before[start..end];
    word.first().is_some_and(u8::is_ascii_uppercase) && word.iter().any(u8::is_ascii_lowercase)
}

/// The spans of the `field: field` entries between the braces of a struct literal.
fn redundant_fields(masked: &str, within: std::ops::Range<usize>) -> Vec<(usize, usize)> {
    let (mut found, mut depth, mut from) = (Vec::new(), 0isize, within.start);
    let mut entries = Vec::new();
    for at in within.clone() {
        match masked.as_bytes()[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                entries.push(from..at);
                from = at + 1;
            }
            _ => {}
        }
    }
    entries.push(from..within.end);
    for entry in entries {
        let text = masked[entry.clone()].trim();
        if let Some((field, value)) = text.split_once(':') {
            let (field, value) = (field.trim(), value.trim());
            let is_word = |text: &str| {
                !text.is_empty() && text.bytes().all(is_word)
            };
            if is_word(field) && field == value && !value.starts_with(':') {
                let start = entry.start + masked[entry.clone()].len() - masked[entry.clone()].trim_start().len();
                found.push((start, start + text.len()));
            }
        }
    }
    found
}

/// A note when `name` takes more than seven parameters (`&self` not counted) or returns a tuple of
/// three or more, which clippy flags and only the plan can fix.
pub(super) fn shape_notes(text: &str, name: &str) -> Vec<String> {
    let masked = masked_to_code(text);
    let Some(signature) = function_span(text, name).and_then(|span| signature(&masked, span)) else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    let count = signature
        .parameters
        .iter()
        .filter(|parameter| parameter.name != "self")
        .count();
    if count > MOST_PARAMETERS {
        notes.push(format!(
            "`{name}` takes {count} parameters; clippy::too_many_arguments fires above {MOST_PARAMETERS}"
        ));
    }
    let widest = signature
        .returns
        .map(|returns| widest_tuple(&masked[returns]))
        .unwrap_or(0);
    if widest >= 3 {
        notes.push(format!(
            "`{name}` returns a {widest}-tuple; clippy::type_complexity may fire"
        ));
    }
    notes
}

/// The number of elements of the widest tuple type written in `ty`.
fn widest_tuple(ty: &str) -> usize {
    let bytes = ty.as_bytes();
    (0..bytes.len())
        .filter(|at| bytes[*at] == b'(' && (*at == 0 || !is_word(bytes[at - 1])))
        .map(|open| {
            let (mut depth, mut elements, mut pending) = (0usize, 0usize, false);
            for byte in &bytes[open..] {
                match byte {
                    b'(' | b'[' | b'<' => depth += 1,
                    b')' | b']' | b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b',' if depth == 1 => {
                        elements += 1;
                        pending = false;
                    }
                    byte if !byte.is_ascii_whitespace() && depth >= 1 => pending = true,
                    _ => {}
                }
            }
            elements + usize::from(pending)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_tail_wrapped_in_ok_becomes_a_statement_followed_by_ok_unit() {
        let produced = "fn guarded(dir: &Path, flag: bool) -> Result<(), String> {\n    \
                        Ok(if flag {\n        let _p = dir.join(\"a\");\n        \
                        Err::<(), String>(\"e\".into())?;\n    })\n}\n";

        assert_eq!(
            unit_tail_unwrapped(produced, "guarded"),
            "fn guarded(dir: &Path, flag: bool) -> Result<(), String> {\n    \
             if flag {\n        let _p = dir.join(\"a\");\n        \
             Err::<(), String>(\"e\".into())?;\n    }\n    Ok(())\n}\n"
        );
    }

    #[test]
    fn a_value_tail_in_ok_is_left_alone() {
        let produced =
            "fn counted(n: u32) -> Result<u32, String> {\n    Ok(if n > 1 { n } else { 1 })\n}\n";

        assert_eq!(unit_tail_unwrapped(produced, "counted"), produced);
    }

    #[test]
    fn a_field_initialised_from_a_binding_of_its_own_name_is_written_in_shorthand() {
        let produced = "fn looked_up(base: &Path, root: &Path) -> usize {\n    \
                        let n = consume(&Lookup { base: base, root: root });\n    n\n}\n";

        assert_eq!(
            field_shorthand(produced, "looked_up"),
            "fn looked_up(base: &Path, root: &Path) -> usize {\n    \
             let n = consume(&Lookup { base, root });\n    n\n}\n"
        );
    }

    #[test]
    fn notes_more_than_seven_parameters_and_a_three_tuple_return() {
        let produced = "fn wide(&self, a: u8, b: u8, c: u8, d: u8, e: u8, f: u8, g: u8, h: u8) \
                        -> (u8, u8, u8) {\n    (a, b, c)\n}\n";

        assert_eq!(
            shape_notes(produced, "wide"),
            [
                "`wide` takes 8 parameters; clippy::too_many_arguments fires above 7",
                "`wide` returns a 3-tuple; clippy::type_complexity may fire",
            ]
        );
    }
}
