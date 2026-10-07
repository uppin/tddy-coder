//! The single form: the callee of one call replaced, the arguments untouched.

use super::super::seam_refusal;
use super::super::signature_rewrites::{call_in, edits_of, Call, Replacement, Span};
use crate::edit::{Range, TextEdit};
use crate::Result;

/// The edits that replace everything before the argument list of the one call `range` covers in
/// `text` with `callee`.
///
/// The range must be exactly one call — [`call_in`] refuses anything else. What is replaced is the
/// callee alone: the arguments and the parentheses around them are kept byte for byte, and a
/// comment between the callee and the `(` is not this operation's to move.
pub(super) fn rewrite_callee(text: &str, range: Range, callee: &str) -> Result<Vec<TextEdit>> {
    let call = call_in(text, range)?;
    let current = text[call.from..call.open].trim_end();
    refuse(&call, text, current, callee)?;
    Ok(edits_of(
        text,
        vec![Replacement {
            span: Span {
                from: call.from,
                to: call.open,
            },
            with: callee.trim().to_string(),
        }],
    ))
}

/// The refusals the single form makes about the call and the callee, before any edit is built.
fn refuse(call: &Call, text: &str, current: &str, callee: &str) -> Result<()> {
    if current.trim() == callee.trim() {
        return Err(seam_refusal(format!(
            "`{current}` is the callee the call already writes: re-pointing a call to itself \
             re-points nothing"
        )));
    }

    // An original method call with a turbofish cannot be restated by a field or method chain — there
    // is nowhere in `self.peer.m` to write `::<T>`. A path callee may carry its own.
    let whole = &text[call.from..call.to];
    if let Ok(syn::Expr::MethodCall(method)) = syn::parse_str::<syn::Expr>(whole) {
        if method.turbofish.is_some() {
            let carries_its_own = matches!(
                syn::parse_str::<syn::Expr>(callee.trim()),
                Ok(syn::Expr::Path(_))
            );
            if !carries_its_own {
                let turbofish = current.find("::<").map_or("::<…>", |at| &current[at..]);
                return Err(seam_refusal(format!(
                    "the call `{whole}` carries a turbofish `{turbofish}`, which a field or method \
                     chain cannot restate: use a path callee that carries its own turbofish"
                )));
            }
        }
    }

    // The old callee must hold no call: `a.m(x).n` replaced by `b.n` would drop `m(x)`'s arguments
    // silently, so the inner call is re-pointed on its own range first.
    if let Some(inner) = the_call_the_callee_holds(current) {
        return Err(seam_refusal(format!(
            "the callee `{current}` holds a call, `{inner}`, whose arguments would be dropped \
             silently: re-point the inner call on its own range first"
        )));
    }

    Ok(())
}

/// The call the callee `current` holds, or `None` when it is only paths, fields, indexes and
/// postfixes. The call is the receiver of the callee's own outermost member.
fn the_call_the_callee_holds(current: &str) -> Option<String> {
    let parsed = syn::parse_str::<syn::Expr>(current).ok()?;
    match &parsed {
        syn::Expr::Call(_) | syn::Expr::MethodCall(_) => Some(current.trim().to_string()),
        syn::Expr::Field(field) => match &*field.base {
            syn::Expr::Call(_) | syn::Expr::MethodCall(_) => {
                let dot = last_dot_outside_brackets(current)?;
                Some(current[..dot].trim().to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

/// The offset of the last `.` of `text` that is not inside a bracket — the one that separates the
/// outermost member of a field or method chain.
fn last_dot_outside_brackets(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut found = None;
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'.' if depth == 0 => found = Some(at),
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;
    use pretty_assertions::assert_eq;

    /// The range over the whole of `text`, one-based, the way a plan anchors a call.
    fn over_the_whole_of(text: &str) -> Range {
        let last_line = text.lines().last().unwrap_or_default();
        Range {
            start: Position { line: 1, col: 1 },
            end: Position {
                line: text.lines().count() as u32,
                col: last_line.len() as u32 + 1,
            },
        }
    }

    /// `text` with the edits applied, last first, so earlier offsets stay valid.
    fn applied(text: &str, edits: &[TextEdit]) -> String {
        let mut result = text.to_string();
        let mut ordered: Vec<&TextEdit> = edits.iter().collect();
        ordered.sort_by_key(|edit| (edit.range.start.line, edit.range.start.col));
        for edit in ordered.into_iter().rev() {
            let offset = |at: Position| {
                let before: usize = result
                    .split_inclusive('\n')
                    .take(at.line as usize - 1)
                    .map(str::len)
                    .sum();
                before + at.col as usize - 1
            };
            let (from, to) = (offset(edit.range.start), offset(edit.range.end));
            result.replace_range(from..to, &edit.new_text);
        }
        result
    }

    /// The call `text` is, after re-pointing it to `callee`.
    fn re_pointed(text: &str, callee: &str) -> String {
        let edits = rewrite_callee(text, over_the_whole_of(text), callee)
            .expect("the call can be re-pointed");
        applied(text, &edits)
    }

    /// The refusal re-pointing the call `text` is to `callee` gives.
    fn the_refusal_re_pointing(text: &str, callee: &str) -> String {
        rewrite_callee(text, over_the_whole_of(text), callee)
            .expect_err("the call cannot be re-pointed")
            .to_string()
    }

    #[test]
    fn single_replaces_only_what_precedes_the_argument_list() {
        // Given a call whose arguments hold nested calls, delimiters in strings and a comment
        let call = "self.slot(\n    a, // , )\n    f(\",)\"),\n    g(/* ( */ 1),\n    \")\",\n)";

        // When it is re-pointed through a field
        let after = re_pointed(call, "self.peer.slot");

        // Then everything from the opening parenthesis on is byte-identical
        assert_eq!(
            after,
            "self.peer.slot(\n    a, // , )\n    f(\",)\"),\n    g(/* ( */ 1),\n    \")\",\n)"
        );
    }

    #[test]
    fn single_turns_a_method_call_into_a_field_chain_call_and_keeps_the_receiver_only_if_the_callee_names_it(
    ) {
        // Given a method call
        let call = "self.dir_for(id)";

        // When it is re-pointed to a field chain that names the receiver, and to a path that does not
        let through_a_field = re_pointed(call, "self.peer.dir_for");
        let through_a_path = re_pointed(call, "lookup::dir_for");

        // Then the receiver survives in the first and is dropped in the second, the arguments in both
        assert_eq!(
            (through_a_field, through_a_path),
            (
                "self.peer.dir_for(id)".to_string(),
                "lookup::dir_for(id)".to_string()
            )
        );
    }

    #[test]
    fn single_refuses_an_old_callee_holding_a_call_a_turbofish_the_callee_cannot_restate_and_a_noop(
    ) {
        // Given three calls whose re-pointing would lose an argument, lose a turbofish, or change nothing
        let holding_a_call = "a.m(x).n(y)";
        let with_a_turbofish = "recv.m::<T>(x)";
        let already_there = "self.slot(x)";

        // When each is re-pointed
        let refusals = [
            the_refusal_re_pointing(holding_a_call, "b.n"),
            the_refusal_re_pointing(with_a_turbofish, "self.peer.m"),
            the_refusal_re_pointing(already_there, "self.slot"),
        ];

        // Then each is refused naming what is wrong with it
        assert!(refusals[0].contains("m(x)"), "{}", refusals[0]);
        assert!(refusals[1].contains("::<T>"), "{}", refusals[1]);
        assert!(refusals[2].contains("self.slot"), "{}", refusals[2]);
    }
}
