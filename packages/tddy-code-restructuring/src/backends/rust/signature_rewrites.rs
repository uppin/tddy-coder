//! The signature and call-site operations, authored here rather than delegated to an assist.
//!
//! rust-analyzer has no assist that retypes one parameter, adds one, reorders them, or edits one
//! call's arguments, so each of these is an edit this backend writes — to the declaration's parameter
//! list or `-> …` for the signature operations, to one call's argument list for the call-site ones.
//! None reaches past the text it was anchored on: a caller is its own operation, which is what a
//! transactional group makes one unit.
//!
//! Everything here reads the *masked* code ([`masked_to_code`]), in which comments and literals can
//! carry no delimiter, and writes only the span it changes, so the formatting around it survives.

use super::early_return::masked_to_code;
use super::seam_refusal;
use super::signature::{arrow_before, offset_at, parameter_list, skip_whitespace};
use crate::edit::{Position, Range, TextEdit};
use crate::plan::rust_syntax::permutation;
use crate::plan::{OrderKey, RefactorKind, RefactorOp};
use crate::Result;

/// A half-open byte range of the text an operation reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    from: usize,
    to: usize,
}

impl Span {
    fn of(self, text: &str) -> &str {
        &text[self.from..self.to]
    }
}

/// One span of the text, and what it becomes.
///
/// An operation answers with the spans it changes rather than with the rewritten file, so the edits
/// it reports are exactly as narrow as the change: a later operation of the same plan anchored on
/// the same line — the function's own name, say — is still addressable.
#[derive(Debug, PartialEq, Eq)]
struct Replacement {
    span: Span,
    with: String,
}

/// `span` replaced by `with`.
fn replacing(span: Span, with: &str) -> Vec<Replacement> {
    vec![Replacement {
        span,
        with: with.to_string(),
    }]
}

/// `inserted` put in at `at`.
fn inserting(at: usize, inserted: &str) -> Vec<Replacement> {
    replacing(Span { from: at, to: at }, inserted)
}

/// `replacements` as the edits of `text`, in the file's own one-based coordinates.
fn edits_of(text: &str, replacements: Vec<Replacement>) -> Vec<TextEdit> {
    let position = |offset: usize| {
        let before = &text[..offset];
        Position {
            line: before.matches('\n').count() as u32 + 1,
            col: before.rsplit('\n').next().map_or(0, str::len) as u32 + 1,
        }
    };
    replacements
        .into_iter()
        .map(|replacement| TextEdit {
            range: Range {
                start: position(replacement.span.from),
                end: position(replacement.span.to),
            },
            new_text: replacement.with,
        })
        .collect()
}

/// The edits that rewrite the declaration of the function whose own name starts at `name_at` as
/// `op` asks.
///
/// Only `change_param_type`, `add_param`, `reorder_params` and `change_return_type` with a `type`
/// are rewrites of this kind; `change_return_type` with a `variant` is an assist's.
pub(super) fn rewrite_declaration(
    text: &str,
    op: &RefactorOp,
    name_at: Position,
) -> Result<Vec<TextEdit>> {
    let code = masked_to_code(text);
    let (open, close) = offset_at(text, name_at)
        .and_then(|name| parameter_list(&code, name))
        .ok_or_else(|| seam_refusal("the anchor does not name a function with a parameter list"))?;
    let parameters = parameters(&code, open, close);

    let replacements = match op.op {
        RefactorKind::ChangeParamType => change_param_type(&code, &parameters, op),
        RefactorKind::AddParam => add_param(&code, (open, &parameters), op),
        RefactorKind::ReorderParams => reorder_params(text, &code, &parameters, op),
        RefactorKind::ChangeReturnType => change_return_type(&code, close, op),
        other => Err(super::failure(format!(
            "{other:?} does not rewrite a declaration"
        ))),
    }?;
    Ok(edits_of(text, replacements))
}

