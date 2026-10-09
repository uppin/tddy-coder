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

/// What `journal` says about the run of its plan that last wrote the plan file back, if any did.
///
/// Some when the journal holds a completed operation with a `PlanSynced` record after it — the
/// run committed operations and rewrote the plan's anchors for the tree it left. `undone` is true
/// when every file a completed record touched hashes to that record's `pre` under `root`: the
/// run's edits have been rolled back since, so the plan describes a tree that no longer exists.
#[allow(
    dead_code,
    reason = "TODO(reshape-apply-robust): implement — called by refuse_a_stale_pending_op at green"
)]
pub(super) fn written_by_run(
    journal: &Journal,
    root: &Path,
    paths: &super::StatePaths,
) -> Result<Option<crate::WrittenByRun>> {
    let _ = (journal, root, paths);
    todo!("TODO(reshape-apply-robust): implement")
}

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

#[cfg(test)]
mod written_by_run_tests {
    use super::*;
    use crate::apply::hash_file;
    use crate::edit::{FileEdit, WorkspaceEdit};
    use crate::journal::JournalRecord;
    use crate::runner::StatePaths;
    use std::collections::BTreeMap;

    /// A journal whose run of the plan completed operation 0 — `src/a.rs` from `before` to
    /// `after` — and then wrote the plan back.
    fn a_journal_of_a_run_that_rewrote_src_a(root: &Path, before: &str, after: &str) -> Journal {
        let digest_of = |text: &str| {
            let file = root.join("digest.tmp");
            std::fs::write(&file, text).unwrap();
            let digest = hash_file(&file).unwrap();
            std::fs::remove_file(&file).unwrap();
            digest
        };
        let pre = BTreeMap::from([("src/a.rs".to_string(), digest_of(before))]);
        let post = BTreeMap::from([("src/a.rs".to_string(), digest_of(after))]);
        let edit = WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: "src/a.rs".to_string(),
                edits: Vec::new(),
            }],
        };
        let mut journal = Journal::default();
        let file = the_journal_file_of_plan_jsonl(root);
        for record in [
            JournalRecord::in_flight(0, None, pre.clone()),
            JournalRecord::completed(0, None, edit, pre, post, Vec::new(), Vec::new()),
            JournalRecord::plan_synced(0, None, "digest".to_string()),
        ] {
            journal.append(&file, record).unwrap();
        }
        journal
    }

    /// Where a run of `plan.jsonl` under `root` journals, its directory created.
    fn the_journal_file_of_plan_jsonl(root: &Path) -> std::path::PathBuf {
        let directory =
            crate::runner::state_directory_for_plan(root, Path::new("plan.jsonl")).unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        directory.join("journal.jsonl")
    }

    /// `#reshape` 10/19: "undone" is a claim about every file the run touched, so it holds only
    /// when each one is back at its pre-run content.
    #[test]
    fn written_by_run_reports_undone_only_when_every_touched_file_is_back_at_its_pre_hash() {
        // Given a run that rewrote `src/a.rs` from "one" to "two", and the file restored to "one"
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let journal = a_journal_of_a_run_that_rewrote_src_a(root, "one\n", "two\n");
        std::fs::write(root.join("src/a.rs"), "one\n").unwrap();
        let paths = StatePaths::for_plan(root, Path::new("plan.jsonl")).unwrap();

        // When the journal is asked who wrote the plan, with the file restored and then not
        let restored = written_by_run(&journal, root, &paths).unwrap();
        std::fs::write(root.join("src/a.rs"), "two\n").unwrap();
        let left = written_by_run(&journal, root, &paths).unwrap();

        // Then both name operation 0, and only the restored tree is undone
        assert_eq!(
            (
                restored.as_ref().map(|run| (run.last_applied, run.undone)),
                left.as_ref().map(|run| (run.last_applied, run.undone))
            ),
            (Some((0, true)), Some((0, false)))
        );
    }
}
