//! What a single-shot run tells the operator, and where it tells them.
//!
//! **Through `log`, never through a print macro.** fd 1 belongs to RPC framing whenever this
//! process serves `--stdio`, and a binary that reaches for `println!` in one mode has already lost
//! the discipline in the other — `tddy_code_restructuring::runner` was made silent for exactly
//! this reason, so reintroducing the problem one layer up would undo it. The logger's default
//! destination is stderr (`tddy_core::default_log_config`), which is where these lines land.
//!
//! The shapes are [`tddy_code_restructuring::console`]'s, and they are **called rather than
//! re-stated**: the same operation rendered by two front ends should read the same to the operator
//! who runs both. What is this module's own is the destination and the level — an answer at INFO,
//! a refusal at ERROR — plus the mapping from the wire messages onto the library types the
//! renderer speaks.

use tddy_code_restructuring::console;
use tddy_code_restructuring::runner::{PlanProgress, RunSummary};
use tddy_code_restructuring::verify::Comparison;
use tddy_code_restructuring::{Position, Range};
use tddy_index_daemon::proto::code_index::{
    analyze_event, restructure_event, AnalyzeEvent, AnchorsResponse, ComplexityResponse,
    DuplicateTestsFound, OperationApplied, PlanStatusResponse, ReportResponse, RestructureEvent,
    RunOutcome, SourceRange, VerifyResponse,
};

/// One event a running operation reported.
///
/// `rehearsal` is the run's own `--dry-run`, carried in because the event vocabulary does not hold
/// it: a summary that said "applied" for a run which wrote nothing would be the one line of this
/// account an operator could act wrongly on.
pub(crate) fn restructure(event: &RestructureEvent, rehearsal: bool) {
    match &event.event {
        Some(restructure_event::Event::Indexing(progress)) => {
            // No per-line stamp: these are log lines, and the log already timestamps them.
            log::info!(target: crate::MAIN, "{}", console::narration("indexing", None, &progress.line));
        }
        Some(restructure_event::Event::Operation(operation)) => applied(operation),
        Some(restructure_event::Event::Note(note)) => {
            log::info!(target: crate::MAIN, "{note}");
        }
        Some(restructure_event::Event::Outcome(outcome)) => ran(outcome, rehearsal),
        Some(restructure_event::Event::Finding(finding)) => {
            log::info!(
                target: crate::MAIN,
                "{}",
                console::finding(&tddy_code_restructuring::runner::Finding {
                    operation: finding.operation as usize,
                    detail: finding.detail.clone(),
                })
            );
        }
        // The service sets exactly one field on every event it sends, so an empty one means this
        // process and the one that produced it disagree about the schema.
        None => log::warn!(target: crate::MAIN, "an event carrying no field at all"),
    }
}

/// What one operation of a plan amounted to, and what it had to widen to get there.
///
/// The widenings come first, as they do in both other front ends: they are what the operation had
/// to do to reach the line that follows.
fn applied(operation: &OperationApplied) {
    for widened in &operation.visibility {
        log::info!(target: crate::MAIN, "{}", console::visibility(widened));
    }
    log::info!(
        target: crate::MAIN,
        "{}",
        console::operation(
            operation.index as usize,
            // The event counts this operation as done already; the renderer counts from the one
            // before it, as the apply loop that produces the line does.
            (operation.done as usize).saturating_sub(1),
            operation.total as usize,
            &operation.kind,
            operation.files.len(),
            !operation.rehearsed_only,
        )
    );
}

/// What the whole run amounted to.
fn ran(outcome: &RunOutcome, rehearsal: bool) {
    for line in console::run_summary(
        &RunSummary {
            applied: outcome.applied as usize,
            total: outcome.total as usize,
            stopped_early: outcome.stopped_early,
        },
        rehearsal,
    ) {
        log::info!(target: crate::MAIN, "{line}");
    }
}

/// How many findings a check made, which is also what makes it a failed run.
pub(crate) fn findings(counted: usize) {
    if counted == 0 {
        log::info!(target: crate::MAIN, "{}", console::NO_FINDINGS);
        return;
    }
    log::error!(target: crate::MAIN, "{}", console::findings_refusal(counted));
}

/// The anchor a run of items sits at, as the JSON document a plan carries it as.
///
/// JSON rather than prose because this answer is written to be pasted into a plan. `file` comes
/// from the request rather than the answer — the schema's `AnchorsResponse` carries the range
/// alone — because a range without the file it is in is not an anchor, and the document the other
/// front ends emit names it.
pub(crate) fn anchors(file: &str, response: &AnchorsResponse) {
    let Some(SourceRange {
        start: Some(start),
        end: Some(end),
    }) = &response.range
    else {
        log::error!(target: crate::MAIN, "the anchor came back without a range");
        return;
    };
    log::info!(
        target: crate::MAIN,
        "{}",
        console::anchor(
            file,
            Range {
                start: Position { line: start.line, col: start.column },
                end: Position { line: end.line, col: end.column },
            }
        )
    );
}

