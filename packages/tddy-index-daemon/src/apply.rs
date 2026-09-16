//! The apply loop, driven here rather than by the library's own entry point.
//!
//! [`tddy_code_restructuring::runner::apply`] runs the same sequence and reports it as lines into
//! a sink. A host that called it would learn what the whole run amounted to and nothing about the
//! operations that made it up — one event per operation, with the files it touched and the
//! visibility it widened, is not something a line carries — which is why
//! [`tddy_code_restructuring::runner::open_run`],
//! [`tddy_code_restructuring::runner::restore_ledger`] and
//! [`tddy_code_restructuring::runner::commit_operation`] were promoted to public: the write-ahead
//! sequence stays in the library, where a crash in the middle of it is still resumable, and the
//! loop around it reports events instead of lines.
//!
//! This function is synchronous by requirement, not by preference: the backend reaches its
//! language server through `Handle::current().block_on` and waits for the index with
//! `std::thread::sleep`, so it runs inside [`tokio::task::spawn_blocking`] with the runtime still
//! driving elsewhere. That is also why the cancellation token is checked *here*, between
//! operations, as well as inside those waits — dropping this work's future would not stop it.

use std::path::Path;
use std::sync::Arc;

use tddy_code_restructuring::backends::rust::ProgressSink;
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::runner::{self, Options, StatePaths};
use tddy_code_restructuring::{Overlay, Plan, Resolution, Result};
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use crate::operations::{note_event, EventSender};
use crate::proto::code_index::{restructure_event, OperationApplied, RestructureEvent, RunOutcome};

/// Execute `options`' plan against the tree under `root`, reporting each operation as an event.
///
/// The caller owns the queue for `root`: the journal this writes carries no plan identity, so two
/// runs under one root must not be in here at once.
pub(crate) fn apply_plan(
    root: &Path,
    options: &Options,
    client: Arc<LspClient>,
    cancel: CancellationToken,
    progress: ProgressSink,
    events: &EventSender<RestructureEvent>,
) -> Result<()> {
    let plan = Plan::parse(&std::fs::read_to_string(options.plan()?)?)?;
    let paths = StatePaths::under(root);

    let mut journal = runner::open_run(&plan, root, &paths, options)?;
    let mut ledger = runner::restore_ledger(&journal, &paths)?;
    let mut registry = runner::registry_for(client, cancel.clone(), progress, logged_trace);
    let start = options.from.unwrap_or_else(|| journal.next_op());
    let mut overlay = Overlay::new();
    let mut done = 0usize;
    let mut stopped_early = false;

    for (index, op) in plan.ops.iter().enumerate().skip(start) {
        // Checked before the operation rather than only inside the index waits: the client that
        // asked for this run has gone, and writing the rest of its plan into the tree anyway is
        // the opposite of what its disconnect asked for. The journal makes the remainder resumable.
        if cancel.is_cancelled() {
            log::info!(
                target: "tddy_index_daemon::apply",
                "stopping after {done} operation(s): nobody is waiting for this run any more"
            );
            break;
        }
        // Honouring `stop_after` is the run doing what it was told, so it ends the loop rather
        // than raising: a successful partial run is not a defective one.
        if options
            .stop_after
            .is_some_and(|limit| index >= start + limit)
        {
            stopped_early = true;
            break;
        }

        let anchor = ledger.translate_anchor(&op.anchor)?;
        let resolved = registry
            .backend_for(Path::new(anchor.file()), op.op)?
            .resolve(
                &op.with_anchor(anchor),
                &Workspace {
                    root,
                    overlay: &overlay,
                },
            )?;

        if options.dry_run {
            ledger.record(&resolved.edit);
            overlay.record(root, &resolved.edit)?;
        } else {
            runner::commit_operation(index, &resolved, root, &paths, &mut journal, &mut ledger)?;
        }
        done += 1;

        // Reported *after* the commit, so an event means the edit is on disk and in the journal.
        emit(
            events,
            &cancel,
            operation_event(index, done, plan.ops.len(), op, &resolved, options.dry_run),
        );
        for note in &resolved.notes {
            emit(events, &cancel, note_event(note));
        }
    }

    emit(
        events,
        &cancel,
        outcome_event(done, plan.ops.len(), stopped_early),
    );
    Ok(())
}

/// Where a seam's diagnostic trace goes when `RESTRUCTURE_TRACE` asks for one. The log, because a
/// line this process writes to stdout is a line in somebody's RPC frame.
fn logged_trace(line: &str) {
    log::debug!(target: "tddy_index_daemon::apply", "trace: {line}");
}

/// What one applied operation amounted to.
fn operation_event(
    index: usize,
    done: usize,
    total: usize,
    op: &tddy_code_restructuring::RefactorOp,
    resolved: &Resolution,
    dry_run: bool,
) -> RestructureEvent {
    RestructureEvent {
        event: Some(restructure_event::Event::Operation(OperationApplied {
            index: index as u32,
            done: done as u32,
            total: total as u32,
            kind: format!("{:?}", op.op),
            files: tddy_code_restructuring::apply::touched_paths(&resolved.edit),
            visibility: resolved
                .report
                .iter()
                .map(|widened| format!("`{}` {} -> {}", widened.item, widened.from, widened.to))
                .collect(),
            rehearsed_only: dry_run,
        })),
    }
}

/// The terminal event: what the whole run amounted to.
fn outcome_event(applied: usize, total: usize, stopped_early: bool) -> RestructureEvent {
    RestructureEvent {
        event: Some(restructure_event::Event::Outcome(RunOutcome {
            applied: applied as u32,
            total: total as u32,
            stopped_early,
        })),
    }
}

/// Send one event to the caller who asked for this run, and cancel the run if they have gone.
///
/// `blocking_send` because this is a blocking thread by construction, so waiting for a slow
/// consumer is the backpressure that keeps the account of a run complete. A failed send means
/// every receiver has been dropped, which is the only disconnect signal a handler gets.
fn emit(
    events: &EventSender<RestructureEvent>,
    cancel: &CancellationToken,
    event: RestructureEvent,
) {
    if events.blocking_send(Ok(event)).is_err() {
        cancel.cancel();
    }
}
