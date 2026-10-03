//! An `extract_method` whose range holds an early exit of the function around it.
//!
//! Split out of `backends/rust.rs`, which is past its size budget.

use super::seam_refusal;
use crate::crate_move::{readable_spans, Prose};
use crate::edit::{Position, Range};
use crate::Result;

/// Refuse an `extract_method` whose range holds a `return` that exits the enclosing function.
///
/// rust-analyzer's "extract into function" copies such a `return` verbatim into the new function.
/// There it returns from the new function, whose return type is not the caller's: plan 10 of the
/// lifecycle destructure applied six of these, reported "applied 6 of 6", and left seven `E0308`s.
/// Where the types happen to agree it is worse — the caller's early exit is silently skipped. No
/// rewrite of the extracted text repairs that, so the range is refused before the server is asked.
///
/// **Except at the end of the function.** A range that runs to the function body's own closing
/// brace, ending with its tail expression, is the function's tail: rust-analyzer keeps the `return`
/// verbatim, the call replaces the range as the tail expression, and the new function's return type
/// is the tail's, which is the caller's. A `return` there returns the same value from the same call,
/// so nothing is skipped and nothing changes type. A range ending with a statement instead (the
/// body's last `return …;`) is still refused: rust-analyzer then rewrites every `return` into an
/// `Option` it matches at the call, and the caller is left with no tail (`E0317`). What
/// [`runs_to_the_end_of_a_function`] reads is stated there; a signature rust-analyzer infers
/// differently from the caller's (an `impl Trait` it spells out) is left to `apply`'s compile gate.
///
/// **Not when that function returns `()`** ([`ends_in_a_unit_tail`]): there is no tail value to
/// return, and the assist rewrites the `return` into a `ControlFlow` the tree does not import.
///
/// Read from the text, so it costs no index, and the same refusal reaches a plain `check`, a
/// `check --deep` (whose static tier runs first) and an `apply`. What [`early_returns`] relies on is
/// stated there.
pub(super) fn refuse_early_returns(text: &str, range: Range) -> Result<()> {
    let found = early_returns(text, range);
    if found.is_empty()
        || (runs_to_the_end_of_a_function(text, range) && !ends_in_a_unit_tail(text, range))
    {
        return Ok(());
    }

    let source: Vec<&str> = text.split('\n').collect();
    let named: Vec<String> = found
        .iter()
        .map(|line| {
            let written = source
                .get(*line as usize - 1)
                .map_or("", |text| text.trim());
            format!("line {line} (`{written}`)")
        })
        .collect();

    Err(seam_refusal(format!(
        "the range returns early from the function around it, on {}. An extracted function cannot \
         carry an early exit of its caller: the assist copies the `return` verbatim, so it returns \
         from the new function instead — whose return type differs, which is `E0308` at best and a \
         silently skipped exit at worst. Cut the range so it holds no `return`, end it before the \
         first one, or run it to the end of the function's tail expression, where the call becomes \
         the tail and a `return` means what it did.",
        named.join(" and ")
    )))
}

/// Whether the range ends in its function's tail expression **and** that function returns `()`.
///
/// The tail-range exception lets a range holding a `return` through when it runs to the end of
/// the function, because the extracted function can then return what the tail did. A `()` tail
/// has nothing to return, and rust-analyzer rewrites the early `return` into `ControlFlow` — which
/// the tree does not import. Such a range is refused instead.
pub(super) fn ends_in_a_unit_tail(text: &str, range: Range) -> bool {
    let Some(to) = byte_offset(text, range.end) else {
        return false;
    };
    if !runs_to_the_end_of_a_function(text, range) {
        return false;
    }
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let Some(close) = (to..code.len()).find(|at| !code[*at].is_ascii_whitespace()) else {
        return false;
    };
    matching_open_brace(code, close)
        .and_then(|open| function_header(code, open))
        .is_some_and(|header| !declares_a_return_type(header))
}

