//! The one rendering of a run's results, for every front end that shows them.
//!
//! The same shapes — findings, per-operation lines, the run summary, a verify comparison — used to
//! be stated in three places: this crate's command line, `tddy_tools::index_console` rendering
//! events the warm daemon streamed, and `tddy_index_daemon::render` narrating the daemon binary's
//! own single-shot runs. They read the same because a developer who exported `TDDY_INDEX_SOCKET`
//! must see the same account of the same operation as one who did not — and three statements of
//! that are three things that drift. One already had: #500's run-level narration went to stderr on
//! the cold path and to stdout on the warm one, and only a test asserting one front end's whole
//! output against the other's literals caught it.
//!
//! **Returns lines. Prints nothing.** Two reasons, and both are load-bearing.
//! `tests/library_returns_its_results.rs` asserts that `restructure_cli.rs` is the *only* module
//! under `src/` containing a print macro, because a front end that speaks a protocol on stdout — a
//! persistent server most obviously — would have its frames corrupted by a library writing into
//! that stream. And the daemon does not print at all: it logs, because fd 1 belongs to RPC framing
//! whenever it serves `--stdio`. A renderer that printed could serve neither.
//!
//! What a result *means* is deliberately **not** here. A check with findings and a comparison that
//! does not hold are both answered calls whose answer is a failed run, and each front end turns
//! that into its own thing — an `anyhow::Error` that becomes an exit code, a `Verdict`, an
//! `ExitCode`. So the wording of each refusal is published ([`findings_refusal`],
//! [`comparison_refusal`]) and the judgement stays with the caller that has to act on it.

use crate::edit::VisibilityChange;
use crate::runner::{Finding, Outcome, PlanProgress, RunSummary, SnapshotRewrite};
use crate::verify::{token_difference, Comparison, Excused};

/// Every line a whole run's result amounts to, in the order a reader reads them.
///
/// One arm per [`Outcome`] variant, because the six entry points answer six different questions.
/// A front end holding an `Outcome` — from a library call, or folded out of a stream of events —
/// writes these wherever it writes.
pub fn outcome(outcome: &Outcome, rehearsal: bool) -> Vec<String> {
    match outcome {
        Outcome::Applied(summary) => run_summary(summary, rehearsal),
        Outcome::Status(progress) => plan_progress(progress),
        Outcome::Checked(found) => findings(found),
        Outcome::ItemAnchored(found) => vec![item_anchor(found)],
        Outcome::Verified(comparison) => self::comparison(comparison),
        Outcome::Snapshotted(rewrite) => snapshot_rewrite(rewrite),
    }
}

/// What a `snapshot` did to a plan's header.
///
/// A plan that was already current says so rather than saying nothing: a silent no-op is
/// indistinguishable from a subcommand that did not run, the reason [`NO_FINDINGS`] exists.
pub fn snapshot_rewrite(rewrite: &SnapshotRewrite) -> Vec<String> {
    let mut lines = vec![if rewrite.rewritten {
        format!(
            "rewrote the snapshot header of {} over {} file(s)",
            rewrite.plan, rewrite.paths
        )
    } else {
        format!(
            "{} already snapshots the working tree over {} file(s)",
            rewrite.plan, rewrite.paths
        )
    }];
    let stale: Vec<(String, String)> = rewrite
        .stale
        .iter()
        .map(|found| (found.op.to_string(), found.reason.to_string()))
        .collect();
    let stale: Vec<(&str, &str)> = stale
        .iter()
        .map(|(op, reason)| (op.as_str(), reason.as_str()))
        .collect();
    lines.extend(stale_operations(&stale));
    lines
}

/// What the whole run amounted to.
///
/// `stopped_early` is stated *before* the count, and it is not decoration: a run that stopped where
/// it was told is a successful partial run, and "applied 2 of 5" on its own reads as one that
/// failed three quarters of the way through.
pub fn run_summary(summary: &RunSummary, rehearsal: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if summary.stopped_early {
        lines.push(format!(
            "   stopped after {} operations as requested",
            summary.applied
        ));
    }
    lines.push(format!(
        "{} {} of {} operations",
        if rehearsal { "resolved" } else { "applied" },
        summary.applied,
        summary.total
    ));
    lines
}

