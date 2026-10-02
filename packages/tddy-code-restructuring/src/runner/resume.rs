//! What a run that continues a journal must establish before it reads the plan's anchors.
//!
//! The plan store writes a plan's pending anchors back after every operation, so the plan a resume
//! reads describes the tree as the journal left it — *if* the write-back happened. These are the
//! checks that make "if" something a run can know, and the lowering that lets item anchors be read
//! from a tree that already holds the journal's edits.

use std::path::Path;

use crate::item_anchor::{self, has_item_anchors};
use crate::journal::Journal;
use crate::plan::Plan;
use crate::plan_store::pending_digest;
use crate::registry::BackendRegistry;
use crate::{RestructureError, Result};

use super::options::usage;
use super::Options;

/// The index of the first operation a run executes: `--from` as an index, `--from` as an id, or
/// where the journal left off.
pub(super) fn start_of(plan: &Plan, options: &Options, journal: &Journal) -> Result<usize> {
    match (options.from, &options.from_id) {
        (Some(_), Some(_)) => Err(usage("--from names an index or an id, not both")),
        (Some(index), None) => Ok(index),
        (None, Some(id)) => plan
            .ops
            .iter()
            .position(|op| op.id.as_ref() == Some(id))
            .ok_or_else(|| {
                RestructureError::MalformedPlan(format!(
                    "--from names the operation `{id}`, which this plan does not have"
                ))
            }),
        (None, None) => Ok(journal.next_op()),
    }
}

/// Whether `journal` was written before plans were kept current: its furthest completed operation
/// carries no id, because runs of that time had none to record.
///
/// The plan beside such a journal was never written back, so its anchors are in the coordinates the
/// run started in, and what translates them is the journal itself.
pub(super) fn predates_plan_write_back(journal: &Journal) -> bool {
    journal
        .last_completed()
        .is_some_and(|record| record.op_id.is_none())
}

/// Refuse a plan whose anchors the journal cannot vouch for.
///
/// - A journal that predates write-back vouches for nothing. A plan of range and symbol anchors is
///   still run as it always was, over the ledger the journal folds to; one with item anchors is
///   refused, because an item anchor is resolved against the tree and the tree is already edited.
/// - Any other journal recorded a digest of the plan's pending anchors after its last completed
///   operation, before writing the plan. A plan that does not match it — or a journal with no such
///   record, which is a crash between the operation and the record — is refused.
///
/// Every outcome is a run that starts correctly or not at all: none redoes an operation or skips one.
pub(super) fn refuse_a_plan_the_journal_cannot_vouch_for(
    plan: &Plan,
    journal: &Journal,
) -> Result<()> {
    let Some(last) = journal.last_completed() else {
        return Ok(());
    };
    if predates_plan_write_back(journal) {
        return if has_item_anchors(plan) {
            Err(RestructureError::PlanUnverifiable {
                applied: last.op + 1,
            })
        } else {
            Ok(())
        };
    }
    match journal.plan_digest_after(last.op) {
        Some(recorded) if recorded == pending_digest(plan, last.op) => Ok(()),
        _ => Err(RestructureError::PlanOutOfSync { op: last.op }),
    }
}

/// `plan` with the item anchors of the operations from `start` on resolved against the tree as it
/// stands, every earlier operation left as written.
///
/// The earlier ones are done: their items were edited by the operations themselves, so resolving
/// them would refuse on a fingerprint that has rightly moved on.
pub(super) fn lower_pending(
    plan: &Plan,
    start: usize,
    root: &Path,
    registry: &mut BackendRegistry,
) -> Result<Plan> {
    let done = start.min(plan.ops.len());
    let pending = Plan {
        ops: plan.ops[done..].to_vec(),
        ..plan.clone()
    };
    if !has_item_anchors(&pending) {
        return Ok(plan.clone());
    }
    let lowered = item_anchor::resolve_item_anchors(&pending, root, registry)?;
    let mut ops = plan.ops[..done].to_vec();
    ops.extend(lowered.ops);
    Ok(Plan {
        ops,
        ..plan.clone()
    })
}
