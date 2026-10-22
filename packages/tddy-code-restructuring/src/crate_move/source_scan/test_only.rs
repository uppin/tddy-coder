//! The test-only items of a Rust text — what the repository's production-line count leaves out.
//!
//! An item is test-only when one of its attributes is the `cfg(test)` marker every other `in_test`
//! decision of this scanner reads: `#[cfg(test)]` or `#[cfg(all(test, …))]`. `#[cfg(not(test))]`,
//! `#[cfg(any(test, …))]` and a `cfg_attr` build outside tests too, so they are not. Read over the
//! masked token stream, so a brace or a semicolon inside a string, a character or a comment never
//! ends an item (`#reshape` 17/24).

use super::{Kind, Scan};
use crate::crate_move::test_binary::{readable_spans, Prose};
use std::ops::Range;

/// The byte span of every test-only item in `text`, at any depth, in source order, nested spans
/// merged into the item that holds them.
///
/// A span runs from the start of the line holding the item's first outer doc comment or attribute
/// to the end of its last line, newline included: through the `;` of a declaration
/// (`mod x_tests;`, `use …;`) or the `}` that closes its first `{` (a function, an inline module,
/// an `impl`).
pub(crate) fn test_only_spans(text: &str) -> Vec<Range<usize>> {
    let scan = Scan::of(text);
    let doc_comments: Vec<Range<usize>> = readable_spans(text)
        .into_iter()
        .filter(|(span, prose)| *prose == Prose::Comment && is_outer_doc(&text[span.clone()]))
        .map(|(span, _)| span)
        .collect();

    let spans = (0..scan.tokens.len())
        .filter_map(|at| scan.cfg_test_attribute(at).map(|attribute| (at, attribute)))
        .map(|(at, attribute)| {
            let start = item_start(text, &scan, &doc_comments, at);
            let end = match item_end(&scan, after_attributes(&scan, at + attribute)) {
                Some(last) => scan.tokens[last].end,
                None => text.len(),
            };
            line_start(text, start)..line_end(text, end)
        })
        .collect();
    merged(spans)
}

/// Whether a comment is an outer doc comment: `///` but not `////`, or `/**` but not `/**/` or
/// `/***`. Inner doc comments (`//!`, `/*!`) document the enclosing item, not the one below.
fn is_outer_doc(comment: &str) -> bool {
    let line_doc = comment.starts_with("///") && !comment.starts_with("////");
    let block_doc =
        comment.starts_with("/**") && !matches!(comment.as_bytes().get(3), Some(b'*' | b'/'));
    line_doc || block_doc
}

/// Where the item whose `cfg(test)` attribute is token `at` starts: walking up, each outer doc
/// comment directly above and each outer attribute directly before belongs to the item.
fn item_start(text: &str, scan: &Scan, doc_comments: &[Range<usize>], at: usize) -> usize {
    let mut token = at;
    let mut head = scan.tokens[at].start;
    loop {
        if let Some(comment) = doc_comment_directly_above(text, doc_comments, head) {
            head = comment;
        } else if let Some(attribute) = attribute_ending_before(scan, token) {
            token = attribute;
            head = scan.tokens[attribute].start;
        } else {
            return head;
        }
    }
}

/// The start of the outer doc comment that sits directly above byte `head`: it ends on the line
/// above `head`'s line with nothing but whitespace after it, and no code precedes it on its line.
/// A blank line between them means the comment documents something else.
fn doc_comment_directly_above(
    text: &str,
    doc_comments: &[Range<usize>],
    head: usize,
) -> Option<usize> {
    let comment = doc_comments
        .iter()
        .rev()
        .find(|comment| comment.end <= head)?;
    let gap = &text[comment.end..head];
    let directly_above = gap.matches('\n').count() == 1 && gap.trim().is_empty();
    let alone_on_its_line = text[line_start(text, comment.start)..comment.start]
        .trim()
        .is_empty();
    (directly_above && alone_on_its_line).then_some(comment.start)
}

/// The index of the `#` that opens the outer attribute whose closing `]` is token `token - 1`.
fn attribute_ending_before(scan: &Scan, token: usize) -> Option<usize> {
    let close = token.checked_sub(1)?;
    if !scan.is_punct(close, b']') {
        return None;
    }
    let open = opening(scan, close, b'[', b']')?;
    let hash = open.checked_sub(1)?;
    scan.is_punct(hash, b'#').then_some(hash)
}

/// The index of the first token at or after `at` that does not open an outer attribute.
fn after_attributes(scan: &Scan, mut at: usize) -> usize {
    while scan.is_punct(at, b'#') && scan.is_punct(at + 1, b'[') {
        match closing(scan, at + 1, b'[', b']') {
            Some(close) => at = close + 1,
            None => break,
        }
    }
    at
}

/// The index of the token that ends the item whose first token is `from`: the first `;` outside
/// any brackets if no `{` comes before it, otherwise the `}` that closes the first `{`.
fn item_end(scan: &Scan, from: usize) -> Option<usize> {
    let mut nesting = 0usize;
    for (index, token) in scan.tokens.iter().enumerate().skip(from) {
        match token.kind {
            Kind::Punct(b'(' | b'[') => nesting += 1,
            Kind::Punct(b')' | b']') => nesting = nesting.saturating_sub(1),
            Kind::Punct(b';') if nesting == 0 => return Some(index),
            Kind::Punct(b'{') if nesting == 0 => return closing(scan, index, b'{', b'}'),
            _ => {}
        }
    }
    None
}

/// The index of the token that closes the `opener` at token `open`, counting only that kind.
fn closing(scan: &Scan, open: usize, opener: u8, closer: u8) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..scan.tokens.len() {
        if scan.is_punct(index, opener) {
            depth += 1;
        } else if scan.is_punct(index, closer) {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// The index of the `opener` that the `closer` at token `close` closes, counting only that kind.
fn opening(scan: &Scan, close: usize, opener: u8, closer: u8) -> Option<usize> {
    let mut depth = 0usize;
    for index in (0..=close).rev() {
        if scan.is_punct(index, closer) {
            depth += 1;
        } else if scan.is_punct(index, opener) {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

/// The spans in source order, with any span nested in another or touching it folded into it.
fn merged(mut spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(spans.len());
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}

/// The byte offset where the line holding byte `at` begins.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

/// The byte offset just past the newline that ends the line holding byte `at`, or the end of
/// `text` when that line is its last.
fn line_end(text: &str, at: usize) -> usize {
    text[at..]
        .find('\n')
        .map_or(text.len(), |newline| at + newline + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case the old count stopped at: an extracted test module declared above production code
    /// is one two-line item, and nothing after it is part of it.
    #[test]
    fn spans_an_out_of_line_test_module_declaration_from_its_attribute_to_its_semicolon() {
        // Given a declaration of an extracted test module between two production items
        let text = "fn a() {}\n#[cfg(test)]\nmod a_tests;\nfn b() {}\n";

        // When its test-only items are spanned
        let spans = test_only_spans(text);

        // Then the one span is exactly the attribute line and the declaration line
        assert_eq!(
            spans
                .iter()
                .map(|span| &text[span.clone()])
                .collect::<Vec<_>>(),
            ["#[cfg(test)]\nmod a_tests;\n"]
        );
    }
}
