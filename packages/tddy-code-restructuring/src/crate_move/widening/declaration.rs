//! Rule 5: reading and replacing the visibility in front of a declaration's name, by position.

use std::ops::Range;

use crate::apply::byte_offset;
use crate::crate_move::manifest_edits::split_visibility;
use crate::crate_move::source_scan::masked_code;
use crate::crate_move::Result;
use crate::edit::Position;
use crate::RestructureError;

/// The visibility of a declaration: the byte span of the keyword (with the space after it) and the
/// keyword as written, `""` for a private one, whose span is empty.
pub(crate) type Visibility = (Range<usize>, String);

/// A declaration whose visibility keyword is on a line above its name, which is named here.
pub(crate) struct KeywordAbove {
    pub(crate) name: String,
}

impl KeywordAbove {
    /// The refusal, for a declaration in `file` (or in no file the caller can name, for `None`).
    pub(crate) fn refusal(&self, file: Option<&str>) -> RestructureError {
        let whose = match file {
            Some(file) => format!("`{}` in `{file}`", self.name),
            None => format!("`{}`", self.name),
        };
        RestructureError::SeamRefused(format!(
            "the declaration of {whose} has its keyword on a line above its name, so the move \
             cannot widen it: write the keyword and the name on one line"
        ))
    }
}

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
///
/// Only the unit tests read it without a file to name: the move reads [`visibility_in_file`].
#[cfg(test)]
pub(crate) fn visibility_span(text: &str, declared_at: Position) -> Result<Option<Visibility>> {
    read_visibility(text, declared_at).map_err(|above| above.refusal(None))
}

/// [`visibility_span`], with the refusal naming the `file` the declaration is in.
///
/// # Errors
///
/// The refusal [`visibility_span`] makes.
pub(crate) fn visibility_in_file(
    file: &str,
    text: &str,
    declared_at: Position,
) -> Result<Option<Visibility>> {
    read_visibility(text, declared_at).map_err(|above| above.refusal(Some(file)))
}

fn read_visibility(
    text: &str,
    declared_at: Position,
) -> std::result::Result<Option<Visibility>, KeywordAbove> {
    let Ok(name_at) = byte_offset(text, declared_at.line, declared_at.col) else {
        return Ok(None);
    };
    let code = masked_code(text);
    let Some(name) = code.get(name_at..).map(identifier_at_start) else {
        return Ok(None);
    };
    if name.is_empty() {
        return Ok(None);
    }

    let line_start = code[..name_at].rfind('\n').map_or(0, |at| at + 1);
    let from = after_attributes(&code, line_start, name_at);
    let prefix = &code[from..name_at];
    if prefix.is_empty() {
        return match keyword_above(&code[..line_start]) {
            true => Err(KeywordAbove {
                name: name.to_string(),
            }),
            false => Ok(Some((name_at..name_at, String::new()))),
        };
    }

    let (written, _) = split_visibility(prefix);
    let spaces = prefix[written.len()..].len() - prefix[written.len()..].trim_start().len();
    let end = if written.is_empty() {
        from
    } else {
        from + written.len() + spaces
    };
    Ok(Some((from..end, written.to_string())))
}

fn identifier_at_start(rest: &str) -> &str {
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Where the declaration's own text starts on the line `from..to` of `code`: past the indentation
/// and any `#[…]` attribute written ahead of it.
fn after_attributes(code: &str, from: usize, to: usize) -> usize {
    let mut at = from;
    loop {
        at += code[at..to].len() - code[at..to].trim_start().len();
        if !code[at..to].starts_with("#[") {
            return at;
        }
        let mut depth = 0usize;
        let close = code[at..to].char_indices().find_map(|(index, c)| {
            match c {
                '[' => depth += 1,
                ']' => depth = depth.saturating_sub(1),
                _ => {}
            }
            (c == ']' && depth == 0).then_some(index)
        });
        match close {
            Some(index) => at += index + 1,
            None => return at,
        }
    }
}

/// Whether the code ahead of a name's line ends in the middle of a declaration — a keyword or a
/// visibility restriction — rather than at the end of the one before it.
fn keyword_above(before: &str) -> bool {
    before
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(|last| last.is_alphanumeric() || last == '_' || last == ')')
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
