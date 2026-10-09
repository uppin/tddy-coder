//! The test-only items of a Rust text — what the repository's production-line count leaves out.
//!
//! An item is test-only when one of its attributes is the `cfg(test)` marker every other `in_test`
//! decision of this scanner reads: `#[cfg(test)]` or `#[cfg(all(test, …))]`. `#[cfg(not(test))]`,
//! `#[cfg(any(test, …))]` and a `cfg_attr` build outside tests too, so they are not. Read over the
//! masked token stream, so a brace or a semicolon inside a string, a character or a comment never
//! ends an item (`#reshape` 15/19).

use std::ops::Range;

/// The byte span of every test-only item in `text`, at any depth, in source order, nested spans
/// merged into the item that holds them.
///
/// A span runs from the start of the line holding the item's first outer doc comment or attribute
/// to the end of its last line, newline included: through the `;` of a declaration
/// (`mod x_tests;`, `use …;`) or the `}` that closes its first `{` (a function, an inline module,
/// an `impl`).
#[allow(
    dead_code,
    reason = "TODO(reshape-oversized-files): implement — `runner::budget::production_lines` reads the spans"
)]
pub(crate) fn test_only_spans(text: &str) -> Vec<Range<usize>> {
    // TODO(reshape-oversized-files): implement
    let _ = text;
    todo!("test_only_spans: mask, tokenise, and span every cfg(test)-marked item")
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