/// Whether a function header, from its `fn` to its body, declares a return type: a `->` outside the
/// parameter list and the generics, before any `where` clause.
fn declares_a_return_type(header: &[u8]) -> bool {
    let (mut parentheses, mut angles) = (0isize, 0isize);
    let mut at = 0usize;
    while at < header.len() {
        match header[at] {
            b'(' | b'[' => parentheses += 1,
            b')' | b']' => parentheses -= 1,
            b'-' if header.get(at + 1) == Some(&b'>') => {
                if parentheses == 0 && angles == 0 {
                    return true;
                }
                at += 1;
            }
            b'<' => angles += 1,
            b'>' => angles -= 1,
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let end = body_scan::word_end(header, at);
                if &header[at..end] == b"where" && parentheses == 0 && angles == 0 {
                    return false;
                }
                at = end;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    false
}

/// The one-based lines inside `range` holding a `return` whose target is the enclosing function.
///
/// **What this relies on.** The text, lexed — not the server, because a plain `check` has none, and
/// no server answer says which body a `return` leaves. Strings, raw strings, character literals
/// and comments are masked first by the lexer the test-binary move already uses
/// ([`readable_spans`]), so neither a `return` nor a brace written inside one counts. What is left
/// is scanned for the constructs that open a body of their own, where a `return` exits that body
/// rather than the caller:
///
/// - a closure: `|…|` or `||` where an expression may start (so `a || b` stays an operator), with
///   a block body, a `-> T { … }` body, or an expression body that ends at the next `,`, `;` or
///   closing delimiter at its own depth;
/// - an `async` block, with or without `move`;
/// - a nested `fn name` — `fn(` is a pointer type and opens nothing.
///
/// A body opened *outside* the range is the enclosing function as far as the extraction is
/// concerned — statements lifted out of a closure body cannot carry its `return` either — which is
/// why only the range itself is scanned, starting at depth zero.
///
/// **Not seen:** a `return` a macro expands to (`bail!`, `ensure!`). Nothing in the text shows it;
/// `apply`'s compile gate is what catches the result.
fn early_returns(text: &str, range: Range) -> Vec<u32> {
    let (Some(from), Some(to)) = (byte_offset(text, range.start), byte_offset(text, range.end))
    else {
        return Vec::new();
    };
    if from >= to {
        return Vec::new();
    }

    let masked = masked_to_code(text);
    let mut scan = body_scan::Scan::default();
    scan.run(&masked.as_bytes()[from..to]);

    let mut lines: Vec<u32> = scan
        .returns
        .iter()
        .map(|offset| text[..from + offset].matches('\n').count() as u32 + 1)
        .collect();
    lines.dedup();
    lines
}

/// Whether `range` ends with the tail expression of a named function's body.
///
/// Read over the masked code, like [`early_returns`]: the range may not end with `;`, and after it
/// there may be only whitespace (a comment is masked to it) before a `}`, which must close a
/// function's body. The body's
/// `{` is found by matching braces backwards, and it is a function's when the tokens before it, back
/// to the previous `;`, `{` or `}`, hold `fn name`. Anything else — a closure's `|…| {`, an `if` or
/// `match` arm, an `async` block — is a body of its own or a block inside the function, whose end
/// is not the caller's. A `{` inside an unclosed `(` or `[` is an argument (a closure passed to a
/// call), never a function body.
fn runs_to_the_end_of_a_function(text: &str, range: Range) -> bool {
    let Some(to) = byte_offset(text, range.end) else {
        return false;
    };
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let ends_with_a_statement = code[..to]
        .iter()
        .rev()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b';');
    if ends_with_a_statement {
        return false;
    }
    let Some(close) = (to..code.len()).find(|at| !code[*at].is_ascii_whitespace()) else {
        return false;
    };
    if code[close] != b'}' {
        return false;
    }
    let Some(open) = matching_open_brace(code, close) else {
        return false;
    };
    opens_a_function_body(code, open)
}

/// The `{` that the `}` at `close` closes.
pub(super) fn matching_open_brace(code: &[u8], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for at in (0..close).rev() {
        match code[at] {
            b'}' => depth += 1,
            b'{' if depth == 0 => return Some(at),
            b'{' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Whether the `{` at `open` starts a function's body: `fn name` among the tokens of its header.
pub(super) fn opens_a_function_body(code: &[u8], open: usize) -> bool {
    function_header(code, open).is_some()
}

/// The header of the function whose body the `{` at `open` starts, from the tokens after the
/// previous `;`, `{` or `}` — or `None` when that `{` starts something else.
fn function_header(code: &[u8], open: usize) -> Option<&[u8]> {
    let mut depth = 0isize;
    let mut start = 0usize;
    for at in (0..open).rev() {
        match code[at] {
            b')' | b']' => depth += 1,
            b'(' | b'[' if depth == 0 => return None,
            b'(' | b'[' => depth -= 1,
            b';' | b'{' | b'}' if depth == 0 => {
                start = at + 1;
                break;
            }
            _ => {}
        }
    }

    let header = &code[start..open];
    let mut at = 0usize;
    while at < header.len() {
        if header[at].is_ascii_alphabetic() || header[at] == b'_' {
            let end = body_scan::word_end(header, at);
            if &header[at..end] == b"fn" && body_scan::next_is_identifier(header, end) {
                return Some(&header[at..]);
            }
            at = end;
        } else {
            at += 1;
        }
    }
    None
}

/// The byte offset of a one-based line and character column, clamped to the end of its line.
pub(super) fn byte_offset(text: &str, at: Position) -> Option<usize> {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(at.line.checked_sub(1)? as usize)
        .map(str::len)
        .sum();
    let line = text[line_start..].split('\n').next()?;
    let within = line
        .char_indices()
        .nth(at.col.saturating_sub(1) as usize)
        .map_or(line.len(), |(offset, _)| offset);
    Some(line_start + within)
}

/// `text` with every comment blanked and every literal reduced to a `0`, byte for byte.
///
/// Byte for byte, so an offset into the result is an offset into `text`. A literal becomes `0`
/// rather than blank because it ends an expression: `'a' | 'b'` in a pattern is an operator, and a
/// blank would leave `|` reading as the start of a closure. Newlines are kept everywhere.
pub(super) fn masked_to_code(text: &str) -> String {
    let mut masked: Vec<u8> = text
        .bytes()
        .map(|byte| if byte == b'\n' { b'\n' } else { b' ' })
        .collect();
    let mut literal_from = 0usize;
    let literal = |masked: &mut Vec<u8>, from: usize, to: usize| {
        if let Some(first) = (from..to).find(|at| masked[*at] != b'\n') {
            masked[first] = b'0';
        }
    };

    for (span, kind) in readable_spans(text) {
        literal(&mut masked, literal_from, span.start);
        if kind == Prose::Code {
            masked[span.clone()].copy_from_slice(&text.as_bytes()[span.clone()]);
        }
        literal_from = span.end;
    }
    literal(&mut masked, literal_from, text.len());

    // Every byte is ASCII except those copied whole from code spans, which begin and end on
    // character boundaries.
    String::from_utf8(masked).expect("masking keeps the text valid UTF-8")
}

mod body_scan;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;

    /// A function body, one entry per line, so each test's line numbers can be read off it.
    fn a_body(lines: &[&str]) -> String {
        lines.join("\n") + "\n"
    }

    /// Whole lines `first..=last`, from column 1 to past the end of the longest plausible line.
    fn lines(first: u32, last: u32) -> Range {
        Range {
            start: Position {
                line: first,
                col: 1,
            },
            end: Position {
                line: last,
                col: 200,
            },
        }
    }

    #[test]
    fn finds_a_return_that_exits_the_enclosing_function() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> Result<u32, String> {",
            "    let level = 1;",
            "    if x { return Ok(1); }",
            "    Ok(level)",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 3));

        // Then
        assert_eq!(found, vec![3]);
    }

    #[test]
    fn leaves_a_return_inside_a_closure_body() {
        // Given
        let text = a_body(&[
            "fn start(items: &[u32]) -> u32 {",
            "    let first = items.iter().map(|item| { return item * 2; }).sum::<u32>();",
            "    let second = items.iter().map(|item| return item + 1).sum::<u32>();",
            "    first + second",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 3));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    #[test]
    fn leaves_a_return_inside_a_closure_with_a_declared_return_type() {
        // Given
        let text = a_body(&[
            "fn start() -> u32 {",
            "    let doubled = move |level: u32| -> u32 { return level * 2; };",
            "    doubled(2)",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 2));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    #[test]
    fn leaves_a_return_inside_an_async_block() {
        // Given
        let text = a_body(&[
            "async fn start() -> u32 {",
            "    let pending = async move { return 2; };",
            "    pending.await",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 2));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    #[test]
    fn leaves_a_return_inside_a_nested_function() {
        // Given
        let text = a_body(&[
            "fn start() -> u32 {",
            "    fn helper(level: u32) -> u32 {",
            "        return level * 2;",
            "    }",
            "    helper(2)",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 4));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    #[test]
    fn leaves_a_return_written_in_a_string_or_a_comment() {
        // Given
        let text = a_body(&[
            "fn start() -> &'static str {",
            "    let said = \"return early\"; // return here?",
            "    let raw = r#\"} return {\"#; /* return */",
            "    said",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 3));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    #[test]
    fn finds_a_return_after_a_closure_has_ended() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> u32 {",
            "    let doubled = |level: u32| level * 2;",
            "    if x { return doubled(1); }",
            "    0",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 3));

        // Then
        assert_eq!(found, vec![3]);
    }

    #[test]
    fn reads_a_logical_or_as_an_operator_rather_than_a_closure() {
        // Given
        let text = a_body(&[
            "fn start(x: bool, y: bool) -> u32 {",
            "    if x || y { return 1; }",
            "    0",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 2));

        // Then
        assert_eq!(found, vec![2]);
    }

    #[test]
    fn reads_a_brace_in_a_character_literal_as_data() {
        // Given
        let text = a_body(&[
            "fn start(c: char) -> u32 {",
            "    let open = |seen: char| seen == '}';",
            "    if open(c) { return 1; }",
            "    0",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 3));

        // Then
        assert_eq!(found, vec![3]);
    }

    #[test]
    fn leaves_identifiers_that_merely_contain_the_word() {
        // Given
        let text = a_body(&[
            "fn start(returned: u32) -> u32 {",
            "    let early_return = returned + r#return();",
            "    early_return",
            "}",
        ]);

        // When
        let found = early_returns(&text, lines(2, 2));

        // Then
        assert_eq!(found, Vec::<u32>::new());
    }

    /// The call replaces the function's own tail, so a `return` in the new function returns what
    /// the caller would have: its return type is the tail's, which is the caller's.
    #[test]
    fn allows_a_return_in_a_range_that_runs_to_the_end_of_the_function() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> Result<u32, String> {",
            "    let level = 2;",
            "    if x {",
            "        return Ok(1);",
            "    }",
            "    Ok(level) // the tail",
            "}",
        ]);

        // When
        let checked = refuse_early_returns(&text, lines(2, 6));

        // Then
        assert!(checked.is_ok(), "{checked:?}");
    }

    #[test]
    fn allows_a_return_in_the_tail_of_a_method_whose_signature_spans_lines() {
        // Given
        let text = a_body(&[
            "impl Host {",
            "    pub(crate) fn start(",
            "        &self,",
            "        x: bool,",
            "    ) -> Result<u32, String>",
            "    where",
            "        Self: Sized,",
            "    {",
            "        if x { return Ok(1); }",
            "        Ok(2)",
            "    }",
            "}",
        ]);

        // When
        let checked = refuse_early_returns(&text, lines(9, 10));

        // Then
        assert!(checked.is_ok(), "{checked:?}");
    }

    /// A body ending in `return …;` has no tail expression, and rust-analyzer rewrites the returns
    /// of such a range into an `Option` matched at the call, which leaves the caller without a tail.
    #[test]
    fn refuses_a_return_in_a_range_that_ends_with_the_functions_last_return() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> Result<u32, String> {",
            "    let level = 2;",
            "    if x { return Ok(1); }",
            "    return Ok(level);",
            "}",
        ]);

        // When
        let found = refuse_early_returns(&text, lines(2, 4)).map_err(|error| error.to_string());

        // Then
        assert!(
            found.as_ref().is_err_and(|refusal| refusal
                .contains("on line 3 (`if x { return Ok(1); }`) and line 4 (`return Ok(level);`)")),
            "{found:?}"
        );
    }

    /// The end of a block inside the function is not the end of the function: the statements after
    /// that block still run once the extracted code has returned.
    #[test]
    fn refuses_a_return_in_a_range_that_runs_to_the_end_of_an_inner_block() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> u32 {",
            "    let level = if x {",
            "        if level_is_known() { return 1; }",
            "        2",
            "    } else { 3 };",
            "    level",
            "}",
        ]);

        // When
        let checked = refuse_early_returns(&text, lines(3, 4));

        // Then
        assert!(
            checked.is_err(),
            "the range ends at an `if` block, not the function"
        );
    }

    /// A closure's body is not the enclosing function's, so a range at its end does not end the
    /// function the `return` would leave.
    #[test]
    fn refuses_a_return_in_a_range_that_runs_to_the_end_of_a_closure_body() {
        // Given
        let text = a_body(&[
            "fn start(items: &[u32]) -> Vec<u32> {",
            "    items.iter().map(|item| {",
            "        if *item > 2 { return 0; }",
            "        *item",
            "    }).collect()",
            "}",
        ]);

        // When
        let checked = refuse_early_returns(&text, lines(3, 4));

        // Then
        assert!(
            checked.is_err(),
            "the range ends at a closure body, not the function"
        );
    }

    #[test]
    fn refuses_naming_each_line_and_why_the_extraction_cannot_carry_it() {
        // Given
        let text = a_body(&[
            "fn start(x: bool) -> Result<u32, String> {",
            "    if x { return Ok(1); }",
            "    let level = 2;",
            "    if level > 1 {",
            "        return Err(String::new());",
            "    }",
            "    Ok(level)",
            "}",
        ]);

        // When
        let refusal = refuse_early_returns(&text, lines(2, 6)).map_err(|error| error.to_string());

        // Then
        assert_eq!(
            refusal,
            Err("this seam cannot be cut here: the range returns early from the function around \
                 it, on line 2 (`if x { return Ok(1); }`) and line 5 (`return Err(String::new());`). \
                 An extracted function cannot carry an early exit of its caller: the assist copies \
                 the `return` verbatim, so it returns from the new function instead — whose return \
                 type differs, which is `E0308` at best and a silently skipped exit at worst. Cut \
                 the range so it holds no `return`, end it before the first one, or run it to the \
                 end of the function's tail expression, where the call becomes the tail and a \
                 `return` means what it did."
                .to_string())
        );
    }

    #[test]
    fn a_range_ending_in_the_tail_of_a_unit_function_ends_in_a_unit_tail() {
        // Given `f`, which returns `()`, and a range from its `let` to its end
        let text = "fn f(x: Option<u32>) {\n    let Some(v) = x else { return; };\n    \
                    if v > 1 {\n        drop(v);\n    }\n}\n";
        let range = Range {
            start: crate::edit::Position { line: 2, col: 5 },
            end: crate::edit::Position { line: 5, col: 6 },
        };

        assert!(ends_in_a_unit_tail(text, range));
    }

    #[test]
    fn a_range_ending_in_the_tail_of_a_function_returning_a_value_does_not() {
        let text = "fn f(x: Option<u32>) -> u32 {\n    let Some(v) = x else { return 0; };\n    \
                    v + 1\n}\n";
        let range = Range {
            start: crate::edit::Position { line: 2, col: 5 },
            end: crate::edit::Position { line: 3, col: 10 },
        };

        assert!(!ends_in_a_unit_tail(text, range));
    }
}
