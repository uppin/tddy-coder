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
//! each counted and summarised rather than silently dropped, so the exit status is a signal. The
//! passes run in this order, each on what the previous one left:
//!
//! 1. **`use` items** are scaffolding, wholly: a multi-line `use crate::{` … `};` is reduced to its
//!    first line before anything is compared, so its members are never read as statements. Imports
//!    are the compiler's to check. (A comment written *inside* a `use` group is excused with it.)
//! 2. **Exact multiset**: every statement is compared as a multiset across the whole crate, so a
//!    statement moving between files is not a difference. A leading `pub` / `pub(crate)` /
//!    `pub(super)` / `pub(in …)` is stripped first (the assist widens what it moves), a `#[cfg(test)]`
//!    directly above a `use` is dropped (the tidy gates an import only tests use), and a statement
//!    `rustfmt` wrapped is joined back into one when a `(` or `[` is left open (bounded, never across
//!    a blank line or a comment).
//! 3. **Visibility pairing**: a lost and a gained statement equal once a leading `pub…` is deleted.
//! 4. **Re-point pairing**: a lost and a gained statement, 1:1, equal once lowercase module
//!    qualifiers are deleted (`f(` becoming `m::f(`). Only lowercase segments go, so `Foo::new(` never
//!    pairs with `Bar::new(`.
//! 5. **Token multiset, last resort**: every leftover statement is tokenised — identifiers, numbers,
//!    string and char literals and each `//` comment as single opaque tokens, other punctuation one
//!    by one — dropping whitespace, `{` `}` `,` `;` and lowercase module qualifiers. When the lost
//!    and the gained tokens are the *same multiset* the leftovers differ only by reflow, regrouping
//!    or re-pointing (a match arm becoming a block, a closure call collapsing onto one line) and all
//!    are excused, counted in [`Excused::repointed`] (there is deliberately no separate wire count).
//!    When they differ nothing is excused here, and [`token_difference`] names the tokens that
//!    changed. It cannot excuse a renamed callee, a changed or dropped argument, a lost statement or
//!    a lost comment: each unbalances the multisets. The price is that one real loss anywhere keeps
//!    all the leftover reflow reported too, beside the tokens line that points at the loss.
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

mod statements;
// Post-move fix: the engine's named facade left out this `pub` item because nothing references it,
// which removed it from the crate's public path; see docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md § 7.
pub use statements::statements;

mod tokens;
pub use tokens::token_difference;

mod rebind;
pub use rebind::Rebind;
mod repoint;
pub use repoint::Repoint;
mod retarget;
pub use retarget::{Declared, Retarget};

