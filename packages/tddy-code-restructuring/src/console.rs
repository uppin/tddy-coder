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

use crate::edit::{Range, VisibilityChange};
use crate::runner::{Finding, Outcome, PlanProgress, RunSummary};
use crate::verify::Comparison;

/// Every line a whole run's result amounts to, in the order a reader reads them.
///
/// One arm per [`Outcome`] variant, because the five entry points answer five different questions.
/// A front end holding an `Outcome` — from a library call, or folded out of a stream of events —
/// writes these wherever it writes.
pub fn outcome(outcome: &Outcome, rehearsal: bool) -> Vec<String> {
    match outcome {
        Outcome::Applied(summary) => run_summary(summary, rehearsal),
        Outcome::Status(progress) => plan_progress(progress),
        Outcome::Checked(found) => findings(found),
        Outcome::Anchored { file, range } => vec![anchor(file, *range)],
        Outcome::Verified(comparison) => self::comparison(comparison),
    }
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

/// The anchor a run of items sits at, as the JSON document a plan carries it as.
///
/// JSON rather than prose because this answer is written to be pasted into a plan. `file` is
/// carried alongside the range because a range without the file it is in is not an anchor.
pub fn anchor(file: &str, range: Range) -> String {
    serde_json::json!({
        "kind": "range",
        "file": file,
        "start": { "line": range.start.line, "col": range.start.col },
        "end": { "line": range.end.line, "col": range.end.col }
    })
    .to_string()
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
}
