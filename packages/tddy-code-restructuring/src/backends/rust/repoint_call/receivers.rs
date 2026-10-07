//! The bulk form's receiver walk: where the receiver of one method call begins and ends.

use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::super::seam_refusal;
use crate::crate_move::{readable_spans, Prose};
use crate::{RestructureError, Result};

/// The byte range of the receiver of the method call whose name token starts at `site` in `text`:
/// the maximal postfix chain ending at the `.` before the name, parentheses included, and nothing
/// that binds looser (`&x.m()` has the receiver `x`, `a + b.m()` has `b`).
///
/// The walk reads the *masked* code, so a delimiter written inside a string or a comment cannot
/// close a group early; a receiver that holds a comment is refused on the original text, and one
/// that is a block or a closure is refused because the hops cannot be inserted into it.
pub(super) fn receiver_span(text: &str, site: usize) -> Result<Range<usize>> {
    let masked = masked_to_code(text);
    let code = masked.as_bytes();
    let dot = dot_before(code, site)
        .ok_or_else(|| refuse("is not a method call: there is no `.` before the name"))?;
    let span = chain_start(code, dot)?..dot;
    refuse_a_comment(text, &span)?;
    refuse_a_block(text, &span)?;
    Ok(span)
}

/// The offset at which the hops of a template are inserted for the method call at `site`: the end
/// of its receiver.
pub(super) fn insertions_for(text: &str, site: usize, _hops: &str) -> Result<usize> {
    receiver_span(text, site).map(|receiver| receiver.end)
}

/// The offset of the `.` that separates the name at `site` from its receiver, whitespace around it
/// skipped.
fn dot_before(code: &[u8], site: usize) -> Option<usize> {
    let at = skip_whitespace_back(code, site);
    (at > 0 && code[at - 1] == b'.' && code.get(at.wrapping_sub(2)) != Some(&b'.')).then(|| at - 1)
}

/// The start of the maximal postfix chain ending at `end` (exclusive), refusing a chain that ends
/// in a block.
fn chain_start(code: &[u8], end: usize) -> Result<usize> {
    let mut pos = end;
    loop {
        let at = skip_whitespace_back(code, pos);
        if at == 0 {
            return Ok(pos);
        }
        match code[at - 1] {
            b')' => pos = matching_open(code, at - 1, b'(', b')')?,
            b']' => pos = matching_open(code, at - 1, b'[', b']')?,
            b'}' => return Err(refuse("ends in a block")),
            b'?' => pos = at - 1,
            byte if is_identifier(byte) => {
                let start = identifier_start(code, at);
                match qualifier_before(code, start) {
                    Some(before) => pos = before,
                    None => return Ok(start),
                }
            }
            byte if byte.is_ascii_digit() => {
                let start = digits_start(code, at);
                match qualifier_before(code, start) {
                    Some(before) => pos = before,
                    None => return Ok(start),
                }
            }
            _ => return Ok(pos),
        }
    }
}

/// The offset a chain continues from when the token starting at `start` is a member reached through
/// `.` or a path step reached through `::`, or `None` when it is the chain's own base.
fn qualifier_before(code: &[u8], start: usize) -> Option<usize> {
    let before = skip_whitespace_back(code, start);
    if before > 0 && code[before - 1] == b'.' && code.get(before.wrapping_sub(2)) != Some(&b'.') {
        return Some(before - 1);
    }
    if before >= 2 && code[before - 1] == b':' && code[before - 2] == b':' {
        return Some(before - 2);
    }
    None
}

/// The `(` that the `)` at `close` closes, or the `[` that the `]` does.
fn matching_open(code: &[u8], close: usize, open: u8, shut: u8) -> Result<usize> {
    let mut depth = 0usize;
    for at in (0..close).rev() {
        if code[at] == shut {
            depth += 1;
        } else if code[at] == open {
            if depth == 0 {
                return Ok(at);
            }
            depth -= 1;
        }
    }
    Err(refuse("has an unclosed bracket, so it cannot be read"))
}

/// Refuse a receiver that holds a comment: the comment would sit between the receiver and the hops.
fn refuse_a_comment(text: &str, span: &Range<usize>) -> Result<()> {
    let holds = readable_spans(text).into_iter().any(|(prose, kind)| {
        kind == Prose::Comment && prose.start < span.end && span.start < prose.end
    });
    if holds {
        return Err(refuse(
            "holds a comment, which the hops cannot be inserted across",
        ));
    }
    Ok(())
}