/// The type the function whose own name starts at `name_at` returns, as written, or `None` when it
/// declares none.
pub(super) fn returned_type(text: &str, name_at: Position) -> Option<String> {
    let code = masked_to_code(text);
    let (_, close) = parameter_list(&code, offset_at(text, name_at)?)?;
    let span = return_type_span(&code, close)?;
    Some(span.of(text).to_string())
}

fn change_param_type(code: &str, parameters: &[Span], op: &RefactorOp) -> Result<Vec<Replacement>> {
    let name = required(op.name.as_deref(), "name")?;
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    let parameter = parameters
        .iter()
        .find(|parameter| binding(code, **parameter) == Some(name))
        .ok_or_else(|| {
            seam_refusal(format!(
                "`{name}` is not a parameter of the function the anchor names"
            ))
        })?;
    let colon = separator_colon(code, *parameter)
        .ok_or_else(|| seam_refusal(format!("`{name}` declares no type to change")))?;
    let from = skip_whitespace(code.as_bytes(), colon + 1);
    Ok(replacing(
        Span {
            from,
            to: parameter.to,
        },
        new_type,
    ))
}

fn add_param(
    code: &str,
    (open, parameters): (usize, &[Span]),
    op: &RefactorOp,
) -> Result<Vec<Replacement>> {
    let name = required(op.name.as_deref(), "name")?;
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    let position = required(op.variant.as_deref(), "variant")?;

    if parameters
        .iter()
        .any(|parameter| binding(code, *parameter) == Some(name))
    {
        return Err(seam_refusal(format!(
            "`{name}` is already a parameter of the function the anchor names"
        )));
    }

    // A receiver stays first: `first` means first after it.
    let receivers = parameters
        .iter()
        .take_while(|parameter| binding(code, **parameter).is_none())
        .count();
    let index = match position {
        "first" => receivers,
        "last" => parameters.len(),
        after => {
            let after = after.strip_prefix("after:").unwrap_or(after);
            parameters
                .iter()
                .position(|parameter| binding(code, *parameter) == Some(after))
                .ok_or_else(|| {
                    seam_refusal(format!(
                        "`{after}` is not a parameter of the function the anchor names"
                    ))
                })?
                + 1
        }
    };
    Ok(with_an_entry_at(
        open,
        parameters,
        index,
        &format!("{name}: {new_type}"),
    ))
}

fn reorder_params(
    text: &str,
    code: &str,
    parameters: &[Span],
    op: &RefactorOp,
) -> Result<Vec<Replacement>> {
    let named: Vec<(Span, String)> = parameters
        .iter()
        .filter_map(|parameter| Some((*parameter, binding(code, *parameter)?.to_string())))
        .collect();
    let current: Vec<OrderKey> = named
        .iter()
        .map(|(_, name)| OrderKey::Name(name.clone()))
        .collect();
    let moved = permutation(&current, &op.order)?;
    let spans: Vec<Span> = named.iter().map(|(span, _)| *span).collect();
    Ok(reordered(text, &spans, &moved))
}

fn change_return_type(code: &str, close: usize, op: &RefactorOp) -> Result<Vec<Replacement>> {
    let new_type = required(op.type_.as_deref(), "type")?.trim();
    Ok(match return_type_span(code, close) {
        Some(declared) => replacing(declared, new_type),
        None => inserting(close + 1, &format!(" -> {new_type}")),
    })
}

/// The `T` of `-> T` after the parameter list closing at `close`, up to the body, a `where` clause
/// or the `;` of a declaration without a body.
fn return_type_span(code: &str, close: usize) -> Option<Span> {
    let bytes = code.as_bytes();
    let arrow = skip_whitespace(bytes, close + 1);
    if !code[arrow..].starts_with("->") {
        return None;
    }
    let from = skip_whitespace(bytes, arrow + "->".len());

    let mut depth = 0usize;
    let mut to = from;
    while to < bytes.len() {
        match bytes[to] {
            b'(' | b'[' | b'<' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b'>' if !arrow_before(bytes, to) => depth = depth.saturating_sub(1),
            b'{' | b';' if depth == 0 => break,
            b'w' if depth == 0 && begins_the_word(code, to, "where") => break,
            _ => {}
        }
        to += 1;
    }
    let to = from + code[from..to].trim_end().len();
    (to > from).then_some(Span { from, to })
}

