use super::super::refuse_a_broken_result;
use crate::Result;

use super::super::commit_operation;

use super::progress_line;

use super::report_visibility;

use crate::{
    plan_store::PlanStore,
    registry::Workspace,
    runner::{
        entry_points::anchor_entry_points, group_gate::GroupRun, restore_ledger, resume,
        AppliedRun, StatePaths,
    },
    BackendRegistry, Journal,
};

use crate::Overlay;

use super::registry_for;

use super::super::refuse_a_broken_baseline;

use crate::PositionLedger;

use crate::Plan;

use crate::RestructureError;

use super::super::RunSummary;

use super::super::Finding;

use tokio_util::sync::CancellationToken;

use tddy_lsp::client::LspClient;

use std::sync::Arc;

use super::super::Options;

use crate::plan_store::PlanKey;

use std::path::Path;

/// Apply the plan `key` names from `store`, starting at `from` (or where its journal left off),
/// refreshing the plan's pending operations after each one and flushing it at the end.
///
/// What every front end runs: the daemon over its long-lived store, a one-shot `apply` over a store
/// that lives for the run. The plan is the store's copy, not the file: `options.target` names
/// nothing here, and a file edited since it was loaded changes nothing about what runs.
///
/// **The store's anchors are current.** After each committed operation the plan's pending operations
/// are rewritten for the tree it left and the plan is written back, so what the file says is what
/// the tree holds as of the last completed operation. A run therefore translates anchors through the
/// edits *of this run* only — a ledger folded from the journal would carry the edits of earlier runs
/// a second time — and a resume reads anchors that match the tree it resumes on. The write follows
/// the journal's record of the operation, so a crash between the two leaves the file one operation
/// behind the journal.
///
/// A dry run writes nothing: no journal record, no refresh, no flush. Neither does a run that is
/// refused before its first operation: the plan file is as it was.
///
/// # Errors
///
/// [`RestructureError::PlanChangedOnDisk`] when the plan's file changed since it was loaded, at the
/// first write. The operations committed before it stay on disk and in the journal.
pub fn apply_from_store(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("apply requires a rust-analyzer LSP session".into())
    })?;
    refuse_a_stale_pending_op(store, key, &options)?;
    let summary = apply_held_plan(root, store, key, &options, client, &cancel)?;
    // A run that refused or failed writes nothing here: the operations it did commit wrote the plan
    // as they landed ([`record_applied_op`]), and one that never started must leave the plan file
    // as it found it — "nothing was written" is what its refusal says.
    if !options.dry_run {
        store.flush(key)?;
    }
    Ok(summary)
}

/// A run that has been opened: its journal, the plan it executes, and where in it to start.
///
/// What [`open_plan_run`] hands back, so the loop that drives it — the command line's, which
/// reports lines, and the daemon's, which reports events — starts from the same state.
pub struct PlanRun {
    pub journal: Journal,
    /// The plan to execute, every item anchor lowered into the range it names on the tree the run
    /// starts on.
    pub plan: Plan,
    pub paths: StatePaths,
    /// Translates `plan`'s anchors: through the edits **this run** commits and nothing earlier, since
    /// the plan was written back to match the tree — unless the journal `legacy`, in which case it
    /// starts from the journal's own fold. See [`apply_from_store`].
    pub ledger: PositionLedger,
    /// The journal was written before plans were kept current, so this run continues it as such: its
    /// operations are journalled without ids and the plan is neither refreshed nor written back —
    /// mixing the two epochs in one plan would leave anchors that match neither.
    pub legacy: bool,
    /// The index of the first operation to execute.
    pub start: usize,
}

/// Open the run of `plan`, a plan a store holds at `plan_path`: the journal, with its refusals, and
/// item anchors lowered, in the one order every apply loop uses ([`open_run_resolving_anchors`]).
///
/// Takes the plan rather than the store, so a host whose store is shared can copy the plan out and
/// not hold the store through a baseline compile check that takes minutes.
pub fn open_plan_run(
    plan: &Plan,
    plan_path: &Path,
    root: &Path,
    options: &Options,
    registry: &mut BackendRegistry,
    cancel: &CancellationToken,
) -> Result<PlanRun> {
    let paths = StatePaths::for_plan(root, plan_path)?;
    // Item anchors resolve, and then the baseline compile check runs, both before `.restructure/`
    // exists — see `open_run_resolving_anchors` for why in that order.
    let (journal, lowered) = anchor_entry_points::open_run_resolving_anchors(
        plan,
        root,
        &paths,
        options,
        registry,
        || refuse_a_broken_baseline(root, plan, options, cancel),
    )?;
    // The checkpoint must agree with the journal either way. What translates anchors differs:
    // a journal that predates write-back left the plan in the coordinates the run began in, so the
    // journal's own fold does it; any other leaves the plan current, and what translates is this
    // run's edits alone.
    let folded = restore_ledger(&journal, &paths)?;
    let legacy = resume::predates_plan_write_back(&journal);
    let start = resume::start_of(&lowered, options, &journal)?;
    Ok(PlanRun {
        journal,
        plan: lowered,
        paths,
        ledger: if legacy {
            folded
        } else {
            PositionLedger::new()
        },
        legacy,
        start,
    })
}

