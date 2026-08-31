//! Statement-level comparison of a working tree against a git ref.
//!
//! Green tests are necessary and weak. A restructure claims to preserve behaviour, and that claim is
//! only ever a comparison between two states — so the strongest evidence available is to compare the
//! two texts statement by statement, as multisets, across the whole crate.
//!
//! What that catches, and nothing else does: a comment attached to no item. rust-analyzer relocates
//! trivia along with the item it belongs to, and trivia belonging to nothing is simply not carried.
//! The compiler cannot see it, the test suite cannot see it, and a diff of the *moved* lines cannot
//! see it either — the lines in question moved nowhere. On one real split two such comments went
//! missing and only a mechanical before-and-after count surfaced them.
//!
//! Whole-crate rather than per-seam on purpose: a restructure moves code *within* a crate, so the
//! crate is the unit over which the multiset is invariant, and no knowledge of the seams is needed.

use std::collections::BTreeMap;

/// What comparing two trees found.
pub struct Comparison {
    pub before: usize,
    pub after: usize,
    /// Statements the ref had that the working tree does not, with their multiplicity.
    pub missing: Vec<String>,
    /// Statements the working tree has that the ref did not.
    pub added: Vec<String>,
}

impl Comparison {
    pub fn holds(&self) -> bool {
        self.missing.is_empty() && self.added.is_empty()
    }
}

/// Every line that carries behaviour, normalised for the one thing a relocation always changes.
///
/// Indentation is stripped because relocating an item into a module shifts every line of it by a
/// level, which is not a change in meaning. Everything else is kept verbatim — a string literal's
/// contents included, since reindenting inside one *is* a behaviour change.
///
/// `use`, `mod` and the bare braces of a wrapper are excluded because they are exactly what a
/// restructure is *supposed* to add and remove. Comments are deliberately included: a dropped comment
/// is the finding this comparison exists to make.
pub fn statements(text: &str) -> Vec<String> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !is_structural(line))
        .map(str::to_string)
        .collect()
}

/// Whether a line is scaffolding a restructure is allowed to move, add or remove.
fn is_structural(line: &str) -> bool {
    let body = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);

    matches!(body, "{" | "}" | "};" | "})" | "});")
        || body.starts_with("use ")
        || body.starts_with("mod ")
        || body.starts_with("impl ")
        || body == "impl"
}

/// Compare two sets of sources, keyed by path, as one multiset of statements each.
///
/// Paths are not compared. A statement moving from one file to another is the entire point of a
/// restructure, so only the crate-wide totals are held against each other.
pub fn compare(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Comparison {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut before_total = 0usize;
    let mut after_total = 0usize;

    for text in before.values() {
        for statement in statements(text) {
            *counts.entry(statement).or_default() += 1;
            before_total += 1;
        }
    }
    for text in after.values() {
        for statement in statements(text) {
            *counts.entry(statement).or_default() -= 1;
            after_total += 1;
        }
    }

    let mut missing = Vec::new();
    let mut added = Vec::new();
    for (statement, delta) in counts {
        for _ in 0..delta.max(0) {
            missing.push(statement.clone());
        }
        for _ in 0..(-delta).max(0) {
            added.push(statement.clone());
        }
    }

    Comparison {
        before: before_total,
        after: after_total,
        missing,
        added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect()
    }

    /// The whole point: relocating an item indents it and wraps it in a `mod`, and neither is a change
    /// in behaviour.
    #[test]
    fn holds_across_an_item_relocated_into_a_module() {
        let before = sources(&[("lib.rs", "pub fn tally() -> u8 {\n    1 + 1\n}\n")]);
        let after = sources(&[(
            "lib.rs",
            "mod counting {\n    pub fn tally() -> u8 {\n        1 + 1\n    }\n}\npub use counting::*;\n",
        )]);

        assert!(compare(&before, &after).holds());
    }

    #[test]
    fn holds_when_a_statement_moves_to_another_file() {
        let before = sources(&[("lib.rs", "let spread = 1;\n"), ("other.rs", "")]);
        let after = sources(&[("lib.rs", ""), ("other.rs", "let spread = 1;\n")]);

        assert!(compare(&before, &after).holds());
    }

    #[test]
    fn reports_a_statement_the_tree_lost() {
        let before = sources(&[("lib.rs", "let spread = 1;\nlet total = 2;\n")]);
        let after = sources(&[("lib.rs", "let total = 2;\n")]);

        assert_eq!(compare(&before, &after).missing, ["let spread = 1;"]);
    }

    #[test]
    fn reports_a_statement_the_tree_gained() {
        let before = sources(&[("lib.rs", "let total = 2;\n")]);
        let after = sources(&[("lib.rs", "let total = 2;\nlet smuggled = 7;\n")]);

        assert_eq!(compare(&before, &after).added, ["let smuggled = 7;"]);
    }

    /// The finding this comparison exists for: trivia attached to nothing is relocated by nobody.
    #[test]
    fn reports_an_orphaned_comment_that_was_dropped() {
        let before = sources(&[(
            "lib.rs",
            "// documents a wrapper deleted long ago\nlet total = 2;\n",
        )]);
        let after = sources(&[("lib.rs", "let total = 2;\n")]);

        assert_eq!(
            compare(&before, &after).missing,
            ["// documents a wrapper deleted long ago"]
        );
    }

    /// A widening the assist could not avoid is a real difference and has to show, or the comparison
    /// would normalise away the one output the visibility report exists to name.
    #[test]
    fn reports_a_visibility_the_assist_widened() {
        let before = sources(&[("lib.rs", "fn clamp() -> u8 {\n    1\n}\n")]);
        let after = sources(&[("lib.rs", "pub(crate) fn clamp() -> u8 {\n    1\n}\n")]);

        let comparison = compare(&before, &after);

        assert_eq!(comparison.missing, ["fn clamp() -> u8 {"]);
        assert_eq!(comparison.added, ["pub(crate) fn clamp() -> u8 {"]);
    }

    #[test]
    fn counts_the_statements_on_each_side() {
        let before = sources(&[("lib.rs", "use std::fmt;\nlet total = 2;\n")]);
        let after = sources(&[("lib.rs", "let total = 2;\n")]);

        let comparison = compare(&before, &after);

        assert_eq!((comparison.before, comparison.after), (1, 1));
    }
}
