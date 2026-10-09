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
///
/// Read on the range's first line only, and never past the range's end. A range that is nothing
/// but such punctuation has no better position than its start, and keeps it.
pub(super) fn hover_bearing_position(text: &str, range: Range) -> Position {
    let Some(line) = line_chars(text, range.start.line) else {
        return range.start;
    };
    let limit = if range.end.line == range.start.line {
        (range.end.col as usize - 1).min(line.len())
    } else {
        line.len()
    };

    let mut at = range.start.col as usize - 1;
    let mut after_ampersand = false;
    while at < limit {
        match line[at] {
            '&' => after_ampersand = true,
            '*' | '!' | '-' | '(' => after_ampersand = false,
            character if character.is_whitespace() => {}
            _ if after_ampersand && word_at(&line, at, "mut") => {
                at += "mut".len();
                after_ampersand = false;
                continue;
            }
            _ => {
                return Position {
                    line: range.start.line,
                    col: at as u32 + 1,
                }
            }
        }
        at += 1;
    }
    range.start
}

/// `range` widened to the borrow around it when it selects a place (a field access or a path)
/// whose parent expression is `&<place>` or `&mut <place>` on the same line; otherwise `range`
/// unchanged.
///
/// A borrow on another line, or a range spanning lines, is not widened here; see
/// [`refuse_by_value_hoist`], which refuses what this leaves.
pub(super) fn widened_to_its_borrow(text: &str, range: Range) -> Range {
    match borrow_of_place(text, range) {
        Some(borrow) if borrow.line == range.start.line && range.end.line == range.start.line => {
            Range {
                start: borrow,
                end: range.end,
            }
        }
        _ => range,
    }
}

/// Refuse an `extract_variable` whose selection is a place directly under a borrow that the
/// selection does not include.
///
/// The assist binds what is selected by value, so `self.v` selected inside `&self.v` becomes
/// `let x = self.v;` — a move out of a borrow when the type is not `Copy`. This is decided from the
/// text alone and does not ask whether the type is `Copy`: there is no type information here, and a
/// by-value hoist of a borrowed place is never what the borrow meant. A `Copy` place is refused as
/// well, the cost of not guessing; selecting the borrow including its `&` is always accepted.
///
/// It is checked on the selection rather than on the assist's edit. The edit would be read for
/// `let <name> = <place>;` against the same two facts — a place, a unary `&` before it — so it
/// could only agree with this check, never catch what it misses.
pub(super) fn refuse_by_value_hoist(text: &str, range: Range) -> crate::Result<()> {
    match borrow_of_place(text, range) {
        None => Ok(()),
        Some(borrow) => Err(super::server_defect(format!(
            "extract_variable at {}:{} selects a place that is borrowed ({}:{}), and would bind it \
             by value, moving it out of the borrow. Select the borrow including its `&`.",
            range.start.line, range.start.col, borrow.line, borrow.col
        ))),
    }
}

/// Where the unary `&` (or `&mut`) is, when `range` selects a place that is its whole operand.
///
/// Read across lines. The `&` is a borrow, not the bit-and operator, when what precedes it cannot
/// end an expression. A place followed by a call, an index, a method or a `?` is the receiver of
/// that expression and not the operand of the borrow, so it has none.
fn borrow_of_place(text: &str, range: Range) -> Option<Position> {
    let chars: Vec<char> = text.chars().collect();
    let (from, to) = (offset_of(text, range.start)?, offset_of(text, range.end)?);
    if from >= to || to > chars.len() || !is_a_place_across_lines(&chars[from..to]) {
        return None;
    }
    let continues = chars[to..]
        .iter()
        .find(|character| !character.is_whitespace())
        .is_some_and(|character| matches!(character, '.' | '(' | '[' | '?' | ':'));
    if continues {
        return None;
    }

    let mut before = from;
    skip_whitespace_backwards(&chars, &mut before);
    if before >= 3
        && chars[before - 3..before].iter().collect::<String>() == "mut"
        && before
            .checked_sub(4)
            .is_none_or(|at| !is_identifier_character(chars[at]))
    {
        before -= 3;
        skip_whitespace_backwards(&chars, &mut before);
    }
    let borrow = before.checked_sub(1).filter(|at| chars[*at] == '&')?;

    let mut opener = borrow;
    skip_whitespace_backwards(&chars, &mut opener);
    let is_a_unary_borrow = opener
        .checked_sub(1)
        .is_none_or(|at| !ends_an_expression(chars[at]) && chars[at] != '&');
    is_a_unary_borrow.then(|| position_of(&chars, borrow))
}