fn begins_the_word(code: &str, at: usize, word: &str) -> bool {
    let bytes = code.as_bytes();
    let is_word_byte = |byte: &u8| byte.is_ascii_alphanumeric() || *byte == b'_';
    code[at..].starts_with(word)
        && !at
            .checked_sub(1)
            .is_some_and(|before| is_word_byte(&bytes[before]))
        && !bytes.get(at + word.len()).is_some_and(is_word_byte)
}

/// The parameters between the parentheses at `open` and `close`, each trimmed of its whitespace.
fn parameters(code: &str, open: usize, close: usize) -> Vec<Span> {
    entries(code, open, close, true)
}

/// The entries between the delimiters at `open` and `close`, split at the commas outside any
/// brackets (and, for a parameter list, outside any generic arguments), each trimmed.
fn entries(code: &str, open: usize, close: usize, counting_angles: bool) -> Vec<Span> {
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut from = open + 1;
    let mut push = |from: usize, to: usize| {
        let trimmed = code[from..to].trim();
        if !trimmed.is_empty() {
            let start = from + (code[from..to].len() - code[from..to].trim_start().len());
            found.push(Span {
                from: start,
                to: start + trimmed.len(),
            });
        }
    };
    for index in open + 1..close {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b'<' if counting_angles => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'>' if counting_angles && !arrow_before(bytes, index) => {
                depth = depth.saturating_sub(1)
            }
            b',' if depth == 0 => {
                push(from, index);
                from = index + 1;
            }
            _ => {}
        }
    }
    push(from, close);
    found
}

/// What a parameter binds — `price` for `price: u32` and `mut price: u32` — or `None` for a
/// receiver, which binds `self` and declares no type.
fn binding(code: &str, parameter: Span) -> Option<&str> {
    let colon = separator_colon(code, parameter)?;
    let pattern = code[parameter.from..colon].trim();
    Some(
        pattern
            .strip_prefix("mut ")
            .map_or(pattern, str::trim_start),
    )
}

/// The `:` between a parameter's pattern and its type, which is never half of a `::`.
fn separator_colon(code: &str, parameter: Span) -> Option<usize> {
    let bytes = code.as_bytes();
    (parameter.from..parameter.to).find(|at| {
        bytes[*at] == b':'
            && bytes.get(at + 1) != Some(&b':')
            && at.checked_sub(1).is_none_or(|before| bytes[before] != b':')
    })
}

/// `entry` made the entry at `index` of the list opening at `open`, leaving the separators and
/// layout around the others as they were.
fn with_an_entry_at(open: usize, entries: &[Span], index: usize, entry: &str) -> Vec<Replacement> {
    match (entries.get(index), entries.last()) {
        (Some(before), _) => inserting(before.from, &format!("{entry}, ")),
        (None, Some(last)) => inserting(last.to, &format!(", {entry}")),
        (None, None) => inserting(open + 1, entry),
    }
}

/// The entry at `spans[k]` made to hold what was at `spans[moved[k]]`, for every `k` that changes.
fn reordered(text: &str, spans: &[Span], moved: &[usize]) -> Vec<Replacement> {
    spans
        .iter()
        .zip(moved)
        .enumerate()
        .filter(|(place, (_, source))| place != *source)
        .map(|(_, (span, source))| Replacement {
            span: *span,
            with: spans[*source].of(text).to_string(),
        })
        .collect()
}