/// Last resort for what the exact and 1:1 passes left: when the leftover lost statements and the
/// leftover gained ones carry the same token multiset, they differ only in layout and qualifiers.
fn reflow_pass(missing: Vec<String>, added: Vec<String>) -> Paired {
    let same = tokens::token_difference(&missing, &added).is_none();
    if same {
        return Paired {
            pairs: missing.len(),
            missing: Vec::new(),
            added: Vec::new(),
        };
    }
    Paired {
        missing,
        added,
        pairs: 0,
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
    statements::strip_visibility(statement).to_string()
}

fn re_point_key(statement: &str) -> String {
    let bare = statements::strip_qualifiers(statements::strip_visibility(statement));
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
    compare_with(before, after, &Declared::default())
}

/// [`compare`], told of the `impl` retargets the author declared (`restructure verify --retarget`).
///
/// The declaration is read by [`retarget::account`], which pairs a renamed statement with its
/// original and excuses the headers a split repeats. A retarget that was not declared is reported as
/// it always was.
pub fn compare_with(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    declared: &Declared,
) -> Comparison {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut before_statements: Vec<String> = Vec::new();
    let mut before_total = 0usize;
    let mut after_total = 0usize;
    let mut gates = 0usize;

    for text in before.values() {
        let (found, excused) = statements::analyse(text);
        gates += excused;
        before_total += found.len();
        for statement in found {
            *counts.entry(statement.clone()).or_default() += 1;
            before_statements.push(statement);
        }
    }
    for text in after.values() {
        let (found, excused) = statements::analyse(text);
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
    let accounted = retarget::account(widened.missing, widened.added, declared, &before_statements);
    let called = repoint::account(accounted.missing, accounted.added, declared);
    let paired = pair_by(called.missing, called.added, re_point_key);
    let reflowed = reflow_pass(paired.missing, paired.added);

    Comparison {
        before: before_total,
        after: after_total,
        missing: reflowed.missing,
        added: reflowed.added,
        excused: Excused {
            repointed: accounted.pairs + called.pairs + paired.pairs + reflowed.pairs,
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

    const CHECK_ARM: &str =
        "Command::Check => check(root, options, client, cancel).map(Outcome::Checked),\n";
    const CHECK_BLOCK: &str = "Command::Check => {\ncheck_entry_points::check(root, options, client, cancel).map(Outcome::Checked)\n}\n";

    #[test]
    fn a_multi_line_use_group_is_excused_whole_and_the_next_statement_still_compared() {
        let comparison = verdict(
            "use crate::{\n    A,\n    B,\n};\nlet x = 1;\n",
            "use crate::{\n    C,\n    runner::{a, b},\n};\nlet x = 2;\n",
        );

        assert_eq!(comparison.missing, ["let x = 1;"]);
        assert_eq!(comparison.added, ["let x = 2;"]);
    }

    #[test]
    fn a_multi_line_public_use_group_is_excused_whole() {
        let comparison = verdict(
            "pub use crate::{\n    A,\n};\n",
            "pub(crate) use crate::{\n    B,\n    c::{D, E},\n};\n",
        );

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn a_match_arm_reflowed_into_a_block_with_a_gained_qualifier_is_excused() {
        let comparison = verdict(CHECK_ARM, CHECK_BLOCK);

        assert!(comparison.holds(), "{comparison:?}");
        assert_eq!(comparison.excused.repointed, 1);
    }

    #[test]
    fn a_closure_let_reflowed_to_one_line_with_a_gained_qualifier_is_excused() {
        let comparison = verdict(
            "let (journal, lowered) = open_run(plan, root, || {\n    refuse(root, plan)\n})?;\n",
            "let (journal, lowered) = anchors::open_run(plan, root, || refuse(root, plan))?;\n",
        );

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn the_real_entry_points_split_exits_clean() {
        let before = "use crate::{\n    AppliedRun, Command, Finding,\n    StatePaths,\n};\n\
            fn run() {\n\
            match command {\n\
            Command::Anchors => item_anchors(root, options, client, cancel).map(Outcome::ItemAnchored),\n\
            Command::Check => check(root, options, client, cancel).map(Outcome::Checked),\n\
            }\n\
            let (journal, lowered) = open_run_resolving_anchors(plan, root, &paths, options, registry, || {\n\
            refuse_a_broken_baseline(root, plan, options, cancel)\n\
            })?;\n}\n";
        let after = "use crate::{\n    plan_store::PlanStore,\n    runner::{entry_points::anchor_entry_points, restore_ledger, AppliedRun},\n};\n\
            fn run() {\n\
            match command {\n\
            Command::Anchors => anchor_entry_points::item_anchors(root, options, client, cancel)\n\
            .map(Outcome::ItemAnchored),\n\
            Command::Check => {\n\
            check_entry_points::check(root, options, client, cancel).map(Outcome::Checked)\n\
            }\n\
            }\n\
            let (journal, lowered) = anchor_entry_points::open_run_resolving_anchors(plan, root, &paths, options, registry, || refuse_a_broken_baseline(root, plan, options, cancel))?;\n}\n";

        let comparison = verdict(before, after);

        assert!(comparison.holds(), "{comparison:?}");
    }

    #[test]
    fn a_renamed_callee_is_not_excused_and_the_tokens_say_which() {
        let comparison = verdict(CHECK_ARM, &CHECK_BLOCK.replace("::check(", "::inspect("));

        let tokens = tokens::token_difference(&comparison.missing, &comparison.added);

        assert!(!comparison.holds());
        assert_eq!(
            tokens.as_deref(),
            Some("verify: tokens lost: check x1; tokens gained: inspect x1")
        );
    }

    #[test]
    fn a_dropped_argument_is_not_excused() {
        let comparison = verdict(CHECK_ARM, &CHECK_BLOCK.replace("client, cancel", "client"));

        assert!(!comparison.holds());
        assert_eq!(
            token_difference(&comparison.missing, &comparison.added).as_deref(),
            Some("verify: tokens lost: cancel x1; tokens gained: none")
        );
    }

    #[test]
    fn a_lost_comment_is_still_reported_beside_reflow_noise() {
        let comparison = verdict(&format!("// the check arm\n{CHECK_ARM}"), CHECK_BLOCK);

        assert!(comparison.missing.contains(&"// the check arm".to_string()));
    }

    #[test]
    fn a_lost_statement_beside_unrelated_reflow_is_reported_with_its_tokens() {
        let comparison = verdict(&format!("let spread = 1;\n{CHECK_ARM}"), CHECK_BLOCK);

        assert!(!comparison.holds());
        assert_eq!(
            token_difference(&comparison.missing, &comparison.added).as_deref(),
            Some("verify: tokens lost: 1 x1, = x1, let x1, spread x1; tokens gained: none")
        );
    }

    #[test]
    fn equal_leftovers_have_no_token_difference() {
        assert_eq!(token_difference(&[], &[]), None);
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