/// The offset in characters of the one-based `position` in `text`.
fn offset_of(text: &str, position: Position) -> Option<usize> {
    let mut offset = 0;
    for (index, line) in text.split('\n').enumerate() {
        if index + 1 == position.line as usize {
            let column = position.col.checked_sub(1)? as usize;
            return (column <= line.chars().count()).then_some(offset + column);
        }
        offset += line.chars().count() + 1;
    }
    None
}

/// The one-based position of the character at `offset`.
fn position_of(chars: &[char], offset: usize) -> Position {
    let line_start = chars[..offset]
        .iter()
        .rposition(|character| *character == '\n')
        .map_or(0, |newline| newline + 1);
    Position {
        line: chars[..offset]
            .iter()
            .filter(|character| **character == '\n')
            .count() as u32
            + 1,
        col: (offset - line_start) as u32 + 1,
    }
}

/// [`is_a_place`], allowing the whitespace a path or field access is broken across lines with:
/// never between two identifier characters, which would make two words of one.
fn is_a_place_across_lines(selected: &[char]) -> bool {
    let words_are_whole = !selected.iter().enumerate().any(|(at, character)| {
        character.is_whitespace()
            && selected[..at]
                .iter()
                .rev()
                .find(|before| !before.is_whitespace())
                .is_some_and(|before| is_identifier_character(*before))
            && selected[at..]
                .iter()
                .find(|after| !after.is_whitespace())
                .is_some_and(|after| is_identifier_character(*after))
    });
    let compact: Vec<char> = selected
        .iter()
        .copied()
        .filter(|character| !character.is_whitespace())
        .collect();
    words_are_whole && is_a_place(&compact)
}

/// The characters of the one-based `line`.
fn line_chars(text: &str, line: u32) -> Option<Vec<char>> {
    let wanted = text.split('\n').nth(line.checked_sub(1)? as usize)?;
    Some(wanted.chars().collect())
}

