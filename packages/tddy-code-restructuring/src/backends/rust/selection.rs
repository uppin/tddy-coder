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
/// whose parent expression is `&<place>` or `&mut <place>`; otherwise `range` unchanged.
///
/// The `&` is a borrow, not the bit-and operator, when what precedes it cannot end an expression.
/// A place followed by a call, an index, a method or a `?` is the receiver of that expression and
/// not the operand of the borrow, so it is left as selected.
pub(super) fn widened_to_its_borrow(text: &str, range: Range) -> Range {
    let Some(line) = line_chars(text, range.start.line) else {
        return range;
    };
    if range.end.line != range.start.line {
        return range;
    }
    let (from, to) = (range.start.col as usize - 1, range.end.col as usize - 1);
    if from >= to || to > line.len() || !is_a_place(&line[from..to]) {
        return range;
    }
    let continues = line[to..]
        .iter()
        .find(|character| !character.is_whitespace())
        .is_some_and(|character| matches!(character, '.' | '(' | '[' | '?' | ':'));
    if continues {
        return range;
    }

    let mut before = from;
    skip_whitespace_backwards(&line, &mut before);
    if before >= 3 && line[before - 3..before].iter().collect::<String>() == "mut" {
        before -= 3;
        skip_whitespace_backwards(&line, &mut before);
    }
    let Some(borrow) = before.checked_sub(1).filter(|at| line[*at] == '&') else {
        return range;
    };

    let mut opener = borrow;
    skip_whitespace_backwards(&line, &mut opener);
    let is_a_unary_borrow = opener
        .checked_sub(1)
        .is_none_or(|at| !ends_an_expression(line[at]) && line[at] != '&');
    if !is_a_unary_borrow {
        return range;
    }

    Range {
        start: Position {
            line: range.start.line,
            col: borrow as u32 + 1,
        },
        end: range.end,
    }
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
