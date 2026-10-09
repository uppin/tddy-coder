//! Rule 1: the comments rust-analyzer drops from a range that holds a `?`.
//!
//! The assist rebuilds the extracted body's statement list to wrap its outputs in `Ok(…)` and
//! loses the trivia between statements and after them. A comment inside a statement survives. The
//! statements keep their order and the assist appends one tail, so each lost comment is put back
//! beside the statement it annotated — or the operation is refused, naming every comment it would
//! lose.

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
}

/// The comments of `range` in `original` that the function `name` in `produced` does not hold,
/// counted as a multiset of trimmed comment text.
pub(super) fn dropped(original: &str, range: Range, produced: &str, name: &str) -> Vec<Comment> {
    let _ = (original, range, produced, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("dropped")
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
    let _ = (original, range, produced, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("restored")
}

/// How many comments of `range` rust-analyzer will drop: those between or after the range's
/// top-level statements, when the range holds a `?`. What `check` reports on its progress line.
pub(super) fn at_risk(text: &str, range: Range) -> usize {
    let _ = (text, range);
    // TODO(reshape-extract-method-clean): implement
    todo!("at_risk")
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
