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
    restructure_event, AnchorsResponse, OperationApplied, PlanStatusResponse, RestructureEvent,
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

/// A refusal, as the thing that failed the run.
pub(crate) fn refusal(status: &tddy_rpc::Status) {
    log::error!(target: crate::MAIN, "{status}");
}
