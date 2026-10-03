use crate::edit::Position;

use super::super::early_return::opens_a_function_body;

use super::names_bound;

use super::super::early_return::masked_to_code;

use super::super::early_return::byte_offset;

use crate::edit::Range;

/// A path the parent's `use` declaration names, as the module the seam becomes has to write it.
///
/// That module is a **child** of the file's own, so a path relative to the file's module starts one
/// level too high there: the parent's `super::X` is `super::super::X` in the child, and its `self::X`
/// is `super::X`. Written verbatim, `use super::SplitStartFailure;` in a module under
/// `svc_spawn_split_agent` named `svc_spawn_split_agent::SplitStartFailure`, which does not exist.
///
/// A path from the crate root (`crate::`), an absolute one (`::`) and one through an extern crate
/// mean the same thing from any module, and are left alone. So is a bare path through an item the
/// parent declares (`sibling::X`, reached through 2018's uniform paths): it cannot be told from an
/// extern crate by reading, and the verification behind every reconstruction refuses it by name.
pub(in super::super) fn rebased_for_child(path: &str) -> String {
    match path.split("::").next() {
        Some("super") => format!("super::{path}"),
        Some("self") => format!("super{}", &path["self".len()..]),
        _ => path.to_string(),
    }
}

/// The function-local `use` items of the function spanning `origin_function` that bind any of
/// `names` — what an extracted function not nested in its origin must carry into its own body.
///
/// Each is returned as written, so the carried `use` stays local and means what it did. Read over
/// the masked code, so a `use` in a comment or a string is not one.
pub(in super::super) fn function_local_uses_reaching(
    text: &str,
    origin_function: Range,
    names: &[String],
) -> Vec<String> {
    let (Some(from), Some(to)) = (
        byte_offset(text, origin_function.start),
        byte_offset(text, origin_function.end),
    ) else {
        return Vec::new();
    };
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let mut carried = Vec::new();

    let mut at = from;
    while at < to {
        let starts_a_statement = code[..at]
            .iter()
            .rev()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| matches!(byte, b'{' | b';' | b'}'));
        let is_use = code[at..to].starts_with(b"use")
            && code
                .get(at + 3)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'{');
        let word_starts_here =
            at == 0 || !(code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_');
        if starts_a_statement && is_use && word_starts_here {
            let Some(end) = code[at..to].iter().position(|byte| *byte == b';') else {
                break;
            };
            let statement = &text[at..at + end + 1];
            if names_bound(statement)
                .iter()
                .any(|bound| names.contains(bound))
            {
                carried.push(statement.to_string());
            }
            at += end + 1;
        } else {
            at += 1;
        }
    }

    carried
}

/// `extracted` with the function-local `use` items its new function `name` needs written into the
/// new function's body.
///
/// rust-analyzer puts the new function beside the one it came from, where the origin's own `use`
/// items are not in scope. A `use` in the origin that binds a name the range at `range` mentions
/// is carried, unless the range itself already holds it (it travelled with the range). The origin
/// is read off `original`, the file before the assist ran; its own `use` is left as written.
pub(in super::super) fn carry_function_local_uses(
    original: &str,
    extracted: &str,
    range: Range,
    name: &str,
) -> String {
    let carried = local_uses_to_carry(original, range);
    if carried.is_empty() {
        return extracted.to_string();
    }
    let Some(body) = body_of_function(extracted, name) else {
        return extracted.to_string();
    };

    let indent = indentation_of_line_holding(extracted, body.opened);
    let written: String = carried
        .iter()
        .map(|statement| format!("\n{indent}    {statement}"))
        .collect();

    let mut carried_into = extracted.to_string();
    carried_into.insert_str(body.opened + 1, &written);
    carried_into
}

/// The `use` items of the function around `range` that bind a name the range mentions.
///
/// What `check` reports ahead of time and what `apply` carries.
pub(in super::super) fn local_uses_to_carry(original: &str, range: Range) -> Vec<String> {
    let Some(function) = enclosing_function(original, range) else {
        return Vec::new();
    };
    let (Some(from), Some(to)) = (
        byte_offset(original, range.start),
        byte_offset(original, range.end),
    ) else {
        return Vec::new();
    };
    if from >= to {
        return Vec::new();
    }
    let masked = masked_to_code(original);
    let mentioned: Vec<String> = masked[from..to]
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();

    function_local_uses_reaching(original, function, &mentioned)
        .into_iter()
        .filter(|statement| {
            // A `use` the range holds travels with it.
            !original[from..to].contains(statement.as_str())
        })
        .collect()
}

/// The body of the function that encloses `range`, from its `{` to just past its `}`.
fn enclosing_function(text: &str, range: Range) -> Option<Range> {
    let from = byte_offset(text, range.start)?;
    let masked = masked_to_code(text);
    let code = masked.as_bytes();

    let mut depth = 0usize;
    let open = (0..from).rev().find(|at| match code[*at] {
        b'}' => {
            depth += 1;
            false
        }
        b'{' if depth > 0 => {
            depth -= 1;
            false
        }
        b'{' => opens_a_function_body(code, *at),
        _ => false,
    })?;

    let mut depth = 0usize;
    let close = (open..code.len()).find(|at| match code[*at] {
        b'{' => {
            depth += 1;
            false
        }
        b'}' => {
            depth -= 1;
            depth == 0
        }
        _ => false,
    })?;

    Some(Range {
        start: position_of(text, open),
        end: position_of(text, close + 1),
    })
}

/// Where the body of `fn name` opens in `text`, as the byte offset of its brace.
struct FunctionBody {
    pub(crate) opened: usize,
}

fn body_of_function(text: &str, name: &str) -> Option<FunctionBody> {
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let declaration = format!("fn {name}");
    let at = masked.match_indices(&declaration).find_map(|(at, _)| {
        let ends = code
            .get(at + declaration.len())
            .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'));
        let begins = at == 0 || !(code[at - 1].is_ascii_alphanumeric() || code[at - 1] == b'_');
        (ends && begins).then_some(at)
    })?;

    let mut depth = 0isize;
    let opened = (at..code.len()).find(|at| match code[*at] {
        b'(' | b'[' => {
            depth += 1;
            false
        }
        b')' | b']' => {
            depth -= 1;
            false
        }
        b'{' => depth == 0,
        _ => false,
    })?;
    Some(FunctionBody { opened })
}

/// The leading whitespace of the line holding byte `offset`.
fn indentation_of_line_holding(text: &str, offset: usize) -> &str {
    let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let line = &text[line_start..];
    &line[..line.len() - line.trim_start().len()]
}

/// The one-based line and character column of a byte offset.
fn position_of(text: &str, offset: usize) -> Position {
    let line_start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    Position {
        line: text[..offset].matches('\n').count() as u32 + 1,
        col: text[line_start..offset].chars().count() as u32 + 1,
    }
}