/// Whether the word `word` starts at `at`, as a whole word.
fn word_at(line: &[char], at: usize, word: &str) -> bool {
    let end = at + word.len();
    line.get(at..end)
        .is_some_and(|found| found.iter().copied().eq(word.chars()))
        && line
            .get(end)
            .is_none_or(|next| !is_identifier_character(*next))
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Whether `selected` reads as a field access or a path: identifiers joined by `.` or `::`.
fn is_a_place(selected: &[char]) -> bool {
    selected
        .first()
        .is_some_and(|first| is_identifier_character(*first))
        && selected
            .iter()
            .all(|character| is_identifier_character(*character) || matches!(character, '.' | ':'))
}

/// Whether a character before a `&` makes that `&` a binary operator.
fn ends_an_expression(character: char) -> bool {
    is_identifier_character(character) || matches!(character, ')' | ']' | '}' | '?' | '"' | '\'')
}

fn skip_whitespace_backwards(line: &[char], at: &mut usize) {
    while *at > 0 && line[*at - 1].is_whitespace() {
        *at -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    const BORROWING: &str = "fn f(&self) -> bool {\n    let r = &self.v;\n    r.is_some()\n}\n";

    const BLOCK_AND_COMMENT: &str = "fn block(x: u32) -> u32 {\n    let mut y = x;\n    {\n        \
                                     // the step\n        let z = y + 1;\n        y = z * 2;\n    }\n    y\n}\n";

    #[test]
    fn a_range_opening_on_a_blocks_brace_is_probed_at_its_first_statement() {
        // The range opens on the bare block's `{` (3:5); the first typed token is `let`'s, at 5:9,
        // past the comment on line 4
        assert_eq!(
            hover_bearing_position(
                BLOCK_AND_COMMENT,
                Range {
                    start: at(3, 5),
                    end: at(7, 6)
                }
            ),
            at(5, 9)
        );
    }

    #[test]
    fn a_range_opening_on_a_line_comment_is_probed_at_the_code_after_it() {
        assert_eq!(
            hover_bearing_position(
                BLOCK_AND_COMMENT,
                Range {
                    start: at(4, 9),
                    end: at(6, 19)
                }
            ),
            at(5, 9)
        );
    }

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

    #[test]
    fn a_place_whose_borrow_is_on_the_previous_line_is_refused() {
        // Given `&` ending one line and the place opening the next
        let text = "fn f(&self) {\n    let r = &\n        self.v;\n}\n";
        let selected = Range {
            start: at(3, 9),
            end: at(3, 15),
        };

        // When
        let refusal = refuse_by_value_hoist(text, selected);

        // Then it is refused as unusable, naming the position and the remedy
        let said = refusal.unwrap_err().to_string();
        assert!(said.contains("unusable"), "{said}");
        assert!(said.contains("3:9"), "{said}");
        assert!(said.contains("including its `&`"), "{said}");
    }

    #[test]
    fn a_place_split_across_lines_under_a_borrow_is_refused() {
        // Given `&self` and `.v` on separate lines, selected whole
        let text = "fn f(&self) {\n    let r = &self\n        .v;\n}\n";
        let selected = Range {
            start: at(2, 14),
            end: at(3, 11),
        };

        // When
        let refusal = refuse_by_value_hoist(text, selected);

        // Then
        assert!(refusal.is_err());
    }

    #[test]
    fn a_place_under_a_mutable_borrow_on_another_line_is_refused() {
        let text = "fn f(&mut self) {\n    let r = &mut\n        self.v;\n}\n";
        let selected = Range {
            start: at(3, 9),
            end: at(3, 15),
        };

        assert!(refuse_by_value_hoist(text, selected).is_err());
    }

    #[test]
    fn a_selection_that_includes_its_borrow_is_accepted() {
        let selected = Range {
            start: at(2, 13),
            end: at(2, 20),
        };

        assert!(refuse_by_value_hoist(BORROWING, selected).is_ok());
    }

    #[test]
    fn a_place_not_under_a_borrow_is_accepted() {
        let text = "fn f(&self) -> usize {\n    let n = self.n;\n    n\n}\n";
        let selected = Range {
            start: at(2, 13),
            end: at(2, 19),
        };

        assert!(refuse_by_value_hoist(text, selected).is_ok());
    }

    #[test]
    fn a_receiver_under_a_borrow_of_the_whole_call_is_accepted() {
        // Given `&self.v.len()`: `self.v` is a receiver, the operand of the borrow is the call
        let text = "fn f(&self) {\n    let r = &self.v.len();\n}\n";
        let selected = Range {
            start: at(2, 14),
            end: at(2, 20),
        };

        assert!(refuse_by_value_hoist(text, selected).is_ok());
    }

    #[test]
    fn a_bit_and_is_not_read_as_a_borrow() {
        let text = "fn f(a: u32, b: u32) -> u32 {\n    let r = a &\n        b;\n    r\n}\n";
        let selected = Range {
            start: at(3, 9),
            end: at(3, 10),
        };

        assert!(refuse_by_value_hoist(text, selected).is_ok());
    }

    #[test]
    fn a_borrowed_place_on_one_line_is_widened_and_so_not_refused() {
        // Given the common case, which the widening answers
        let selected = Range {
            start: at(2, 14),
            end: at(2, 20),
        };

        // When
        let widened = widened_to_its_borrow(BORROWING, selected);

        // Then the widened range includes its borrow, so nothing is left to refuse
        assert!(refuse_by_value_hoist(BORROWING, widened).is_ok());
    }
}
