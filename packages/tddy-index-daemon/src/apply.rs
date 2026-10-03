//! The apply loop, driven here rather than by the library's own entry point.
//!
//! [`tddy_code_restructuring::runner::apply`] runs the same sequence and reports it as lines into
//! a sink. A host that called it would learn what the whole run amounted to and nothing about the
//! operations that made it up — one event per operation, with the files it touched and the
//! visibility it widened, is not something a line carries — which is why
//! [`tddy_code_restructuring::runner::open_plan_run`],
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
use tddy_code_restructuring::plan_store::PlanKey;
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::runner::{self, Options, PlanRun};
use tddy_code_restructuring::{Overlay, Resolution, Result};
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use crate::index::SharedPlanStore;
use crate::operations::{note_event, EventSender};
use crate::proto::code_index::{restructure_event, OperationApplied, RestructureEvent, RunOutcome};

/// The plan a run executes: the store that holds it, and what it is held under.
pub(crate) struct HeldPlan {
    pub(crate) store: SharedPlanStore,
    pub(crate) key: PlanKey,
}

impl HeldPlan {
    fn with_store<T>(
        &self,
        use_store: impl FnOnce(&mut tddy_code_restructuring::plan_store::PlanStore) -> T,
    ) -> T {
        use_store(&mut self.store.lock().expect("a root's plan store"))
    }
}

/// Execute the plan `held` names against the tree under `root`, reporting each operation as an
/// event.
///
/// The plan is the store's copy, not the file, and its run state is its own — keyed by the plan's
/// path — so what the caller owns is the queue for `root`: two runs under one root must not edit
/// its tree at once, whatever plans they are of. After each operation the store refreshes the plan's
/// pending anchors and writes the plan back; see [`runner::apply_from_store`], which this mirrors
/// event for line.
///
/// The store is locked for the steps that need it and not across the run, so a request to list the
/// root's plans is answered while the run is going.
pub(crate) fn apply_plan(
    root: &Path,
    held: &HeldPlan,
    options: &Options,
    client: Arc<LspClient>,
    cancel: CancellationToken,
    progress: ProgressSink,
    events: &EventSender<RestructureEvent>,
) -> Result<()> {
    apply_held_plan(root, held, options, client, &cancel, progress, events)
}

fn apply_held_plan(
    root: &Path,
    held: &HeldPlan,
    options: &Options,
    client: Arc<LspClient>,
    cancel: &CancellationToken,
    progress: ProgressSink,
    events: &EventSender<RestructureEvent>,
) -> Result<()> {
    let (plan, plan_path) = held.with_store(|store| {
        // Before the plan is read out or anything is waited for: a stale operation is refused while
        // nothing has been written.
        runner::refuse_a_stale_pending_op(store, &held.key, options)?;
        store
            .get(&held.key)
            .map(|loaded| (loaded.plan.clone(), store.path_of(&held.key)))
            .ok_or_else(|| {
                tddy_code_restructuring::RestructureError::MalformedPlan(format!(
                    "{} is not loaded — load it first",
                    held.key
                ))
            })
    })?;

    // The gates run on a copy of the plan, outside the store's lock: the baseline compile check
    // takes minutes.
    let mut registry = runner::registry_for(client, cancel.clone(), progress, logged_trace);
    let PlanRun {
        mut journal,
        plan,
        paths,
        mut ledger,
        legacy,
        start,
    } = runner::open_plan_run(&plan, &plan_path, root, options, &mut registry, cancel)?;
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
            runner::commit_operation(
                index,
                op.id.as_ref().filter(|_| !legacy),
                &resolved,
                root,
                &paths,
                &mut journal,
                &mut ledger,
            )?;
            if !legacy {
                held.with_store(|store| {
                    runner::record_applied_op(
                        store,
                        &held.key,
                        index,
                        &resolved,
                        &mut registry,
                        &mut journal,
                        &paths,
                    )
                })?;
            }
        }
        done += 1;

        // Reported *after* the commit, so an event means the edit is on disk and in the journal.
        emit(
            events,
            cancel,
            operation_event(index, done, plan.ops.len(), op, &resolved, options.dry_run),
        );
        for note in &resolved.notes {
            emit(
                events,
                cancel,
                note_event(&tddy_code_restructuring::console::note(note)),
            );
        }
    }

    // Judged before the outcome is sent, so a tree that does not compile ends the stream with the
    // refusal and never with "applied N of N" — the same gate, from the same library, as the cold
    // path's `runner::apply`.
    let run = runner::AppliedRun {
        journal: &journal,
        paths: &paths,
        applied: done,
        total: plan.ops.len(),
    };
    runner::refuse_a_broken_result(root, options, run, cancel)?;
    // Before the outcome, so a plan that could not be written back ends the stream with that and
    // never with "applied N of N". A run that failed earlier writes nothing here: the operations it
    // committed wrote the plan as they landed, and one refused before its first leaves the file as
    // it was.
    if !options.dry_run {
        held.with_store(|store| store.flush(&held.key))?;
    }
    emit(
        events,
        cancel,
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
            op_id: op.id.as_ref().map(ToString::to_string).unwrap_or_default(),
            kind: format!("{:?}", op.op),
            files: tddy_code_restructuring::apply::touched_paths(&resolved.edit),
            // Stated by the renderer rather than here, so a widening carried as a value on this
            // event and one carried in a line of the cold path's account read the same.
            visibility: resolved
                .report
                .iter()
                .map(tddy_code_restructuring::console::widening)
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
