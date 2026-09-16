//! What a single-shot run tells the operator, and where it tells them.
//!
//! **Through `log`, never through a print macro.** fd 1 belongs to RPC framing whenever this
//! process serves `--stdio`, and a binary that reaches for `println!` in one mode has already lost
//! the discipline in the other — `tddy_code_restructuring::runner` was made silent for exactly
//! this reason, so reintroducing the problem one layer up would undo it. The logger's default
//! destination is stderr (`tddy_core::default_log_config`), which is where these lines land.
//!
//! The shapes are `restructure_cli::report`'s, deliberately: the same operation rendered by two
//! front ends should read the same to the operator who runs both.

use tddy_index_daemon::proto::code_index::{
    analyze_event, restructure_event, AnalyzeEvent, AnchorsResponse, ComplexityResponse,
    DuplicateTestsFound, OperationApplied, PlanStatusResponse, ReportResponse, RestructureEvent,
    RunOutcome, SourceRange, VerifyResponse,
};

/// One event a running operation reported.
pub(crate) fn restructure(event: &RestructureEvent) {
    match &event.event {
        Some(restructure_event::Event::Indexing(progress)) => {
            log::info!(target: crate::MAIN, "   indexing: {}", progress.line);
        }
        Some(restructure_event::Event::Operation(operation)) => applied(operation),
        Some(restructure_event::Event::Note(note)) => {
            log::info!(target: crate::MAIN, "{note}");
        }
        Some(restructure_event::Event::Outcome(outcome)) => ran(outcome),
        Some(restructure_event::Event::Finding(finding)) => {
            log::info!(target: crate::MAIN, "{}: {}", finding.operation, finding.detail);
        }
        // The service sets exactly one field on every event it sends, so an empty one means this
        // process and the one that produced it disagree about the schema.
        None => log::warn!(target: crate::MAIN, "an event carrying no field at all"),
    }
}

/// What one operation of a plan amounted to, and what it had to widen to get there.
fn applied(operation: &OperationApplied) {
    log::info!(
        target: crate::MAIN,
        "[{}/{}] op {}: {} -> {} file(s) {}",
        operation.done,
        operation.total,
        operation.index,
        operation.kind,
        operation.files.len(),
        if operation.rehearsed_only {
            "resolved"
        } else {
            "applied"
        }
    );
    for widened in &operation.visibility {
        log::info!(target: crate::MAIN, "   visibility: {widened}");
    }
}

/// What the whole run amounted to.
fn ran(outcome: &RunOutcome) {
    if outcome.stopped_early {
        log::info!(
            target: crate::MAIN,
            "   stopped after {} operations as requested",
            outcome.applied
        );
    }
    log::info!(
        target: crate::MAIN,
        "{} of {} operations",
        outcome.applied,
        outcome.total
    );
}

/// How many findings a check made, which is also what makes it a failed run.
pub(crate) fn findings(counted: usize) {
    if counted == 0 {
        log::info!(target: crate::MAIN, "no findings");
        return;
    }
    log::error!(
        target: crate::MAIN,
        "{counted} finding(s) — see above. Nothing was written."
    );
}

/// The anchor a run of items sits at, as the JSON document a plan carries it as.
///
/// JSON rather than prose because this answer is written to be pasted into a plan, which is the
/// form `restructure_cli` emits it in too.
pub(crate) fn anchors(response: &AnchorsResponse) {
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
        serde_json::json!({
            "kind": "range",
            "start": { "line": start.line, "col": start.column },
            "end": { "line": end.line, "col": end.column }
        })
    );
}

/// How far a plan's journal got.
pub(crate) fn plan_status(response: &PlanStatusResponse) {
    log::info!(target: crate::MAIN, "completed {}", response.completed);
    log::info!(target: crate::MAIN, "in_flight {}", response.in_flight);
    log::info!(target: crate::MAIN, "failed {}", response.failed);
    log::info!(target: crate::MAIN, "pending {}", response.pending);
}

/// What holding the tree against a git ref found, and whether it held.
pub(crate) fn verify(response: &VerifyResponse) {
    log::info!(
        target: crate::MAIN,
        "{} statements before, {} after",
        response.before,
        response.after
    );
    for statement in &response.missing {
        log::info!(target: crate::MAIN, "missing: {statement}");
    }
    for statement in &response.added {
        log::info!(target: crate::MAIN, "added:   {statement}");
    }

    if response.holds {
        log::info!(target: crate::MAIN, "every statement accounted for");
        return;
    }
    log::error!(
        target: crate::MAIN,
        "{} statement(s) the tree lost and {} it gained — see above",
        response.missing.len(),
        response.added.len()
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
