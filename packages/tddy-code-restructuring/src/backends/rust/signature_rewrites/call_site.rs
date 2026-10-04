use super::entries;
use crate::Result;

use super::super::early_return::masked_to_code;

use super::super::seam_refusal;

use super::edits_of;

use super::reordered;

use crate::{backends::rust::signature::offset_at, plan::rust_syntax::permutation};

use crate::plan::OrderKey;

use super::replacing;

use super::with_an_entry_at;

use crate::plan::RefactorKind;

use super::required;

use crate::edit::TextEdit;

use crate::edit::Range;

use crate::plan::RefactorOp;

use super::Span;

/// The call expression `range` covers, with the arguments it passes.
struct Call {
    /// The offset of the `(` opening the argument list.
    pub(crate) open: usize,
    pub(crate) arguments: Vec<Span>,
}

/// The edits that rewrite the call `range` covers in `text` as `op` asks.
///
/// The range must be exactly one call expression — `callee(arguments)` or
/// `receiver.method(arguments)` — and is refused otherwise, so an operation can never land on the
/// arguments of a call nested somewhere inside it.
pub(in super::super) fn rewrite_call(
    text: &str,
    op: &RefactorOp,
    range: Range,
) -> Result<Vec<TextEdit>> {
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
        other => Err(super::super::failure(format!(
            "{other:?} does not edit a call"
        ))),
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
                super::super::failure(format!(
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
        return Err(super::super::server_defect(format!(
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
