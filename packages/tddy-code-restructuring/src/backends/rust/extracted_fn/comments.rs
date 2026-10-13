//! Rule 1: the comments rust-analyzer drops from a range that holds a `?`.
//!
//! The assist rebuilds the extracted body's statement list to wrap its outputs in `Ok(…)` and
//! loses the trivia between statements and after them. A comment inside a statement survives. The
//! statements keep their order and the assist appends one tail, so each lost comment is put back
//! beside the statement it annotated — or the operation is refused, naming every comment it would
//! lose.

use super::span::{function_span, statements, Statement};
use crate::backends::rust::early_return::{byte_offset, masked_to_code};
use crate::backends::rust::server_defect;
use crate::crate_move::{readable_spans, Prose};
use crate::edit::Range;
use crate::Result;

/// A comment of the range that the produced function lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Comment {
    /// The comment as written, trimmed (`// why`, `/* … */`).
    pub text: String,
    /// Its one-based line in the original file.
    pub line: u32,
    /// Whether it trails code on its line rather than standing on lines of its own.
    pub trailing: bool,
    /// The bytes it occupies in the original file.
    start: usize,
    end: usize,
}

/// The comments of `range` in `original` that the function `name` in `produced` does not hold,
/// counted as a multiset of trimmed comment text.
pub(super) fn dropped(original: &str, range: Range, produced: &str, name: &str) -> Vec<Comment> {
    let Some((from, to)) = offsets(original, range) else {
        return Vec::new();
    };
    let function = function_span(produced, name)
        .map_or("", |span| &produced[span.keyword..=span.close]);
    let mut kept: Vec<String> = comments_in(function, 0, function.len())
        .into_iter()
        .map(|comment| comment.text)
        .collect();
    comments_in(original, from, to)
        .into_iter()
        .filter(|comment| match kept.iter().position(|text| *text == comment.text) {
            Some(at) => {
                kept.swap_remove(at);
                false
            }
            None => true,
        })
        .collect()
}

/// `produced` with every dropped comment put back beside the statement it annotated, and how many
/// were put back. Statements that cannot be paired refuse the operation (`ServerDefect`), naming
/// the comments that would be lost.
pub(super) fn restored(
    original: &str,
    range: Range,
    produced: &str,
    name: &str,
) -> Result<(String, usize)> {
    let lost = dropped(original, range, produced, name);
    if lost.is_empty() {
        return Ok((produced.to_string(), 0));
    }
    let Some(insertions) = insertions(original, range, produced, name, &lost) else {
        let named: Vec<String> = lost
            .iter()
            .map(|comment| format!("`{}`", comment.text))
            .collect();
        return Err(server_defect(format!(
            "rust-analyzer dropped {} comment(s) from the extracted function, and its statements \
             cannot be paired with the range's to put them back: {}",
            lost.len(),
            named.join(", ")
        )));
    };
    Ok((with_insertions(produced, insertions), lost.len()))
}

/// How many comments of `range` rust-analyzer will drop: those between or after the range's
/// top-level statements, when the range holds a `?`. What `check` reports on its progress line.
pub(super) fn at_risk(text: &str, range: Range) -> usize {
    let Some((from, to)) = offsets(text, range) else {
        return 0;
    };
    let masked = masked_to_code(text);
    let region = &masked.as_bytes()[from..to];
    if !region.contains(&b'?') {
        return 0;
    }
    let statements = statements(region, from);
    comments_in(text, from, to)
        .iter()
        .filter(|comment| !inside_a_statement(&statements, comment))
        .count()
}

fn offsets(text: &str, range: Range) -> Option<(usize, usize)> {
    Some((byte_offset(text, range.start)?, byte_offset(text, range.end)?))
}

/// Every comment lying wholly within bytes `from..to` of `text`, in order.
fn comments_in(text: &str, from: usize, to: usize) -> Vec<Comment> {
    readable_spans(text)
        .into_iter()
        .filter(|(span, prose)| *prose == Prose::Comment && from <= span.start && span.end <= to)
        .map(|(span, _)| {
            let line_start = text[..span.start].rfind('\n').map_or(0, |at| at + 1);
            Comment {
                text: text[span.clone()].trim().to_string(),
                line: text[..span.start].matches('\n').count() as u32 + 1,
                trailing: !text[line_start..span.start].trim().is_empty(),
                start: span.start,
                end: span.end,
            }
        })
        .collect()
}

fn inside_a_statement(statements: &[Statement], comment: &Comment) -> bool {
    statements
        .iter()
        .any(|statement| statement.start <= comment.start && comment.start < statement.end)
}

/// The text each dropped comment is written back with, and the byte offset of `produced` it goes
/// at: before the line of the produced statement a full-line comment preceded, at the end of the
/// line of the one a trailing comment trailed. `None` when the statements cannot be paired.
fn insertions(
    original: &str,
    range: Range,
    produced: &str,
    name: &str,
    lost: &[Comment],
) -> Option<Vec<(usize, String)>> {
    let (from, to) = offsets(original, range)?;
    let span = function_span(produced, name)?;
    let (masked, masked_produced) = (masked_to_code(original), masked_to_code(produced));
    let paired = statements(&masked.as_bytes()[from..to], from);
    let body = statements(
        &masked_produced.as_bytes()[span.open + 1..span.close],
        span.open + 1,
    );
    // The assist's tail wraps the outputs, and is the one statement the range did not hold.
    let tail = body.last().is_some_and(|last| !last.terminated)
        && paired.last().is_some_and(|last| last.terminated);
    if body.len() != paired.len() + usize::from(tail) {
        return None;
    }

    let mut put = Vec::new();
    for comment in lost {
        if inside_a_statement(&paired, comment) {
            return None;
        }
        let text = &comment.text;
        if comment.trailing {
            let at = paired.iter().rposition(|statement| statement.end <= comment.start)?;
            put.push((line_end(produced, body[at].end - 1), format!(" {text}")));
        } else if let Some(at) = paired.iter().position(|statement| statement.start >= comment.end) {
            let line = line_start(produced, body[at].start);
            put.push((line, format!("{}{text}\n", indentation(produced, line))));
        } else if !tail {
            let last = body.last()?;
            let indent = indentation(produced, line_start(produced, last.start));
            put.push((line_end(produced, last.end - 1), format!("\n{indent}{text}")));
        } else {
            return None;
        }
    }
    Some(put)
}