/// How far a plan's journal got.
pub fn plan_progress(progress: &PlanProgress) -> Vec<String> {
    vec![
        format!("completed {}", progress.completed),
        format!("in_flight {}", progress.in_flight),
        format!("failed {}", progress.failed),
        format!("pending {}", progress.pending),
    ]
}

/// One plan of a store as a front end holds it: its name, how many operations it has, whether it
/// has changes not yet written back, and its stale operations as `(operation, reason)`.
pub type HeldPlanRow<'a> = (&'a str, usize, bool, Vec<(&'a str, &'a str)>);

/// The plans a store holds, one line each — and under it one line per stale operation — or the
/// statement that it holds none.
///
/// Takes rows rather than a store type because the two front ends that render it hold the wire's
/// `LoadedPlan`, not this crate's. `changed` is a plan the store has changed and not yet written
/// back.
pub fn loaded_plans(held: &[HeldPlanRow<'_>]) -> Vec<String> {
    if held.is_empty() {
        return vec!["no plans loaded".to_string()];
    }
    held.iter()
        .flat_map(|(plan, operations, changed, stale)| {
            let line = format!(
                "{plan}: {operations} operation(s){}",
                if *changed {
                    ", not yet written back"
                } else {
                    ""
                }
            );
            std::iter::once(line).chain(stale_operations(stale))
        })
        .collect()
}

/// The operations of a plan that can no longer run as written, one line each, as
/// `(operation, reason)` pairs. Nothing for a plan with none, so a listing of sound plans reads as
/// it always did.
pub fn stale_operations(stale: &[(&str, &str)]) -> Vec<String> {
    stale
        .iter()
        .map(|(op, reason)| format!("  stale {op}: {reason}"))
        .collect()
}

/// Every finding a check made, or the statement that it made none.
///
/// For a front end handed the findings as values. One that is told about them a stream item at a
/// time counts them itself and uses [`finding`] and [`NO_FINDINGS`], because a count is all the
/// verdict needs and re-collecting a stream to render it would be for nothing.
pub fn findings(found: &[Finding]) -> Vec<String> {
    if found.is_empty() {
        return vec![NO_FINDINGS.to_string()];
    }
    found.iter().map(finding).collect()
}

/// One thing wrong with an operation, attributed to the operation that caused it.
pub fn finding(found: &Finding) -> String {
    format!("{}: {}", found.operation, found.detail)
}

/// What a check of a sound plan says. Stated rather than left out: a silent check is
/// indistinguishable from one that never ran.
pub const NO_FINDINGS: &str = "no findings";

/// Why a check that found something is a failed run.
///
/// The wording is published and the judgement is not: each front end decides what a check with
/// findings means to it, and every one of them means the same by this sentence.
pub fn findings_refusal(counted: usize) -> String {
    format!("{counted} finding(s) — see above. Nothing was written.")
}

/// Why an apply that carried out no operation is a failed run.
///
/// An apply used to be a successful run whenever nothing raised, so a run that performed nothing
/// at all exited zero and a script could not tell it from one that did the work. `--stop-after` is
/// excluded by its caller rather than here: a run that stopped where it was told to stop did what
/// it was asked, which is the same judgement the apply loop makes where it sets that flag.
///
/// The wording is published and the judgement is not, exactly as for [`findings_refusal`]: both
/// front ends mean the same by this sentence, and each decides for itself what it does about it.
pub fn nothing_applied_refusal(total: usize) -> String {
    format!("0 of {total} operation(s) were applied, and the run was not asked to stop short")
}

/// Why an apply whose account never arrived is a failed run.
///
/// Only the streaming front end can see this: the terminal event is a run's account of itself, and
/// a stream that ends without one leaves its caller unable to say whether the plan ran. The same
/// hole an event carrying no field at all would leave, and refused for the same reason.
pub const NO_OUTCOME_REFUSAL: &str =
    "the run ended without saying what it did — no outcome reached this caller, so whether the \
     plan was applied is unknown";

/// What holding the tree against a git ref found.
///
/// Ends with the verdict when the comparison holds, and with nothing when it does not — a
/// comparison that failed is reported by [`comparison_refusal`], because it is the thing that
/// fails the run and each front end raises that differently.
pub fn comparison(comparison: &Comparison) -> Vec<String> {
    let mut lines = vec![format!(
        "{} statements before, {} after",
        comparison.before, comparison.after
    )];
    lines.extend(excused_summary(&comparison.excused));
    lines.extend(token_difference(&comparison.missing, &comparison.added));
    lines.extend(
        comparison
            .missing
            .iter()
            .map(|statement| format!("missing: {statement}")),
    );
    lines.extend(
        comparison
            .added
            .iter()
            .map(|statement| format!("added:   {statement}")),
    );
    if comparison.holds() {
        lines.push("every statement accounted for".to_string());
    }
    lines
}

/// The one line naming churn a comparison set aside, or nothing when none was.
fn excused_summary(excused: &Excused) -> Option<String> {
    let parts: Vec<String> = [
        (
            excused.repointed,
            "statement(s) re-pointed through a module qualifier",
        ),
        (excused.visibility, "visibility-normalised"),
        (excused.cfg_test_gates, "cfg(test) gate line(s) excused"),
    ]
    .iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, what)| format!("{count} {what}"))
    .collect();
    (!parts.is_empty()).then(|| format!("verify: {}", parts.join(", ")))
}