/// How far a plan's journal got.
pub(crate) fn plan_status(response: &PlanStatusResponse) {
    for line in console::plan_progress(&PlanProgress {
        completed: response.completed as usize,
        in_flight: response.in_flight as usize,
        pending: response.pending as usize,
        failed: response.failed as usize,
    }) {
        log::info!(target: crate::MAIN, "{line}");
    }
}

/// What holding the tree against a git ref found, and whether it held.
///
/// `holds` is recomputed from the two lists rather than read off the wire: the field is the
/// service's judgement of its own answer, and a renderer that trusted it while printing the lists
/// could state a verdict its own output contradicts.
pub(crate) fn verify(response: &VerifyResponse) {
    let comparison = Comparison {
        before: response.before as usize,
        after: response.after as usize,
        missing: response.missing.clone(),
        added: response.added.clone(),
    };
    for line in console::comparison(&comparison) {
        log::info!(target: crate::MAIN, "{line}");
    }
    if comparison.holds() {
        return;
    }
    log::error!(
        target: crate::MAIN,
        "{}",
        console::comparison_refusal(&comparison)
    );
}

/// One event a running analysis reported.
///
/// The counts are what distinguishes a long capture from a hung one, which is the reason the RPC
/// streams at all — so every phase the library reports is rendered rather than summarised.
pub(crate) fn analyze(event: &AnalyzeEvent) {
    match &event.event {
        Some(analyze_event::Event::BuildStarted(_)) => {
            log::info!(
                target: crate::MAIN,
                "building instrumented tests — minutes, with nothing to report until it ends"
            );
        }
        Some(analyze_event::Event::BuildFinished(built)) => {
            log::info!(target: crate::MAIN, "{} harness(es) to run", built.harnesses);
        }
        Some(analyze_event::Event::HarnessStarted(harness)) => {
            log::info!(
                target: crate::MAIN,
                "[{}/{}] {}: {} test(s)",
                harness.index,
                harness.total,
                harness.spec,
                harness.tests
            );
        }
        Some(analyze_event::Event::TestCaptured(test)) => {
            log::info!(
                target: crate::MAIN,
                "   [{}/{}] {} {}",
                test.index,
                test.total,
                test.name,
                test.status
            );
        }
        Some(analyze_event::Event::CaptureFinished(captured)) => {
            log::info!(
                target: crate::MAIN,
                "captured {} test(s) over {} file(s)",
                captured.tests,
                captured.files
            );
        }
        Some(analyze_event::Event::DuplicateTests(found)) => duplicates(found),
        // The service sets exactly one field on every event it sends, so an empty one means this
        // process and the one that produced it disagree about the schema.
        None => log::warn!(target: crate::MAIN, "an event carrying no field at all"),
    }
}

/// Everything the duplicate detection found.
fn duplicates(found: &DuplicateTestsFound) {
    for group in &found.identical {
        log::info!(
            target: crate::MAIN,
            "identical ({} keys): {}",
            group.signature_size,
            group.tests.join(", ")
        );
    }
    for relation in &found.subsets {
        log::info!(
            target: crate::MAIN,
            "subset ({:.2}): {} ⊂ {}",
            relation.ratio,
            relation.subset,
            relation.superset
        );
    }
    log::info!(
        target: crate::MAIN,
        "{} identical group(s), {} subset relation(s)",
        found.identical.len(),
        found.subsets.len()
    );
}

/// What a report joined, and where it wrote its leaderboard.
///
/// The join rate leads because it is what says whether the leaderboard describes the tree: well
/// below 1 means the join is missing sources, so the scores are over part of it.
pub(crate) fn report(response: &ReportResponse) {
    log::info!(
        target: crate::MAIN,
        "CRAP join: {:.1}% ({} matched, {} unmatched)",
        response.join_rate * 100.0,
        response.matched,
        response.unmatched
    );
    log::info!(target: crate::MAIN, "{}", response.report_path);
}

/// Every function one file holds, with the complexity its branches earn.
pub(crate) fn complexity(response: &ComplexityResponse) {
    for function in &response.functions {
        log::info!(
            target: crate::MAIN,
            "{}:{} {}",
            function.line,
            function.complexity,
            function.name
        );
    }
    log::info!(
        target: crate::MAIN,
        "{} function(s) scored",
        response.functions.len()
    );
}

/// The operator stopped the run, which the service learns from the stream going away.
pub(crate) fn interrupted() {
    log::error!(
        target: crate::MAIN,
        "interrupted — the work was asked to stop, and stops at the end of the unit it is in"
    );
}

/// A refusal, as the thing that failed the run.
pub(crate) fn refusal(status: &tddy_rpc::Status) {
    log::error!(target: crate::MAIN, "{status}");
}
