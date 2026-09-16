//! What a run served by the warm index daemon looks like on this binary's console.
//!
//! **The shapes are [`tddy_code_restructuring::console`]'s, and they are called rather than
//! re-stated**: a developer who exports `TDDY_INDEX_SOCKET` must read the same account of the same
//! operation as one who does not, or the variable changes more than where the work happens.
//!
//! What remains here, and is all that ever should have, is the mapping and the destination. The
//! daemon streams *events* — prost messages shaped for a wire — and this maps each one onto the
//! library type the renderer speaks: `RunOutcome` to a `RunSummary`, `PlanStatusResponse` to a
//! `PlanProgress`, `VerifyResponse` to a `Comparison`, `SourceRange` to a `Range`. Then it writes
//! the lines where this front end writes them.
//!
//! Two destinations, and they must match `restructure_cli::install_console`'s exactly: the answer
//! — findings, per-operation lines, the summary — goes to **stdout**, and the server's narration of
//! how far it got goes **aside, to stderr**. The rule is not cosmetic. A test pins the whole stdout
//! vector of a `check --budget` run against the literal lines the cold path's own suite pins, so
//! the two front ends cannot drift apart silently. It caught this: when #500's run-level narration
//! was merged in, the cold path started sending it to stderr and this renderer was still putting it
//! on stdout, and the assertion failed with the three extra lines.
//!
//! The elapsed-time stamp is the run's own, measured here against this run's clock and handed to
//! [`tddy_code_restructuring::restructure_cli::step_delta`]: it is how long *this console* has been
//! waiting, which is the question its reader is asking.

use std::time::Instant;

use anyhow::Result;
use tddy_code_restructuring::console;
use tddy_code_restructuring::restructure_cli::step_delta;
use tddy_code_restructuring::runner::{PlanProgress, RunSummary};
use tddy_code_restructuring::verify::Comparison;
use tddy_code_restructuring::{Position, Range};
use tddy_index_daemon::proto::code_index::{
    restructure_event, AnchorsResponse, Finding, IndexProgress, OperationApplied,
    PlanStatusResponse, RestructureEvent, RunOutcome, SourceRange, VerifyResponse,
};

/// One line of a run's answer, on the console this front end owns.
fn say(line: &str) {
    println!("{line}");
}

/// One line of the server's narration, beside the answer rather than in it.
fn aside(line: &str) {
    eprintln!("{line}");
}

/// What a run has been told so far, and therefore what it amounts to.
///
/// Findings are counted rather than collected: a check with findings is an *answered* call whose
/// answer is a failed run, so the count is what becomes this process's exit status — the judgement
/// the cold path makes in its own `verdict_on`, made here for the same reason and worded by
/// [`console::findings_refusal`].
///
/// `Anchors` needs no special case here, unlike the cold path — it is a unary RPC, so a client
/// receives a range and no account at all; the waiting the cold path narrates happens inside the
/// daemon and goes to the daemon's log.
pub(crate) struct Rendered {
    /// True when the run was a rehearsal, which decides whether it "resolved" or "applied".
    rehearsal: bool,
    findings: usize,
    /// When this run last narrated something, which is what the next line's stamp is measured
    /// against.
    ///
    /// The clock belongs to the run, exactly as the cold path's belongs to its sink: one
    /// `Rendered` per run, so two runs in one process — or two runs served by one daemon — cannot
    /// interleave their deltas through a shared instant. `None` until the first line, which is
    /// therefore stamped `+0ms` rather than with the age of the process.
    narrated: Option<Instant>,
}

impl Rendered {
    pub(crate) fn new(rehearsal: bool) -> Self {
        Self {
            rehearsal,
            findings: 0,
            narrated: None,
        }
    }

    /// One event a running operation reported.
    pub(crate) fn event(&mut self, event: &RestructureEvent) -> Result<()> {
        match &event.event {
            Some(restructure_event::Event::Indexing(progress)) => self.indexing(progress),
            Some(restructure_event::Event::Operation(operation)) => self.operation(operation),
            Some(restructure_event::Event::Note(note)) => say(note),
            Some(restructure_event::Event::Outcome(outcome)) => self.outcome(outcome),
            Some(restructure_event::Event::Finding(finding)) => self.finding(finding),
            // The service sets exactly one field on every event it sends, so an empty one means
            // this process and the one that produced it disagree about the schema. Refused rather
            // than skipped: a run whose account has a hole in it is not a run that was reported.
            None => {
                anyhow::bail!(
                    "the index daemon sent an event carrying no field at all: this tddy-tools and \
                     that daemon disagree about code_index.proto"
                )
            }
        }
        Ok(())
    }

    /// One line of the server's narration, stamped with the time since the line before it.
    ///
    /// The stamp is #500's and the reason is #500's: on a six-to-ten-minute crate-graph load it is
    /// the only thing distinguishing a run making progress from one that has hung, so a developer
    /// who exported `TDDY_INDEX_SOCKET` must be given it too. The elapsed time is measured *here*,
    /// against this run's own clock, rather than carried on the event: it is how long this console
    /// has been waiting, which is the question a reader of it is asking.
    fn indexing(&mut self, progress: &IndexProgress) {
        let now = Instant::now();
        let stamp = step_delta(self.narrated, now);
        self.narrated = Some(now);
        aside(&console::narration(
            "indexing",
            Some(&stamp),
            &progress.line,
        ));
    }