fn apply_held_plan(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: &Options,
    client: Arc<LspClient>,
    cancel: &CancellationToken,
) -> Result<RunSummary> {
    let plan = store
        .get(key)
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
        })?
        .plan
        .clone();
    let mut registry = registry_for(
        client,
        cancel.clone(),
        Arc::clone(&options.progress),
        options.trace,
    );
    let PlanRun {
        mut journal,
        plan,
        paths,
        mut ledger,
        legacy,
        start,
    } = open_plan_run(
        &plan,
        &store.path_of(key),
        root,
        options,
        &mut registry,
        cancel,
    )?;
    let total = plan.ops.len();
    (options.progress)(&format!(
        "apply: {total} operation(s){}{}",
        if options.dry_run { ", dry-run" } else { "" },
        if start > 0 {
            format!(", from op {start}")
        } else {
            String::new()
        }
    ));
    let mut overlay = Overlay::new();
    let mut done = 0usize;
    let mut stopped_early = false;
    let mut group: Option<GroupRun> = None;

    for (index, op) in plan.ops.iter().enumerate().skip(start) {
        // Honouring `--stop-after` is the run doing what it was told, so it ends the loop rather
        // than raising. Reporting it as a malformed plan — with a usage dump — described a
        // successful partial run as a defective one. Never inside a group, though: a group stands
        // or falls whole, so the limit is judged where a group would begin and its members all
        // count toward it.
        if group.is_none()
            && options
                .stop_after
                .is_some_and(|limit| index >= start + limit)
        {
            (options.progress)(&format!(
                "stopped after {} operation(s) as requested",
                index - start
            ));
            stopped_early = true;
            break;
        }

        let at = ledger.translate_op(op)?;
        (options.progress)(&format!(
            "op {index} of {total}: resolving {:?} in `{}`",
            op.op,
            at.anchor.file()
        ));
        let resolved = registry
            .backend_for(Path::new(at.anchor.file()), op.op)?
            .resolve(
                &at,
                &Workspace {
                    root,
                    overlay: &overlay,
                },
            )?;

        report_visibility(&options.account, &resolved);

        let files = resolved.edit.changes.len();
        (options.progress)(&format!("op {index} of {total}: resolved {files} file(s)"));
        if options.dry_run {
            (options.account)(&progress_line(
                index,
                done,
                plan.ops.len(),
                op.op,
                files,
                false,
            ));
            ledger.record(&resolved.edit);
            overlay.record(root, &resolved.edit)?;
            done += 1;
            continue;
        }

        (options.progress)(&format!(
            "op {index} of {total}: applying {files} file(s) to disk"
        ));
        let id = op.id.as_ref().filter(|_| !legacy);
        if group.is_none() {
            group = GroupRun::begin(&plan, index, &paths, &mut journal)?;
        }
        if let Some(open) = group.as_mut() {
            open.pre_image(index, id, &resolved, root, &paths, &mut journal)?;
        }
        commit_operation(
            index,
            id,
            &resolved,
            root,
            &paths,
            &mut journal,
            &mut ledger,
        )?;
        match group.take() {
            // A group's members reach the plan store together, once the group has compiled.
            Some(mut open) => {
                open.applied(index, resolved.clone());
                if open.closes_at(index) {
                    (options.progress)(&format!(
                        "op {index} of {total}: checking group `{}` compiles",
                        open.name()
                    ));
                    for (member, applied) in open.finish(root, &paths, &mut journal, cancel)? {
                        if !legacy {
                            record_applied_op(
                                store,
                                key,
                                member,
                                &applied,
                                &mut registry,
                                &mut journal,
                                &paths,
                            )?;
                        }
                    }
                } else {
                    group = Some(open);
                }
            }
            None if !legacy => record_applied_op(
                store,
                key,
                index,
                &resolved,
                &mut registry,
                &mut journal,
                &paths,
            )?,
            None => {}
        }
        // Reported *after* the commit, so a line in the account means the edit is on disk and in
        // the journal. An apply used to report nothing at all — the dry run, where nothing is at
        // stake, was the only mode that spoke.
        (options.account)(&progress_line(
            index,
            done,
            plan.ops.len(),
            op.op,
            files,
            true,
        ));
        done += 1;
    }

    let run = AppliedRun {
        journal: &journal,
        paths: &paths,
        applied: done,
        total,
    };
    refuse_a_broken_result(root, options, run, cancel)?;
    Ok(RunSummary {
        applied: done,
        total: plan.ops.len(),
        stopped_early,
    })
}