/// Why a comparison that does not hold is a failed run.
pub fn comparison_refusal(comparison: &Comparison) -> String {
    format!(
        "{} statement(s) the tree lost and {} it gained — see above",
        comparison.missing.len(),
        comparison.added.len()
    )
}

/// The anchor `restructure anchors` found, as the JSON document a plan carries it as.
pub fn item_anchor(anchor: &crate::plan::Anchor) -> String {
    // An anchor is strings, numbers and maps of them, none of which `serde_json` can fail to write.
    serde_json::to_string(anchor).expect("an anchor serialises")
}

/// One line of per-operation progress.
///
/// Two numbers, because they answer different questions and are not interchangeable: `[4/29]` is
/// how far the run has got, and `op 3` is the operation's own index — the one `--from` and
/// `--stop-after` take and the one the journal records. Printing only a human counter would make
/// the number in the log the wrong number to resume from.
pub fn operation(
    index: usize,
    done: usize,
    total: usize,
    kind: &str,
    files: usize,
    applied: bool,
) -> String {
    format!(
        "[{}/{total}] op {index}: {kind} -> {files} file(s) {}",
        done + 1,
        if applied { "applied" } else { "resolved" }
    )
}

/// The transactional group an applied operation belongs to, as a line following the operation's.
///
/// Stated as a line of its own, as [`visibility`] is, so the operation's line keeps the shape every
/// front end and every script already reads.
pub fn group(name: &str) -> String {
    format!("   group: {name}")
}

/// One visibility an extraction had to widen, as a line in the run's account.
///
/// Takes the widening already stated because that is the form it travels in: the daemon's apply
/// loop carries `Vec<String>` on the event, having stated each one with [`widening`].
pub fn visibility(widened: &str) -> String {
    format!("   visibility: {widened}")
}

/// One visibility change, as the run states it.
///
/// Separate from [`visibility`] so that a host reporting widenings as values — the daemon's apply
/// loop, which puts them on an event rather than in a line — states them the same way the line
/// does. An extraction widens what it relocates, and that is an output of the operation rather
/// than an implementation detail, so the two accounts of it must agree.
pub fn widening(change: &VisibilityChange) -> String {
    format!("`{}` {} -> {}", change.item, change.from, change.to)
}