    /// What one operation of a plan amounted to, and what it had to widen to get there.
    ///
    /// The widenings come off the event already stated — the daemon's apply loop states each one
    /// with `console::widening` — so all that is left is the account's own prefix.
    fn operation(&self, operation: &OperationApplied) {
        for widened in &operation.visibility {
            say(&console::visibility(widened));
        }
        say(&console::operation(
            operation.index as usize,
            // The event carries how many operations are *done*, which is already this one
            // included; the renderer counts from the one before it, as the cold path's loop does.
            (operation.done as usize).saturating_sub(1),
            operation.total as usize,
            &operation.kind,
            operation.files.len(),
            !operation.rehearsed_only,
        ));
    }

    /// What the whole run amounted to.
    fn outcome(&self, outcome: &RunOutcome) {
        for line in console::run_summary(&a_run_summary(outcome), self.rehearsal) {
            say(&line);
        }
    }

    fn finding(&mut self, finding: &Finding) {
        self.findings += 1;
        say(&console::finding(&a_finding(finding)));
    }

    /// Whether a check that ran to the end of its stream is a successful run.
    pub(crate) fn verdict_on_findings(&self) -> Result<()> {
        if self.findings == 0 {
            say(console::NO_FINDINGS);
            return Ok(());
        }
        Err(anyhow::anyhow!(console::findings_refusal(self.findings)))
    }
}

/// How far a plan's journal got.
pub(crate) fn plan_status(response: &PlanStatusResponse) {
    for line in console::plan_progress(&PlanProgress {
        completed: response.completed as usize,
        in_flight: response.in_flight as usize,
        pending: response.pending as usize,
        failed: response.failed as usize,
    }) {
        say(&line);
    }
}

/// What holding the tree against a git ref found, and whether it held.
pub(crate) fn verify(response: &VerifyResponse) -> Result<()> {
    let comparison = a_comparison(response);
    for line in console::comparison(&comparison) {
        say(&line);
    }
    if comparison.holds() {
        return Ok(());
    }
    Err(anyhow::anyhow!(console::comparison_refusal(&comparison)))
}

/// The anchor a run of items sits at, as the JSON document a plan carries it as.
///
/// `file` comes from the command line rather than from the response: the schema's
/// `AnchorsResponse` carries the range alone, and the document this front end has always emitted
/// names the file the range is in.
pub(crate) fn anchors(file: &str, response: &AnchorsResponse) -> Result<()> {
    let Some(SourceRange {
        start: Some(start),
        end: Some(end),
    }) = &response.range
    else {
        anyhow::bail!("the index daemon answered the anchor without a range");
    };
    say(&console::anchor(
        file,
        Range {
            start: Position {
                line: start.line,
                col: start.column,
            },
            end: Position {
                line: end.line,
                col: end.column,
            },
        },
    ));
    Ok(())
}

/// The run summary the renderer speaks, out of the event the daemon sent.
fn a_run_summary(outcome: &RunOutcome) -> RunSummary {
    RunSummary {
        applied: outcome.applied as usize,
        total: outcome.total as usize,
        stopped_early: outcome.stopped_early,
    }
}

/// The finding the renderer speaks, out of the event the daemon sent.
fn a_finding(finding: &Finding) -> tddy_code_restructuring::runner::Finding {
    tddy_code_restructuring::runner::Finding {
        operation: finding.operation as usize,
        detail: finding.detail.clone(),
    }
}

/// The comparison the renderer speaks, out of the answer the daemon sent.
///
/// `holds` is recomputed from the two lists rather than read off the wire: the field is the
/// service's own judgement of its own answer, and a renderer that trusted it while printing the
/// lists could state a verdict its own output contradicts.
fn a_comparison(response: &VerifyResponse) -> Comparison {
    Comparison {
        before: response.before as usize,
        after: response.after as usize,
        missing: response.missing.clone(),
        added: response.added.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A check with findings is an answered call whose answer is a failed run, which is what
    /// becomes the exit status — the same judgement the cold path makes.
    #[test]
    fn a_check_that_found_something_is_a_failed_run_naming_how_much_it_found() {
        // Given a rendered check that was told about two findings
        let mut rendered = Rendered::new(false);
        rendered
            .event(&finding_event(0, "the anchor names no such symbol"))
            .expect("the event renders");
        rendered
            .event(&finding_event(1, "the move would introduce a cycle"))
            .expect("the event renders");

        // When its verdict is read
        let verdict = rendered.verdict_on_findings();

        // Then the run failed, and the refusal says how many findings there were
        assert_eq!(
            verdict.expect_err("findings fail the run").to_string(),
            "2 finding(s) — see above. Nothing was written."
        );
    }

    #[test]
    fn a_check_that_found_nothing_is_a_successful_run() {
        // Given a rendered check that was told about no findings
        let rendered = Rendered::new(false);

        // When its verdict is read
        // Then the run succeeded
        assert!(rendered.verdict_on_findings().is_ok());
    }

    /// An event with no field set means this binary and the daemon disagree about the schema, and a
    /// run whose account has a hole in it has not been reported.
    #[test]
    fn refuses_an_event_that_carries_no_field_at_all() {
        // Given an event the daemon sent with nothing in it
        let mut rendered = Rendered::new(false);

        // When it is rendered
        let outcome = rendered.event(&RestructureEvent { event: None });

        // Then the run is refused, naming the disagreement
        assert!(
            outcome
                .expect_err("an empty event is refused")
                .to_string()
                .contains("disagree about code_index.proto"),
            "the refusal did not name the schema disagreement"
        );
    }

    fn finding_event(operation: u32, detail: &str) -> RestructureEvent {
        RestructureEvent {
            event: Some(restructure_event::Event::Finding(Finding {
                operation,
                detail: detail.to_string(),
            })),
        }
    }
}