fn required<'a>(field: Option<&'a str>, name: &str) -> Result<&'a str> {
    field.ok_or_else(|| super::failure(format!("the operation needs `{name}`")))
}

/// The call expression `range` covers, with the arguments it passes.
struct Call {
    /// The offset of the `(` opening the argument list.
    open: usize,
    arguments: Vec<Span>,
}

/// The edits that rewrite the call `range` covers in `text` as `op` asks.
///
/// The range must be exactly one call expression — `callee(arguments)` or
/// `receiver.method(arguments)` — and is refused otherwise, so an operation can never land on the
/// arguments of a call nested somewhere inside it.
pub(super) fn rewrite_call(text: &str, op: &RefactorOp, range: Range) -> Result<Vec<TextEdit>> {
    let call = call_in(text, range)?;
    let arguments = &call.arguments;
    let position = required(op.variant.as_deref(), "variant");

    let replacements = match op.op {
        RefactorKind::AddCallArg => {
            let index = argument_position(position?, arguments.len(), arguments.len() + 1)?;
            let expr = required(op.expr.as_deref(), "expr")?.trim();
            Ok(with_an_entry_at(call.open, arguments, index, expr))
        }
        RefactorKind::ChangeCallArg => {
            let index = argument_position(position?, arguments.len(), arguments.len())?;
            let expr = required(op.expr.as_deref(), "expr")?.trim();
            Ok(replacing(arguments[index], expr))
        }
        RefactorKind::RemoveCallArg => {
            let index = argument_position(position?, arguments.len(), arguments.len())?;
            Ok(replacing(removal_span(arguments, index), ""))
        }
        RefactorKind::ReorderCallArgs => {
            let current: Vec<OrderKey> = (1..=arguments.len() as u32)
                .map(OrderKey::Position)
                .collect();
            let moved = permutation(&current, &op.order)?;
            Ok(reordered(text, arguments, &moved))
        }
        other => Err(super::failure(format!("{other:?} does not edit a call"))),
    }?;
    Ok(edits_of(text, replacements))
}

/// What deleting `arguments[index]` removes: the argument and the comma that separated it from its
/// neighbour, the one after it when there is one.
fn removal_span(arguments: &[Span], index: usize) -> Span {
    match (index.checked_sub(1), arguments.get(index + 1)) {
        (_, Some(next)) => Span {
            from: arguments[index].from,
            to: next.from,
        },
        (Some(previous), None) => Span {
            from: arguments[previous].to,
            to: arguments[index].to,
        },
        (None, None) => arguments[index],
    }
}

/// The zero-based index `first`, `last` or a one-based position names among `len` arguments, which
/// may be up to `limit` for an operation that can also address the place after the last.
fn argument_position(position: &str, len: usize, limit: usize) -> Result<usize> {
    let index = match position {
        "first" => 0,
        "last" => limit.saturating_sub(1),
        number => number
            .parse::<usize>()
            .ok()
            .and_then(|one_based| one_based.checked_sub(1))
            .ok_or_else(|| {
                super::failure(format!(
                    "`{number}` is not `first`, `last` or a one-based position"
                ))
            })?,
    };
    if index >= limit {
        return Err(seam_refusal(format!(
            "`{position}` is past the call's {len} argument(s)"
        )));
    }
    Ok(index)
}

