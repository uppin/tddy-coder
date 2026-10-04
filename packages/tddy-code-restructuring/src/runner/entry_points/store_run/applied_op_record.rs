use crate::runner::resume;
use crate::Result;

use crate::RestructureError;

use crate::runner::StatePaths;

use crate::Journal;

use crate::plan_store::PlanKey;

use crate::plan_store::PlanStore;

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
/// The plan that ran is settled first, and every *other* held plan is folded through the edit only
/// after ([`PlanStore::fold_foreign_op`]). A failure folding another plan fails the run, but it must
/// not leave the plan that ran with an edit on disk and no digest, which its own resume would refuse.
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
    let held = store.get(key).ok_or_else(|| {
        RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
    })?;
    let digest = crate::plan_store::pending_digest(&held.plan, index);
    journal.append(
        &paths.journal,
        crate::journal::JournalRecord::plan_synced(index, Some(id.clone()), digest),
    )?;
    store.flush(key)?;
    // The other plans last. Folding one can fail on the server, and that fails the run — but the
    // edit is on disk by now, so the plan that ran must already be one its journal vouches for.
    store.fold_foreign_op(key, &id, &resolved.edit, resolver)?;
    settle_folded_plans(store, key)
}

/// Write back every *other* plan the operation changed by folding it in.
///
/// Each one's journal first records a digest of its new pending anchors, then the plan is written,
/// in the order [`record_applied_op`] gives for the plan that ran: a plan changed under a journal
/// that still vouched for its old anchors would be refused at its next resume as out of sync.
pub(crate) fn settle_folded_plans(store: &mut PlanStore, applied: &PlanKey) -> Result<()> {
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
pub(crate) fn record_resynced_digest(store: &PlanStore, key: &PlanKey) -> Result<()> {
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
