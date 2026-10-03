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
//! What is excused, because an `extract_module` always causes it and it is not a behaviour change —
//! each counted and summarised rather than silently dropped, so the exit status is a signal:
//!
//! - a leading `pub` / `pub(crate)` / `pub(super)` / `pub(in …)` on any statement (the assist widens
//!   what it moves);
//! - a call re-pointed through a module qualifier (`f(` becoming `m::f(`), when the two statements
//!   are otherwise identical — paired one to one, and only lowercase module segments are deleted, so
//!   `Foo::new(` never pairs with `Bar::new(`;
//! - a `#[cfg(test)]` directly above a `use` (the tidy gates an import only tests use);
//! - `rustfmt` wrapping a statement the above made longer: statements are compared as logical
//!   statements, a wrapped one joined back into one line (bounded, never across a blank or comment).
//!
//! Everything else stays an error, comments included: a lost banner comment is the finding this exists for.
//!
//! Whole-crate rather than per-seam on purpose: a restructure moves code *within* a crate, so the
//! crate is the unit over which the multiset is invariant, and no knowledge of the seams is needed.

use std::collections::BTreeMap;

/// What comparing two trees found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    pub before: usize,
    pub after: usize,
    /// Statements the ref had that the working tree does not, with their multiplicity.
    pub missing: Vec<String>,
    /// Statements the working tree has that the ref did not.
    pub added: Vec<String>,
    /// What was set aside as expected churn rather than reported.
    pub excused: Excused,
}

/// Differences a restructure is known to cause, counted so they are visible without being errors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Excused {
    /// Lost/gained pairs equal once module qualifiers are deleted.
    pub repointed: usize,
    /// Lost/gained pairs equal once a leading `pub…` is deleted.
    pub visibility: usize,
    /// `#[cfg(test)]` lines directly above a `use`.
    pub cfg_test_gates: usize,
}

impl Comparison {
    pub fn holds(&self) -> bool {
        self.missing.is_empty() && self.added.is_empty()
    }
}

/// Every logical statement that carries behaviour, normalised for what a relocation always changes.
///
/// Indentation is stripped because relocating an item into a module shifts every line of it by a
/// level, which is not a change in meaning. Everything else is kept verbatim — a string literal's
/// contents included, since reindenting inside one *is* a behaviour change.
///
/// A statement that `rustfmt` wrapped over several lines is joined back into one (see
/// [`join_wrapped`]), so a widening or re-point that made a line too long to fit does not read as a
/// loss. `use`, `mod` and the bare braces of a wrapper are excluded because they are exactly what a
/// restructure is *supposed* to add and remove, and so is a `#[cfg(test)]` directly above a `use`
/// (the tidy gates an import only tests use). Comments are deliberately included: a dropped comment
/// is the finding this comparison exists to make.
pub fn statements(text: &str) -> Vec<String> {
    analyse(text).0
}

/// [`statements`], and how many `#[cfg(test)]` lines above a `use` were set aside.
fn analyse(text: &str) -> (Vec<String>, usize) {
    let lines: Vec<&str> = text.split('\n').map(str::trim).collect();
    let logical: Vec<String> = join_wrapped(&lines)
        .into_iter()
        .filter(|statement| !statement.is_empty())
        .collect();
    let (gated, excused) = drop_use_gates(logical);
    let kept = gated
        .into_iter()
        .filter(|statement| !is_structural(statement))
        .collect();
    (kept, excused)
}

/// The most physical lines one logical statement is joined from; past it the lines stay physical.
const JOIN_LIMIT: usize = 40;

/// Join a statement `rustfmt` wrapped — one that ends mid-expression with a `(` or `[` still open —
/// into a single line, so it compares equal to its unwrapped form and the other way round.
///
/// Never joins across a blank line or a comment, and gives up (keeping physical lines) at
/// [`JOIN_LIMIT`] or when the brackets never close.
fn join_wrapped(lines: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        match wrapped_span(lines, at) {
            Some(len) => {
                out.push(tidy_joined(&lines[at..at + len]));
                at += len;
            }
            None => {
                out.push(lines[at].to_string());
                at += 1;
            }
        }
    }
    out
}

/// How many lines from `at` make one wrapped statement, when the line at `at` starts one.
fn wrapped_span(lines: &[&str], at: usize) -> Option<usize> {
    let first = lines[at];
    let ends_whole = [";", "{", "}", ",", "=>"]
        .iter()
        .any(|end| first.ends_with(end));
    if first.is_empty() || first.starts_with("//") || ends_whole {
        return None;
    }
    let mut depth = bracket_depth(first);
    if depth <= 0 {
        return None;
    }
    for (offset, next) in lines.iter().enumerate().skip(at + 1).take(JOIN_LIMIT - 1) {
        if next.is_empty() || next.starts_with("//") {
            return None;
        }
        depth += bracket_depth(next);
        if depth <= 0 {
            return Some(offset - at + 1);
        }
    }
    None
}