/// The call `range` covers, refusing a range that is anything else.
fn call_in(text: &str, range: Range) -> Result<Call> {
    let bounds = offset_at(text, range.start).zip(offset_at(text, range.end));
    let covered = bounds
        .filter(|(from, to)| from <= to)
        .and_then(|(from, to)| text.get(from..to))
        .ok_or_else(|| seam_refusal("the anchor's range lies outside its file"))?;
    let leading = covered.len() - covered.trim_start().len();
    let call_text = covered.trim();
    let from = bounds.map_or(0, |(from, _)| from) + leading;
    let to = from + call_text.len();

    let not_a_call = || {
        seam_refusal(format!(
            "`{call_text}` is not a call expression — the range must cover exactly one call, \
             `callee(arguments)` or `receiver.method(arguments)`"
        ))
    };
    let arity = match syn::parse_str::<syn::Expr>(call_text) {
        Ok(syn::Expr::Call(call)) => call.args.len(),
        Ok(syn::Expr::MethodCall(call)) => call.args.len(),
        _ => return Err(not_a_call()),
    };

    let code = masked_to_code(text);
    let open = opening_of_the_last_group(code.as_bytes(), from, to).ok_or_else(not_a_call)?;
    let arguments = arguments_of(text, &code, open, to - 1);
    if arguments.len() != arity {
        return Err(super::server_defect(format!(
            "`{call_text}` passes {arity} argument(s), but {} were read from it",
            arguments.len()
        )));
    }
    Ok(Call { open, arguments })
}

