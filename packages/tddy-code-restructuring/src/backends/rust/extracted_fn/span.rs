//! Where the function an extraction introduced sits in the file.

/// Byte offsets of a function's `fn` keyword, the `{` that opens its body and the `}` that closes
/// it, read over text with comments and literals masked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FnSpan {
    pub keyword: usize,
    pub open: usize,
    pub close: usize,
}

/// The span of the one `fn <name>` declared in `text`, or `None` when there is none.
pub(super) fn function_span(text: &str, name: &str) -> Option<FnSpan> {
    let _ = (text, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("function_span")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_FUNCTIONS: &str = "fn caller() -> u32 {\n    kept(1)\n}\n\n\
                                 fn kept(x: u32) -> u32 {\n    // `fn kept` in a comment\n    x + 1\n}\n";

    #[test]
    fn spans_the_named_function_from_its_keyword_to_its_closing_brace() {
        let span = function_span(TWO_FUNCTIONS, "kept").expect("`kept` is declared");

        assert_eq!(
            &TWO_FUNCTIONS[span.keyword..=span.close],
            "fn kept(x: u32) -> u32 {\n    // `fn kept` in a comment\n    x + 1\n}"
        );
        assert_eq!(&TWO_FUNCTIONS[span.open..span.open + 1], "{");
    }

    #[test]
    fn a_name_only_a_comment_or_a_call_mentions_has_no_span() {
        assert_eq!(function_span(TWO_FUNCTIONS, "missing"), None);
        assert_eq!(
            function_span("fn a() {\n    // fn b() {}\n    b();\n}\n", "b"),
            None
        );
    }
}
