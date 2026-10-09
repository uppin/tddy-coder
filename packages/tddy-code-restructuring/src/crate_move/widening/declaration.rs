//! Rule 5: reading and replacing the visibility in front of a declaration's name, by position.

use std::ops::Range;

use crate::crate_move::Result;
use crate::edit::Position;

/// The byte span of the visibility keyword in front of the name at `declared_at` (with the space
/// after it), and the visibility as written — `pub(crate)`, `pub(super)`, `pub(in a::b)`, `pub` — or
/// an empty span at the declaration's start and `""` for a private one. Attributes and
/// `async`/`const`/`unsafe` qualifiers on the same line are stepped over. `None` when nothing is
/// declared at `declared_at`.
///
/// # Errors
///
/// Refuses, as `SeamRefused`, a declaration whose keyword is on a line above its name: the
/// visibility cannot be found on the name's line, so it cannot be widened in place.
#[allow(
    dead_code,
    reason = "TODO(reshape-move-widen): called by `widened` in the green phase"
)]
pub(crate) fn visibility_span(
    text: &str,
    declared_at: Position,
) -> Result<Option<(Range<usize>, String)>> {
    // TODO(reshape-move-widen): implement
    let _ = (text, declared_at);
    todo!("the visibility in front of a declaration")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_restricted_visibility_and_its_span() {
        // Given a `pub(crate)` function whose name starts at column 15
        let text = "pub(crate) fn mint() {}\n";

        // When its visibility is read at the name
        let read = visibility_span(text, Position { line: 1, col: 15 });

        // Then the keyword and its trailing space are the span
        assert_eq!(
            read.expect("the declaration is read"),
            Some((0..11, "pub(crate)".to_string()))
        );
    }

    #[test]
    fn reads_a_private_field_as_an_empty_span_at_its_start() {
        // Given a private field, indented
        let text = "struct Roster {\n    secret: u64,\n}\n";

        // When its visibility is read at the name
        let read = visibility_span(text, Position { line: 2, col: 5 });

        // Then the span is empty, at the field's first character
        assert_eq!(
            read.expect("the field is read"),
            Some((20..20, String::new()))
        );
    }

    #[test]
    fn steps_over_an_attribute_and_a_qualifier_on_the_name_s_line() {
        // Given an attributed async method
        let text = "impl R {\n    #[inline] pub(super) async fn tick(&self) {}\n}\n";

        // When its visibility is read at the name
        let read = visibility_span(text, Position { line: 2, col: 35 });

        // Then the visibility after the attribute is the span
        assert_eq!(
            read.expect("the method is read"),
            Some((23..34, "pub(super)".to_string()))
        );
    }

    #[test]
    fn refuses_a_declaration_whose_keyword_is_on_the_line_above_its_name() {
        // Given a function written over two lines
        let text = "pub(crate) fn\nmint() {}\n";

        // When its visibility is read at the name
        let refusal = visibility_span(text, Position { line: 2, col: 1 });

        // Then it is refused, naming why
        assert!(refusal
            .expect_err("the keyword is not on the name's line")
            .to_string()
            .contains("has its keyword on a line above its name"));
    }
}