fn with_insertions(text: &str, mut insertions: Vec<(usize, String)>) -> String {
    insertions.sort_by_key(|(at, _)| *at);
    let (mut written, mut cursor) = (String::new(), 0);
    for (at, inserted) in insertions {
        written.push_str(&text[cursor..at]);
        written.push_str(&inserted);
        cursor = at;
    }
    written.push_str(&text[cursor..]);
    written
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

/// The whitespace the line starting at `line` is indented with.
fn indentation(text: &str, line: usize) -> &str {
    let width = text[line..]
        .bytes()
        .take_while(|byte| *byte == b' ' || *byte == b'\t')
        .count();
    &text[line..line + width]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;

    /// `start`'s first four statements and the comments between them, as the 09b plan cut them.
    const ORIGIN: &str = "impl Svc {\n    pub async fn start(&self, agents: &[String], dir: PathBuf) -> Result<usize, String> {\n        \
let mut started = self.seeded(agents).await?;\n        \
// The defs behind those records, which the jail env can only carry for agents this host\n        \
// holds — the records above are what carries the rest.\n        \
let defs = self\n            .roster\n            .iter()\n            // only the held ones\n            \
.filter(|r| agents.contains(r))\n            .count();\n        \
let p = dir.join(\"x\"); // trailing note\n        \
started.push(p.display().to_string());\n        Ok(started.len() + defs)\n    }\n}\n";

    /// What rust-analyzer 2026-03-30 wrote for that range, renamed.
    const PRODUCED: &str = "impl Svc {\n    pub async fn start(&self, agents: &[String], dir: PathBuf) -> Result<usize, String> {\n        \
let (mut started, defs, p) = self.warm_up(agents, dir).await?;\n        \
started.push(p.display().to_string());\n        Ok(started.len() + defs)\n    }\n\n    \
async fn warm_up(&self, agents: &[String], dir: PathBuf) -> Result<(Vec<String>, usize, PathBuf), String> {\n        \
let mut started = self.seeded(agents).await?;\n        \
let defs = self\n            .roster\n            .iter()\n            // only the held ones\n            \
.filter(|r| agents.contains(r))\n            .count();\n        \
let p = dir.join(\"x\");\n        Ok((started, defs, p))\n    }\n}\n";

    fn the_range() -> Range {
        Range {
            start: Position { line: 3, col: 9 },
            end: Position { line: 12, col: 48 },
        }
    }

    #[test]
    fn lists_the_full_line_and_trailing_comments_the_assist_dropped_and_not_the_one_it_kept() {
        let lost = dropped(ORIGIN, the_range(), PRODUCED, "warm_up");

        assert_eq!(
            lost.iter()
                .map(|comment| (comment.line, comment.trailing))
                .collect::<Vec<_>>(),
            [(4, false), (5, false), (12, true)]
        );
    }

    #[test]
    fn puts_each_comment_back_beside_the_statement_it_annotated() {
        let (text, count) =
            restored(ORIGIN, the_range(), PRODUCED, "warm_up").expect("the statements pair");

        assert_eq!(count, 3);
        assert!(
            text.contains(
                "        let mut started = self.seeded(agents).await?;\n        \
                 // The defs behind those records, which the jail env can only carry for agents this host\n        \
                 // holds — the records above are what carries the rest.\n        let defs = self\n"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "        let p = dir.join(\"x\"); // trailing note\n        Ok((started, defs, p))"
            ),
            "{text}"
        );
        assert_eq!(text.matches("// only the held ones").count(), 1, "{text}");
    }

    #[test]
    fn statements_that_cannot_be_paired_refuse_naming_every_comment_that_would_be_lost() {
        // Given a produced body with one statement fewer than the range had, besides its tail
        let merged = PRODUCED.replace("        let p = dir.join(\"x\");\n", "");

        // When the comments are put back
        let refusal = restored(ORIGIN, the_range(), &merged, "warm_up")
            .expect_err("unpairable statements refuse")
            .to_string();

        // Then the refusal names every comment, as the server's defect
        assert!(
            refusal.starts_with(
                "rust-analyzer's answer was unusable: rust-analyzer dropped 3 comment(s) from the \
                 extracted function, and its statements cannot be paired with the range's to put \
                 them back:"
            ),
            "{refusal}"
        );
        assert!(refusal.contains("`// trailing note`"), "{refusal}");
    }

    #[test]
    fn counts_the_comments_at_risk_only_in_a_range_holding_a_question_mark() {
        let without_question_marks = ORIGIN.replace(".await?", ".await.unwrap()");

        assert_eq!(at_risk(ORIGIN, the_range()), 3);
        assert_eq!(at_risk(&without_question_marks, the_range()), 0);
    }
}