/// Anything else an operation had to say about itself, as a line in the run's account.
pub fn note(line: &str) -> String {
    format!("   note: {line}")
}

/// One line of the language server's narration of its own progress.
///
/// `stamp` is the time since the line before it, from
/// [`crate::restructure_cli::step_delta`] — and it is what distinguishes a six-minute crate-graph
/// load from a hang, so every front end with a clock passes one. `None` is for a front end that
/// has no per-line clock because its destination already timestamps: the daemon logs these, and a
/// second stamp inside a stamped log line is noise.
pub fn narration(kind: &str, stamp: Option<&str>, line: &str) -> String {
    match stamp {
        Some(stamp) => format!("   {kind} ({stamp}): {line}"),
        None => format!("   {kind}: {line}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moved here with the function it renders, from `runner::entry_points`, where it was private
    /// and therefore restated by both of the other front ends.
    #[test]
    fn reports_an_applied_operation_with_both_its_counter_and_its_index() {
        // Given the fourth operation of a 29-operation plan, whose index is 3
        let line = operation(3, 3, 29, "ExtractModuleToFile", 3, true);

        // Then the line carries how far the run has got, the resumable index, and the edit's width
        assert_eq!(
            line,
            "[4/29] op 3: ExtractModuleToFile -> 3 file(s) applied"
        );
    }

    #[test]
    fn a_failed_comparison_says_which_tokens_it_lost_and_gained() {
        // Given a comparison whose unexplained statements differ in a token
        let failed = Comparison {
            missing: vec!["let x = check(1);".to_string()],
            added: vec!["let x = inspect(1);".to_string()],
            ..Comparison::default()
        };

        // When it is rendered
        let lines = comparison(&failed);

        // Then one line names what changed
        assert!(
            lines.contains(&"verify: tokens lost: check x1; tokens gained: inspect x1".to_string()),
            "{lines:?}"
        );
    }

    #[test]
    fn distinguishes_a_resolved_operation_from_an_applied_one() {
        // Given the same operation resolved rather than applied
        let line = operation(3, 3, 29, "ExtractModuleToFile", 3, false);

        // Then it says so
        assert!(line.ends_with("resolved"), "{line}");
    }

    /// The daemon carries widenings as values on an event and this crate carries them in a line.
    /// Both state the change the same way, which is the only reason a developer reading one and
    /// then the other sees one account.
    #[test]
    fn states_a_widened_visibility_the_same_way_in_a_line_and_on_an_event() {
        // Given an item an extraction had to make public
        let change = VisibilityChange {
            item: "helper".to_string(),
            from: "private".to_string(),
            to: "pub(crate)".to_string(),
        };

        // When it is stated as a value and as a line
        let stated = widening(&change);

        // Then the line is the value with the account's own prefix, and nothing else differs
        assert_eq!(stated, "`helper` private -> pub(crate)");
        assert_eq!(
            visibility(&stated),
            "   visibility: `helper` private -> pub(crate)"
        );
    }

    /// A front end with a clock stamps each line with the time since the one before it; one whose
    /// destination already timestamps does not. The rest of the line is the same either way,
    /// because it is the same line.
    #[test]
    fn narrates_with_the_elapsed_stamp_a_front_end_has_a_clock_for() {
        // Given one line of a server's narration
        // When it is rendered with and without a stamp
        // Then only the stamp differs
        assert_eq!(
            narration("indexing", Some("+1m30s"), "loading crate graph (42%)"),
            "   indexing (+1m30s): loading crate graph (42%)"
        );
        assert_eq!(
            narration("indexing", None, "loading crate graph (42%)"),
            "   indexing: loading crate graph (42%)"
        );
    }

    #[test]
    fn lists_no_stale_lines_for_a_plan_with_no_stale_operations() {
        // Given no stale operations
        // When they are rendered
        let lines = stale_operations(&[]);

        // Then there is nothing to say
        assert_eq!(lines, Vec::<String>::new());
    }

    #[test]
    fn states_each_stale_operation_with_its_reason_on_a_line_of_its_own() {
        // Given two stale operations
        let stale = [("b1", "item changed"), ("b2", "edited by first.jsonl#a1")];

        // When they are rendered
        let lines = stale_operations(&stale);

        // Then each reads as an indented `stale <operation>: <reason>` line
        assert_eq!(
            lines,
            [
                "  stale b1: item changed",
                "  stale b2: edited by first.jsonl#a1"
            ]
        );
    }

    #[test]
    fn says_so_when_no_plans_are_loaded() {
        // Given a store holding nothing
        // When its plans are listed
        let lines = loaded_plans(&[]);

        // Then the statement that it holds none is the whole answer
        assert_eq!(lines, ["no plans loaded"]);
    }

    #[test]
    fn lists_sound_plans_one_line_each_and_marks_one_not_yet_written_back() {
        // Given a clean plan and a changed one, neither with stale operations
        let held: [HeldPlanRow; 2] = [
            ("first.jsonl", 3, false, Vec::new()),
            ("second.jsonl", 1, true, Vec::new()),
        ];

        // When they are listed
        let lines = loaded_plans(&held);

        // Then each is one line, and only the changed one says it is not yet written back
        assert_eq!(
            lines,
            [
                "first.jsonl: 3 operation(s)",
                "second.jsonl: 1 operation(s), not yet written back"
            ]
        );
    }

    #[test]
    fn lists_a_plans_stale_operations_under_its_line() {
        // Given a plan with two stale operations, and a sound one after it
        let held: [HeldPlanRow; 2] = [
            (
                "second.jsonl",
                4,
                true,
                vec![("b1", "item changed"), ("b3", "item not found in src/a.rs")],
            ),
            ("third.jsonl", 2, false, Vec::new()),
        ];

        // When they are listed
        let lines = loaded_plans(&held);

        // Then the stale operations follow their own plan's line and no other
        assert_eq!(
            lines,
            [
                "second.jsonl: 4 operation(s), not yet written back",
                "  stale b1: item changed",
                "  stale b3: item not found in src/a.rs",
                "third.jsonl: 2 operation(s)"
            ]
        );
    }

    fn a_snapshot_rewrite(
        rewritten: bool,
        stale: Vec<crate::plan_store::OpStaleness>,
    ) -> SnapshotRewrite {
        SnapshotRewrite {
            plan: "plan.jsonl".to_string(),
            paths: 3,
            rewritten,
            stale,
        }
    }

    #[test]
    fn a_snapshot_of_a_plan_with_no_stale_operations_says_only_what_it_did_to_the_header() {
        // Given a rewrite that changed the header and found nothing stale
        let rewrite = a_snapshot_rewrite(true, Vec::new());

        // When it is rendered
        let lines = snapshot_rewrite(&rewrite);

        // Then there is one line
        assert_eq!(
            lines,
            ["rewrote the snapshot header of plan.jsonl over 3 file(s)"]
        );
    }

    #[test]
    fn a_snapshot_names_each_stale_operation_after_the_header_line() {
        // Given a rewrite that left the header alone and found two operations stale
        let rewrite = a_snapshot_rewrite(
            false,
            vec![
                crate::plan_store::OpStaleness {
                    op: crate::OpId("b1".to_string()),
                    reason: crate::plan_store::StaleReason::ItemChanged,
                },
                crate::plan_store::OpStaleness {
                    op: crate::OpId("b2".to_string()),
                    reason: crate::plan_store::StaleReason::ItemNotFound {
                        file: "src/a.rs".to_string(),
                    },
                },
            ],
        );

        // When it is rendered
        let lines = snapshot_rewrite(&rewrite);

        // Then the header line is followed by one line per stale operation, in order
        assert_eq!(
            lines,
            [
                "plan.jsonl already snapshots the working tree over 3 file(s)",
                "  stale b1: item changed",
                "  stale b2: item not found in src/a.rs"
            ]
        );
    }
}