/// Bring the plan `key` up to the tree after its operation `index` was committed, record that in
/// the journal, and write the plan back: pending anchors rewritten through the edit
/// ([`PlanStore::refresh_after_op`]), a digest of them journalled, then the flush.
///
/// The order is the point. The journal's `completed` record is already down, so a crash before the
/// digest leaves an operation with no digest, and a crash after the digest and before the flush
/// leaves a digest the plan on disk does not match — both of which a resume refuses
/// ([`RestructureError::PlanOutOfSync`]) instead of reading anchors from a plan that is behind the
/// tree. The flush is synchronous for the same reason: it is what makes the next resume's check pass.
///
/// What every apply loop calls after [`commit_operation`], the command line's and the daemon's.
pub fn record_applied_op(
    store: &mut PlanStore,
    key: &PlanKey,
    index: usize,
    resolved: &crate::Resolution,
    resolver: &mut dyn crate::item_anchor::ItemResolver,
    journal: &mut Journal,
    paths: &StatePaths,
) -> Result<()> {
    let id = store
        .get(key)
        .and_then(|held| held.plan.ops.get(index))
        .and_then(|op| op.id.clone())
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "{key} has no operation {index} to refresh from"
            ))
        })?;
    store.refresh_after_op(key, &id, &resolved.edit, resolver)?;
    store.fold_foreign_op(key, &id, &resolved.edit, resolver)?;
    let held = store.get(key).ok_or_else(|| {
        RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
    })?;
    let digest = crate::plan_store::pending_digest(&held.plan, index);
    journal.append(
        &paths.journal,
        crate::journal::JournalRecord::plan_synced(index, Some(id), digest),
    )?;
    store.flush(key)?;
    settle_folded_plans(store, key)
}

/// Write back every *other* plan the operation changed by folding it in.
///
/// Each one's journal first records a digest of its new pending anchors, then the plan is written,
/// in the order [`record_applied_op`] gives for the plan that ran: a plan changed under a journal
/// that still vouched for its old anchors would be refused at its next resume as out of sync.
fn settle_folded_plans(store: &mut PlanStore, applied: &PlanKey) -> Result<()> {
    let changed: Vec<PlanKey> = store
        .list()
        .into_iter()
        .filter(|held| held.dirty && held.key != *applied)
        .map(|held| held.key)
        .collect();
    for key in changed {
        record_resynced_digest(store, &key)?;
        store.flush(&key)?;
    }
    Ok(())
}

/// Journal the digest of `key`'s pending anchors as they now are, if its journal vouched for others.
fn record_resynced_digest(store: &PlanStore, key: &PlanKey) -> Result<()> {
    let Some(held) = store.get(key) else {
        return Ok(());
    };
    let paths = StatePaths::for_plan(store.root(), &store.path_of(key))?;
    let mut journal = Journal::load(&paths.journal)?;
    let Some(last) = journal.last_completed() else {
        return Ok(());
    };
    if resume::predates_plan_write_back(&journal) {
        return Ok(());
    }
    let (op, op_id) = (last.op, last.op_id.clone());
    let digest = crate::plan_store::pending_digest(&held.plan, op);
    if journal.plan_digest_after(op) == Some(digest.as_str()) {
        return Ok(());
    }
    journal.append(
        &paths.journal,
        crate::journal::JournalRecord::plan_synced(op, op_id, digest),
    )
}

/// Refuse a run whose plan has a stale operation still to run, before anything is read from the
/// tree or written.
///
/// Not only the *next* operation: a stale one further on is refused at the same place, since the
/// run would reach it after writing everything before it, and what it would do there is what its
/// anchor says about a tree that has moved on. The author re-anchors the operation; nothing here
/// re-targets it.
///
/// What every apply loop calls first, the command line's and the daemon's.
///
/// # Errors
///
/// [`RestructureError::StaleOperation`] naming the first such operation and why it is stale.
pub fn refuse_a_stale_pending_op(
    store: &PlanStore,
    key: &PlanKey,
    options: &Options,
) -> Result<()> {
    let stale = store.stale_ops(key);
    if stale.is_empty() {
        return Ok(());
    }
    let held = store.get(key).ok_or_else(|| {
        RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
    })?;
    let paths = StatePaths::for_plan(store.root(), &store.path_of(key))?;
    let journal = Journal::load(&paths.journal)?;
    let start = resume::start_of(&held.plan, options, &journal)?;
    let pending = held
        .plan
        .ops
        .iter()
        .skip(start)
        .filter_map(|op| op.id.as_ref());
    for id in pending {
        if let Some(found) = stale.iter().find(|found| &found.op == id) {
            return Err(RestructureError::StaleOperation {
                plan: key.to_string(),
                op: id.to_string(),
                reason: found.reason.to_string(),
            });
        }
    }
    Ok(())
}

/// What a check reports of `plan`'s stale operations: one finding each, at the operation's index.
#[must_use]
pub fn stale_findings(plan: &Plan, stale: &[crate::plan_store::OpStaleness]) -> Vec<Finding> {
    stale
        .iter()
        .filter_map(|found| {
            let operation = plan
                .ops
                .iter()
                .position(|op| op.id.as_ref() == Some(&found.op))?;
            Some(Finding {
                operation,
                detail: format!("stale: {} — re-anchor it before applying", found.reason),
            })
        })
        .collect()
}
