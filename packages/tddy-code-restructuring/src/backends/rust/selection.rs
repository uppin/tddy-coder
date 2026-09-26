//! Where an extraction asks rust-analyzer about its range — and what range it asks about.
//!
//! Two defects came from asking about the range exactly as written. `extract_variable`'s type
//! probe hovered on the range's first character, and on `&` the hover stays `null` for ever, so a
//! range opening with a borrow waited without end and wedged the warm daemon behind it. And a
//! selection of `self.v` whose parent is `&self.v` was hoisted by value — `let x = self.v;`, a
//! move out of `&self` — where the borrow was the expression that meant something.

use crate::edit::{Position, Range};

/// The first position of `range` that can carry a hover: past leading `&`, `&mut`, `*`, `!`, `-`
/// and `(`, and past whitespace after them.
#[allow(dead_code)] // TODO(extraction-defects): the type probe asks here instead of at `range.start`.
pub(super) fn hover_bearing_position(text: &str, range: Range) -> Position {
    // TODO(extraction-defects): implement
    let _ = (text, range);
    todo!("extraction-defects: the first hover-bearing position of a range")
}

/// `range` widened to the borrow around it when it selects a place (a field access or a path)
/// whose parent expression is `&<place>` or `&mut <place>`; otherwise `range` unchanged.
#[allow(dead_code)] // TODO(extraction-defects): `extract_variable` asks for the widened range.
pub(super) fn widened_to_its_borrow(text: &str, range: Range) -> Range {
    // TODO(extraction-defects): implement
    let _ = (text, range);
    todo!("extraction-defects: widen a borrowed place to its borrow")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    const BORROWING: &str = "fn f(&self) -> bool {\n    let r = &self.v;\n    r.is_some()\n}\n";

    #[test]
    fn a_range_opening_with_a_borrow_is_probed_past_the_ampersand() {
        // `&self.v` spans 2:13–2:20; the hover-bearing token is `self`, at 2:14
        assert_eq!(
            hover_bearing_position(
                BORROWING,
                Range {
                    start: at(2, 13),
                    end: at(2, 20)
                }
            ),
            at(2, 14)
        );
    }

    #[test]
    fn a_range_opening_with_a_mutable_borrow_is_probed_past_the_mut() {
        let text = "fn f(&mut self) {\n    let r = &mut self.v;\n}\n";

        assert_eq!(
            hover_bearing_position(
                text,
                Range {
                    start: at(2, 13),
                    end: at(2, 24)
                }
            ),
            at(2, 18)
        );
    }

    #[test]
    fn a_range_opening_on_an_identifier_is_probed_where_it_starts() {
        assert_eq!(
            hover_bearing_position(
                BORROWING,
                Range {
                    start: at(3, 5),
                    end: at(3, 16)
                }
            ),
            at(3, 5)
        );
    }

    #[test]
    fn a_borrowed_field_read_is_widened_to_its_borrow() {
        // Selecting `self.v` (2:14–2:20) inside `&self.v` selects the borrow (2:13–2:20)
        assert_eq!(
            widened_to_its_borrow(
                BORROWING,
                Range {
                    start: at(2, 14),
                    end: at(2, 20)
                }
            ),
            Range {
                start: at(2, 13),
                end: at(2, 20)
            }
        );
    }

    #[test]
    fn a_field_read_not_under_a_borrow_is_left_as_selected() {
        let text = "fn f(&self) -> usize {\n    let n = self.n;\n    n\n}\n";
        let selected = Range {
            start: at(2, 13),
            end: at(2, 19),
        };

        assert_eq!(widened_to_its_borrow(text, selected), selected);
    }
}