/// Refuse a receiver that is a block or a closure: there is no single expression to insert after.
fn refuse_a_block(text: &str, span: &Range<usize>) -> Result<()> {
    let source = &text[span.clone()];
    let parsed = syn::parse_str::<syn::Expr>(source)
        .map_err(|_| refuse("is not one expression, so the hops cannot be inserted after it"))?;
    let mut blocks = Blocks::default();
    syn::visit::Visit::visit_expr(&mut blocks, &parsed);
    if blocks.found {
        return Err(refuse(
            "is a block or a closure, so the hops cannot be inserted after it",
        ));
    }
    Ok(())
}

/// Whether the expression visited holds a block, a closure or another block-like body.
#[derive(Default)]
struct Blocks {
    found: bool,
}

impl<'ast> syn::visit::Visit<'ast> for Blocks {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        use syn::Expr::*;
        if matches!(
            expr,
            Block(_) | Unsafe(_) | Const(_) | Async(_) | TryBlock(_) | Closure(_)
        ) {
            self.found = true;
            return;
        }
        syn::visit::visit_expr(self, expr);
    }
}

/// The receiver refusal, in the words every caller of the walk shares.
fn refuse(why: &str) -> RestructureError {
    seam_refusal(format!(
        "the receiver of this call {why}: it must be one postfix chain of paths, fields, calls, \
         indexes, `?` and `.await`"
    ))
}

/// The largest offset at or before `at` with nothing but whitespace between it and `at`.
fn skip_whitespace_back(code: &[u8], at: usize) -> usize {
    let mut start = at;
    while start > 0 && code[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    start
}

/// The start of the identifier ending at `at` (exclusive).
fn identifier_start(code: &[u8], at: usize) -> usize {
    let mut start = at;
    while start > 0 && is_identifier(code[start - 1]) {
        start -= 1;
    }
    start
}

/// The start of the run of digits ending at `at` (exclusive).
fn digits_start(code: &[u8], at: usize) -> usize {
    let mut start = at;
    while start > 0 && code[start - 1].is_ascii_digit() {
        start -= 1;
    }
    start
}

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The receiver of the call of `m` in `statement`, as text.
    fn the_receiver_of_m_in(statement: &str) -> String {
        let site = statement.find("m(").expect("the statement calls `m`");
        let span = receiver_span(statement, site).expect("the receiver is read");
        statement[span].to_string()
    }

    /// The refusal reading the receiver of the call of `m` in `statement` gives.
    fn the_refusal_for_the_receiver_of_m_in(statement: &str) -> String {
        let site = statement.find("m(").expect("the statement calls `m`");
        insertions_for(statement, site, ".peer")
            .expect_err("the receiver is refused")
            .to_string()
    }

    #[test]
    fn the_receiver_walk_stops_at_the_operator_that_binds_looser() {
        // Given calls of `m` preceded by an operator that binds looser, or by a postfix that belongs
        let receivers = [
            the_receiver_of_m_in("let y = &x.m(1);"),
            the_receiver_of_m_in("let y = -x.m(1);"),
            the_receiver_of_m_in("let y = a + b.m(1);"),
            the_receiver_of_m_in("let y = x?.m(1);"),
            the_receiver_of_m_in("let y = fut.await.m(1);"),
            the_receiver_of_m_in("let y = v[0].m(1);"),
            the_receiver_of_m_in("let y = (a + b).m(1);"),
        ];

        // Then the receiver stops at the operator, and keeps `?`, `.await`, indexing and parentheses
        assert_eq!(
            receivers,
            ["x", "x", "b", "x?", "fut.await", "v[0]", "(a + b)"]
        );
    }

    #[test]
    fn a_receiver_ending_in_a_block_or_holding_a_comment_is_refused() {
        // Given calls of `m` on a block, on a closure's block body, and on a receiver holding a comment
        let refusals = [
            the_refusal_for_the_receiver_of_m_in("let y = unsafe { x }.m(1);"),
            the_refusal_for_the_receiver_of_m_in("let y = (|| { x })().m(1);"),
            the_refusal_for_the_receiver_of_m_in("let y = x /* why */ .m(1);"),
        ];

        // Then each is refused for its receiver, not for the operation being unimplemented
        assert!(
            refusals.iter().all(|refusal| refusal.contains("receiver")),
            "{refusals:?}"
        );
    }
}