/// Net `(`/`[` opened by a line, ignoring any inside a string literal.
fn bracket_depth(line: &str) -> i32 {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        match (in_string, escaped, ch) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (_, _, '"') => in_string = !in_string,
            (false, _, '(' | '[') => depth += 1,
            (false, _, ')' | ']') => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// The joined lines as one, spaced as the unwrapped form is: no space inside a bracket, and none of
/// the trailing comma `rustfmt` adds to a wrapped list.
fn tidy_joined(lines: &[&str]) -> String {
    [
        (", )", ")"),
        (", ]", "]"),
        ("( ", "("),
        ("[ ", "["),
        (" )", ")"),
        (" ]", "]"),
    ]
    .iter()
    .fold(lines.join(" "), |text, (from, to)| text.replace(from, to))
}

/// Remove each `#[cfg(test)]` whose next statement is a `use`, counting them.
fn drop_use_gates(statements: Vec<String>) -> (Vec<String>, usize) {
    let mut kept = Vec::with_capacity(statements.len());
    let mut excused = 0;
    for (at, statement) in statements.iter().enumerate() {
        let gates_a_use = statement == "#[cfg(test)]"
            && statements
                .get(at + 1)
                .is_some_and(|next| strip_visibility(next).starts_with("use "));
        if gates_a_use {
            excused += 1;
        } else {
            kept.push(statement.clone());
        }
    }
    (kept, excused)
}

/// The statement without a leading `pub`, `pub(crate)`, `pub(super)` or `pub(in …)`.
fn strip_visibility(statement: &str) -> &str {
    let Some(rest) = statement.strip_prefix("pub") else {
        return statement;
    };
    if let Some(after) = rest.strip_prefix(' ') {
        return after;
    }
    match rest
        .strip_prefix('(')
        .and_then(|inner| inner.split_once(") "))
    {
        Some((_, after)) => after,
        None => statement,
    }
}

/// Whether a line is scaffolding a restructure is allowed to move, add or remove.
fn is_structural(line: &str) -> bool {
    let body = strip_visibility(line);

    matches!(body, "{" | "}" | "};" | "})" | "});")
        || body.starts_with("use ")
        || body.starts_with("mod ")
        || body.starts_with("impl ")
        || body == "impl"
}

/// The statement with every module qualifier — a lowercase `segment::` before an identifier — deleted.
///
/// Uppercase segments are types and stay, so `Foo::new(` and `Bar::new(` remain different. Text in a
/// string literal is left alone.
fn strip_qualifiers(statement: &str) -> String {
    let chars: Vec<char> = statement.chars().collect();
    let mut out = String::with_capacity(statement.len());
    let mut at = 0;
    let mut in_string = false;
    while at < chars.len() {
        if chars[at] == '"' && (at == 0 || chars[at - 1] != '\\') {
            in_string = !in_string;
        }
        let qualifier = if in_string {
            0
        } else {
            qualifier_chain_len(&chars, at)
        };
        if qualifier > 0 {
            at += qualifier;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// Length of the run of `segment::` qualifiers starting at `at` (`crate::a::b::` is one run).
fn qualifier_chain_len(chars: &[char], at: usize) -> usize {
    let mut total = 0;
    let mut chained = false;
    loop {
        let len = qualifier_len(chars, at + total, chained);
        if len == 0 {
            return total;
        }
        total += len;
        chained = true;
    }
}

/// Length of the `segment::` starting at `at`, or 0 when no module qualifier starts there.
fn qualifier_len(chars: &[char], at: usize, chained: bool) -> usize {
    let Some(&first) = chars.get(at) else {
        return 0;
    };
    let starts_token = chained
        || at == 0
        || !(chars[at - 1].is_alphanumeric() || matches!(chars[at - 1], '_' | ':'));
    if !starts_token || !(first.is_ascii_lowercase() || first == '_') {
        return 0;
    }
    let segment = chars[at..]
        .iter()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || **c == '_')
        .count();
    let after = at + segment;
    let separated = chars.get(after) == Some(&':') && chars.get(after + 1) == Some(&':');
    let names_something = chars
        .get(after + 2)
        .is_some_and(|c| c.is_alphabetic() || *c == '_');
    if separated && names_something {
        segment + 2
    } else {
        0
    }
}

/// What is left of two lists after pairing entries that agree on a key, one to one.
struct Paired {
    missing: Vec<String>,
    added: Vec<String>,
    pairs: usize,
}

fn pair_by(missing: Vec<String>, added: Vec<String>, key: fn(&str) -> String) -> Paired {
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for statement in added {
        by_key.entry(key(&statement)).or_default().push(statement);
    }
    let mut unpaired = Vec::new();
    let mut pairs = 0;
    for statement in missing {
        match by_key.get_mut(&key(&statement)).and_then(Vec::pop) {
            Some(_) => pairs += 1,
            None => unpaired.push(statement),
        }
    }
    let mut added: Vec<String> = by_key.into_values().flatten().collect();
    added.sort();
    Paired {
        missing: unpaired,
        added,
        pairs,
    }
}

fn visibility_key(statement: &str) -> String {
    strip_visibility(statement).to_string()
}

fn re_point_key(statement: &str) -> String {
    let bare = strip_qualifiers(strip_visibility(statement));
    bare.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compare two sets of sources, keyed by path, as one multiset of statements each.
///
/// Paths are not compared. A statement moving from one file to another is the entire point of a
/// restructure, so only the crate-wide totals are held against each other.
///
/// What the exact comparison leaves is then reconciled, each lost statement against at most one
/// gained one: first those equal but for a leading visibility (an `extract_module` widens what it
/// moves), then those equal but for module qualifiers (it re-points calls through the new module).
/// Anything else — a changed argument, another call target, a statement with no counterpart — stays
/// reported.
pub fn compare(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Comparison {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut before_total = 0usize;
    let mut after_total = 0usize;
    let mut gates = 0usize;

    for text in before.values() {
        let (found, excused) = analyse(text);
        gates += excused;
        before_total += found.len();
        for statement in found {
            *counts.entry(statement).or_default() += 1;
        }
    }
    for text in after.values() {
        let (found, excused) = analyse(text);
        gates += excused;
        after_total += found.len();
        for statement in found {
            *counts.entry(statement).or_default() -= 1;
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

    let widened = pair_by(missing, added, visibility_key);
    let re_pointed = pair_by(widened.missing, widened.added, re_point_key);

    Comparison {
        before: before_total,
        after: after_total,
        missing: re_pointed.missing,
        added: re_pointed.added,
        excused: Excused {
            repointed: re_pointed.pairs,
            visibility: widened.pairs,
            cfg_test_gates: gates,
        },
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

    #[test]
    fn counts_the_statements_on_each_side() {
        let before = sources(&[("lib.rs", "use std::fmt;\nlet total = 2;\n")]);
        let after = sources(&[("lib.rs", "let total = 2;\n")]);

        let comparison = compare(&before, &after);

        assert_eq!((comparison.before, comparison.after), (1, 1));
    }

    fn verdict(before: &str, after: &str) -> Comparison {
        compare(
            &sources(&[("lib.rs", before)]),
            &sources(&[("lib.rs", after)]),
        )
    }

    #[test]
    fn excuses_a_call_re_pointed_through_a_module_qualifier() {
        let comparison = verdict(
            "let changed = refresh_pending(&mut rows, edit)?;\n",
            "let changed = refresh::refresh_pending(&mut rows, edit)?;\n",
        );

        assert!(comparison.holds());
        assert_eq!(comparison.excused.repointed, 1);
    }

    #[test]
    fn excuses_a_path_re_pointed_through_super_and_crate_prefixes() {
        let comparison = verdict(
            "let total = tally(1);\nlet sum = add(2);\n",
            "let total = super::tally(1);\nlet sum = crate::a::b::add(2);\n",
        );

        assert!(comparison.holds());
        assert_eq!(comparison.excused.repointed, 2);
    }

    #[test]
    fn does_not_pair_a_re_point_that_calls_a_different_function() {
        let comparison = verdict("let x = tally(1);\n", "let x = refresh::other(1);\n");

        assert_eq!(comparison.missing, ["let x = tally(1);"]);
        assert_eq!(comparison.added, ["let x = refresh::other(1);"]);
    }

    #[test]
    fn does_not_pair_a_re_point_whose_arguments_differ() {
        let comparison = verdict("let x = tally(1);\n", "let x = refresh::tally(2);\n");

        assert!(!comparison.holds());
        assert_eq!(comparison.excused.repointed, 0);
    }

    #[test]
    fn does_not_pair_the_same_associated_function_of_two_types() {
        let comparison = verdict("let x = Foo::new();\n", "let x = Bar::new();\n");

        assert_eq!(comparison.missing, ["let x = Foo::new();"]);
        assert_eq!(comparison.added, ["let x = Bar::new();"]);
    }

    #[test]
    fn pairs_each_lost_statement_with_at_most_one_gained() {
        let comparison = verdict(
            "let x = tally(1);\n",
            "let x = a::tally(1);\nlet x = b::tally(1);\n",
        );

        assert_eq!(comparison.excused.repointed, 1);
        assert_eq!(comparison.added.len(), 1);
        assert!(comparison.missing.is_empty());
    }

    #[test]
    fn does_not_look_for_module_qualifiers_inside_a_string_literal() {
        let comparison = verdict("let x = \"a::b\";\n", "let x = \"b\";\n");

        assert!(!comparison.holds());
    }

    #[test]
    fn excuses_a_widened_field_and_a_widened_function() {
        let comparison = verdict(
            "file: String,\nfn clamp() -> u8 {\n    1\n}\n",
            "pub(crate) file: String,\npub(super) fn clamp() -> u8 {\n    1\n}\n",
        );

        assert!(comparison.holds());
        assert_eq!(comparison.excused.visibility, 2);
    }

    #[test]
    fn excuses_a_restricted_visibility_path() {
        let comparison = verdict(
            "fn clamp() -> u8 {\n    1\n}\n",
            "pub(in crate::plan) fn clamp() -> u8 {\n    1\n}\n",
        );

        assert!(comparison.holds());
    }

    #[test]
    fn still_reports_a_new_public_function_with_no_counterpart() {
        let comparison = verdict(
            "fn clamp() -> u8 {\n    1\n}\n",
            "fn clamp() -> u8 {\n    1\n}\npub fn smuggled() -> u8 {\n    7\n}\n",
        );

        assert_eq!(comparison.added, ["7", "pub fn smuggled() -> u8 {"]);
    }

    #[test]
    fn excuses_a_cfg_test_gate_above_a_use() {
        let comparison = verdict(
            "use std::fmt;\nlet total = 2;\n",
            "#[cfg(test)]\nuse std::fmt;\nlet total = 2;\n",
        );

        assert!(comparison.holds());
        assert_eq!(comparison.excused.cfg_test_gates, 1);
    }

    #[test]
    fn excuses_a_cfg_test_gate_above_a_public_use() {
        let comparison = verdict("pub use a::b;\n", "#[cfg(test)]\npub(crate) use a::b;\n");

        assert!(comparison.holds());
    }

    #[test]
    fn reports_a_cfg_test_gate_above_a_function() {
        let comparison = verdict("fn helper() {\n}\n", "#[cfg(test)]\nfn helper() {\n}\n");

        assert_eq!(comparison.added, ["#[cfg(test)]"]);
    }

    #[test]
    fn reports_a_cfg_test_gate_above_a_mod() {
        let comparison = verdict("", "#[cfg(test)]\nmod tests {\n}\n");

        assert_eq!(comparison.added, ["#[cfg(test)]"]);
    }

    #[test]
    fn a_reflowed_signature_equals_its_single_line_form() {
        let comparison = verdict(
            "fn refresh(rows: &mut [Row], edit: Edit) -> bool {\n    true\n}\n",
            "fn refresh(\n    rows: &mut [Row],\n    edit: Edit,\n) -> bool {\n    true\n}\n",
        );

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn a_single_line_signature_equals_its_reflowed_form() {
        let comparison = verdict(
            "let changed = refresh(\n    rows,\n    edit,\n)?;\n",
            "let changed = refresh(rows, edit)?;\n",
        );

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn a_widened_and_reflowed_signature_is_excused_together() {
        let comparison = verdict(
            "fn refresh(rows: &mut [Row], edit: Edit) -> bool {\n    true\n}\n",
            "pub(crate) fn refresh(\n    rows: &mut [Row],\n    edit: Edit,\n) -> bool {\n    true\n}\n",
        );

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn does_not_join_across_a_comment_line() {
        let comparison = verdict("let x = f(\n// why\n1);\n", "let x = f(1);\n");

        assert!(!comparison.holds());
    }

    #[test]
    fn reports_a_lost_banner_comment_alongside_excused_churn() {
        let comparison = verdict(
            "// \u{2500}\u{2500} output types \u{2500}\u{2500}\nfile: String,\n",
            "pub(crate) file: String,\n",
        );

        assert_eq!(
            comparison.missing,
            ["// \u{2500}\u{2500} output types \u{2500}\u{2500}"]
        );
    }

    /// The split of `plan_store.rs` into `plan_store/refresh.rs` that could not exit zero.
    #[test]
    fn holds_for_the_plan_store_refresh_split() {
        let before =
            "let changed = refresh_pending(&mut refreshed[position + 1..], edit, resolver)?;\n";
        let after = "#[cfg(test)]\nuse refresh::refresh_pending;\nlet changed = refresh::refresh_pending(&mut refreshed[position + 1..], edit, resolver)?;\n";

        let comparison = verdict(before, after);

        assert!(comparison.holds(), "{comparison:?}");
        assert_eq!(
            (
                comparison.excused.repointed,
                comparison.excused.cfg_test_gates
            ),
            (1, 1)
        );
    }
}
