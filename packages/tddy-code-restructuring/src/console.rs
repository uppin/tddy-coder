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
use crate::verify::Comparison;
use std::time::{Duration, Instant};

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
    vec![if rewrite.rewritten {
        format!(
            "rewrote the snapshot header of {} over {} file(s)",
            rewrite.plan, rewrite.paths
        )
    } else {
        format!(
            "{} already snapshots the working tree over {} file(s)",
            rewrite.plan, rewrite.paths
        )
    }]
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

/// The plans a store holds, one line each, or the statement that it holds none.
///
/// Takes `(plan, operations, changed)` triples rather than a store type because the two front ends
/// that render it hold the wire's `LoadedPlan`, not this crate's. `changed` is a plan the store has
/// changed and not yet written back.
pub fn loaded_plans(held: &[(&str, usize, bool)]) -> Vec<String> {
    if held.is_empty() {
        return vec!["no plans loaded".to_string()];
    }
    held.iter()
        .map(|(plan, operations, changed)| {
            format!(
                "{plan}: {operations} operation(s){}",
                if *changed {
                    ", not yet written back"
                } else {
                    ""
                }
            )
        })
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

/// Decides which of a server's narration lines a console shows.
///
/// A cold index load reports one progress token per proc-macro, dylib and crate — hundreds of
/// lines that bury the few that matter. Both front ends (the in-process one and
/// `tddy_tools::index_console`) pass every line through one throttle so they cannot drift.
///
/// Rules:
/// - a line that does not start with `working` — a phase change, `crate index ready`, an error,
///   `warming crate index...` — is always admitted;
/// - `working...` lines are per-item progress: the first one after any non-working line is
///   admitted, then at most one per [`WORKING_INTERVAL`];
/// - a line ending `(100%)` is always admitted.
///
/// Pure and clock-injected: the caller supplies `now`, and nothing is printed here. What the
/// caller stamps a printed line with is the time since the last line *admitted*, so the stamps
/// stay true when lines are dropped.
#[derive(Debug, Default)]
pub struct NarrationThrottle {
    /// When a `working` line was last admitted; `None` after any non-working line.
    last_working: Option<Instant>,
}

/// The least time between two admitted per-item progress lines.
pub const WORKING_INTERVAL: Duration = Duration::from_secs(2);

impl NarrationThrottle {
    /// Whether `line`, arriving at `now`, is to be shown.
    pub fn admit(&mut self, line: &str, now: Instant) -> bool {
        if !line.starts_with("working") {
            self.last_working = None;
            return true;
        }
        let due = self
            .last_working
            .is_none_or(|last| now.duration_since(last) >= WORKING_INTERVAL);
        if due || line.ends_with("(100%)") {
            self.last_working = Some(now);
            return true;
        }
        false
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

    fn a_throttle_and_a_start() -> (NarrationThrottle, Instant) {
        (NarrationThrottle::default(), Instant::now())
    }

    #[test]
    fn a_throttle_always_admits_a_line_that_is_not_per_item_progress() {
        // Given a throttle that has just admitted a working line
        let (mut throttle, t0) = a_throttle_and_a_start();
        throttle.admit("working: proc-macro serde built", t0);

        // When phase changes and errors arrive in the same instant
        // Then every one of them is admitted
        assert!(throttle.admit("warming crate index...", t0));
        assert!(throttle.admit("crate index ready", t0));
        assert!(throttle.admit("error: the server exited", t0));
    }

    #[test]
    fn a_throttle_admits_the_first_working_line_then_one_per_two_seconds() {
        // Given a throttle and a burst of working lines, one every 100ms
        let (mut throttle, t0) = a_throttle_and_a_start();
        let at = |ms: u64| t0 + Duration::from_millis(ms);

        // When they arrive
        let admitted: Vec<bool> = [0, 100, 200, 300, 1999, 2000, 2100]
            .iter()
            .map(|&ms| throttle.admit("working: proc-macro serde built", at(ms)))
            .collect();

        // Then the first is admitted, the next four are dropped, one at 2s is admitted
        assert_eq!(admitted, [true, false, false, false, false, true, false]);
    }

    #[test]
    fn a_throttle_admits_a_working_line_after_a_line_that_is_not_one() {
        // Given a throttle that admitted a working line a moment ago
        let (mut throttle, t0) = a_throttle_and_a_start();
        throttle.admit("working: loading proc-macros", t0);

        // When a phase line arrives and a working line follows it
        throttle.admit("loading crate graph", t0 + Duration::from_millis(10));
        let admitted = throttle.admit("working: 12/40 (30%)", t0 + Duration::from_millis(20));

        // Then the new phase's first working line is shown
        assert!(admitted);
    }

    #[test]
    fn a_throttle_always_admits_a_line_reporting_completion() {
        // Given a throttle that admitted a working line a moment ago
        let (mut throttle, t0) = a_throttle_and_a_start();
        throttle.admit("working: 1273/1851 (68%)", t0);

        // When the same progress reaches 100% inside the interval
        let admitted = throttle.admit("working: 1851/1851 (100%)", t0 + Duration::from_millis(5));

        // Then it is admitted
        assert!(admitted);
    }
}