/// The `(` that matches the `)` ending `bytes[from..to]`.
fn opening_of_the_last_group(bytes: &[u8], from: usize, to: usize) -> Option<usize> {
    let mut depth = 0usize;
    for at in (from..to).rev() {
        match bytes[at] {
            b')' => depth += 1,
            b'(' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The arguments between the parentheses at `open` and `close`.
///
/// A comma outside any bracket is a separator unless it sits inside generic arguments
/// (`f::<A, B>(x)`) or a closure's parameters (`|a, b| a`), neither of which is bracketed. Those
/// are told apart the only reliable way: a piece that is not an expression by itself is joined to
/// the next until the two are one.
fn arguments_of(text: &str, code: &str, open: usize, close: usize) -> Vec<Span> {
    let mut arguments = Vec::new();
    let mut joined: Option<Span> = None;
    for piece in entries(code, open, close, false) {
        let candidate = Span {
            from: joined.map_or(piece.from, |pending| pending.from),
            to: piece.to,
        };
        if syn::parse_str::<syn::Expr>(candidate.of(text)).is_ok() {
            arguments.push(candidate);
            joined = None;
        } else {
            joined = Some(candidate);
        }
    }
    // A piece still unfinished is not an argument; the caller notices the count is short.
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Anchor, RefactorOp};
    use pretty_assertions::assert_eq;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    /// An operation of `kind` with every field absent but the ones `fill` sets.
    fn an_op(kind: RefactorKind, fill: impl FnOnce(&mut RefactorOp)) -> RefactorOp {
        let mut op = RefactorOp {
            id: None,
            op: kind,
            anchor: Anchor::Range {
                file: "src/lib.rs".to_string(),
                start: at(1, 1),
                end: at(1, 1),
            },
            name: None,
            to: None,
            variant: None,
            with_private_deps: false,
            reexport: None,
            to_file: false,
            also: Vec::new(),
            group: None,
            type_: None,
            expr: None,
            order: Vec::new(),
        };
        fill(&mut op);
        op
    }

    /// `text` with `edits` applied.
    fn applied(text: &str, edits: Vec<TextEdit>) -> String {
        let mut applied = text.to_string();
        for edit in edits.iter().rev() {
            let from = offset_at(text, edit.range.start).expect("an edit starts in the text");
            let to = offset_at(text, edit.range.end).expect("an edit ends in the text");
            applied.replace_range(from..to, &edit.new_text);
        }
        applied
    }

    /// `text` with the call on its first line, wholly covered, rewritten as `op` asks.
    fn the_call_rewritten(text: &str, op: &RefactorOp) -> std::result::Result<String, String> {
        let end = text.lines().next().map_or(1, |line| line.len() as u32 + 1);
        rewrite_call(
            text,
            op,
            Range {
                start: at(1, 1),
                end: at(1, end),
            },
        )
        .map(|edits| applied(text, edits))
        .map_err(|error| error.to_string())
    }

    #[test]
    fn a_new_argument_goes_in_without_disturbing_the_others() {
        // Given a call with a turbofish and a closure, whose commas are not separators
        let call = "f(g::<A, B>(1), |a, b| a, 3)\n";

        // When an argument is added first, and another last
        let first = an_op(RefactorKind::AddCallArg, |op| {
            op.variant = Some("first".to_string());
            op.expr = Some("0".to_string());
        });
        let last = an_op(RefactorKind::AddCallArg, |op| {
            op.variant = Some("last".to_string());
            op.expr = Some("9".to_string());
        });

        // Then each lands beside the right neighbour
        assert_eq!(
            (
                the_call_rewritten(call, &first),
                the_call_rewritten(call, &last)
            ),
            (
                Ok("f(0, g::<A, B>(1), |a, b| a, 3)\n".to_string()),
                Ok("f(g::<A, B>(1), |a, b| a, 3, 9)\n".to_string()),
            )
        );
    }

    #[test]
    fn removing_an_argument_takes_its_separator_with_it() {
        // Given a call of three arguments
        let call = "f(1, 2, 3)\n";
        let removing = |position: &str| {
            an_op(RefactorKind::RemoveCallArg, |op| {
                op.variant = Some(position.to_string())
            })
        };

        // When the first, the middle and the last are removed in turn
        let results =
            ["first", "2", "last"].map(|position| the_call_rewritten(call, &removing(position)));

        // Then none leaves a stray comma
        assert_eq!(
            results,
            [
                Ok("f(2, 3)\n".to_string()),
                Ok("f(1, 3)\n".to_string()),
                Ok("f(1, 2)\n".to_string()),
            ]
        );
    }

    #[test]
    fn a_range_that_is_not_one_call_is_refused() {
        // Given a range over a bare name
        let rewriting = an_op(RefactorKind::ChangeCallArg, |op| {
            op.variant = Some("first".to_string());
            op.expr = Some("1".to_string());
        });

        // When an argument is changed there
        let refusal = the_call_rewritten("basket\n", &rewriting);

        // Then it is refused as not a call
        assert_eq!(
            refusal.map_err(|reason| reason.contains("is not a call expression")),
            Err(true)
        );
    }

    #[test]
    fn a_return_type_is_replaced_up_to_the_where_clause() {
        // Given a function returning a generic type, with a where clause
        let text = "fn f<T>(a: T) -> Vec<T> where T: Copy {\n    vec![a]\n}\n";
        let retyping = an_op(RefactorKind::ChangeReturnType, |op| {
            op.type_ = Some("Option<T>".to_string())
        });

        // When its return type changes
        let rewritten =
            rewrite_declaration(text, &retyping, at(1, 4)).map(|edits| applied(text, edits));

        // Then only the `-> …` changes
        assert_eq!(
            rewritten.map_err(|error| error.to_string()),
            Ok("fn f<T>(a: T) -> Option<T> where T: Copy {\n    vec![a]\n}\n".to_string())
        );
    }

    #[test]
    fn a_parameter_is_added_after_the_receiver_and_by_position() {
        // Given a method with a receiver and one parameter
        let text = "fn f(&self, a: u8) {}\n";
        let adding = |position: &str| {
            an_op(RefactorKind::AddParam, |op| {
                op.name = Some("b".to_string());
                op.type_ = Some("u16".to_string());
                op.variant = Some(position.to_string());
            })
        };

        // When it is added first, and after `a`
        let results = ["first", "after:a"]
            .map(|position| rewrite_declaration(text, &adding(position), at(1, 4)))
            .map(|rewritten| {
                rewritten
                    .map(|edits| applied(text, edits))
                    .map_err(|error| error.to_string())
            });

        // Then `first` stays behind the receiver
        assert_eq!(
            results,
            [
                Ok("fn f(&self, b: u16, a: u8) {}\n".to_string()),
                Ok("fn f(&self, a: u8, b: u16) {}\n".to_string()),
            ]
        );
    }
}
