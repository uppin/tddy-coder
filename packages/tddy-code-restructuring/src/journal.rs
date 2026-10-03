//! The append-only event journal.
//!
//! `plan.jsonl` is the command log: the plan store rewrites its pending anchors as operations apply.
//! This journal is the event log: one record per operation, holding the [`WorkspaceEdit`] the
//! operation actually produced. The [`crate::PositionLedger`] is `fold(journal)`, which is what
//! checkpoint verification compares against; a run translates anchors through its own edits only,
//! because the plan's anchors already reflect the earlier ones.
//!
//! Write-ahead discipline: `InFlight` is recorded *before* the disk write, `Completed` *after*.
//!
//! A transactional group adds records around its members, in this order: `GroupStarted` naming every
//! member, then per member a `PreImaged` record holding the bytes of each file it is about to touch
//! for the first time in the group — written before that member's `InFlight` — and finally
//! `GroupCompleted` once the group's end gate passes, or `GroupRolledBack` once its files were
//! restored from those pre-images. Hashes say *whether* a file changed; only contents can put it
//! back, which is what a rollback has to do.

use crate::edit::WorkspaceEdit;
use crate::ledger::PositionLedger;
use crate::plan::OpId;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    InFlight,
    Completed,
    Failed,
    /// The plan's pending anchors were brought up to the tree after the operation completed, and the
    /// plan is about to be written back — see [`JournalRecord::plan_digest`]. Not an operation's
    /// own state: nothing that counts or folds operations reads it.
    PlanSynced,
    /// A transactional group began: [`JournalRecord::group`] names it and
    /// [`JournalRecord::members`] lists the plan indices of every operation in it.
    GroupStarted,
    /// A group member is about to touch files for the first time in its group:
    /// [`JournalRecord::pre_images`] holds their bytes as they were. Written before the member's
    /// `InFlight`, so a crash anywhere inside the group leaves what a rollback needs.
    PreImaged,
    /// The group's end gate passed: its members stay applied.
    GroupCompleted,
    /// The group did not compile at its end, or a resume found it unfinished, and every file it
    /// touched was restored from its pre-images.
    GroupRolledBack,
}

mod group;
pub use group::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub seq: usize,
    /// Index into the plan's operation list.
    pub op: usize,
    /// The operation's stable id, which survives a reorder of the plan where the index does not.
    /// Absent from a record written before operations had ids, or by a run with none to name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_id: Option<OpId>,
    pub status: OpStatus,
    /// The resolved edit — present once the operation completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<WorkspaceEdit>,
    /// Content hashes of every touched file *before* the operation applied.
    #[serde(default)]
    pub pre: BTreeMap<String, String>,
    /// Content hashes of every touched file *after* the operation applied.
    #[serde(default)]
    pub post: BTreeMap<String, String>,
    /// What the backend had to report about this operation — a visibility it could not preserve.
    /// Skipped when empty, so a record with nothing to report serialises exactly as it always did and
    /// a journal written by an older binary still loads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub report: Vec<crate::edit::VisibilityChange>,
    /// Anything else the backend said about the operation, kept for the same reason: a consequence
    /// that was only ever printed is a consequence nobody can audit afterwards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// On a [`OpStatus::PlanSynced`] record: the digest of the plan's pending anchors as the run
    /// is about to write them ([`crate::plan_store::pending_digest`]).
    ///
    /// What a resume compares the plan it reads against. The record is appended *before* the plan
    /// is written, so a crash in between leaves a digest the file does not match — which the resume
    /// refuses — rather than a file that is behind the journal with nothing to say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    /// On a group record: the group's id, as the plan's operations name it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// On a [`OpStatus::GroupStarted`] record: the plan index of every member.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<usize>,
    /// On a [`OpStatus::PreImaged`] record: the files the member is about to touch, as they were.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pre_images: Vec<group::PreImage>,
}

impl JournalRecord {
    /// Written before an operation touches disk, so a crash leaves evidence that it was attempted.
    pub fn in_flight(op: usize, op_id: Option<OpId>, pre: BTreeMap<String, String>) -> Self {
        Self {
            seq: 0,
            op,
            op_id,
            status: OpStatus::InFlight,
            edit: None,
            report: Vec::new(),
            notes: Vec::new(),
            plan_digest: None,
            pre,
            post: BTreeMap::new(),
            group: None,
            members: Vec::new(),
            pre_images: Vec::new(),
        }
    }

    /// Written once the operation's edit has landed.
    pub fn completed(
        op: usize,
        op_id: Option<OpId>,
        edit: WorkspaceEdit,
        pre: BTreeMap<String, String>,
        post: BTreeMap<String, String>,
        report: Vec<crate::edit::VisibilityChange>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            seq: 0,
            op,
            op_id,
            status: OpStatus::Completed,
            edit: Some(edit),
            pre,
            post,
            report,
            notes,
            plan_digest: None,
            group: None,
            members: Vec::new(),
            pre_images: Vec::new(),
        }
    }

    /// Written after an operation's plan refresh and before the plan is written back.
    pub fn plan_synced(op: usize, op_id: Option<OpId>, plan_digest: String) -> Self {
        Self {
            seq: 0,
            op,
            op_id,
            status: OpStatus::PlanSynced,
            edit: None,
            pre: BTreeMap::new(),
            post: BTreeMap::new(),
            report: Vec::new(),
            notes: Vec::new(),
            plan_digest: Some(plan_digest),
            group: None,
            members: Vec::new(),
            pre_images: Vec::new(),
        }
    }

    /// Written before a group's first member: the group, and the plan index of every member. `op`
    /// is the first member's index.
    pub fn group_started(op: usize, group: String, members: Vec<usize>) -> Self {
        Self {
            members,
            ..Self::a_group_record(op, None, OpStatus::GroupStarted, group)
        }
    }

    /// Written before member `op`'s `InFlight`: the files it is about to touch for the first time
    /// in its group, as they were.
    pub fn pre_imaged(
        op: usize,
        op_id: Option<OpId>,
        group: String,
        pre_images: Vec<group::PreImage>,
    ) -> Self {
        Self {
            pre_images,
            ..Self::a_group_record(op, op_id, OpStatus::PreImaged, group)
        }
    }

    /// Written once the group's end gate passed. `op` is the last member's index.
    pub fn group_completed(op: usize, group: String) -> Self {
        Self::a_group_record(op, None, OpStatus::GroupCompleted, group)
    }

    /// Written once the group's files were restored from its pre-images. `op` is the member the
    /// group had reached.
    pub fn group_rolled_back(op: usize, group: String) -> Self {
        Self::a_group_record(op, None, OpStatus::GroupRolledBack, group)
    }

    fn a_group_record(op: usize, op_id: Option<OpId>, status: OpStatus, group: String) -> Self {
        Self {
            seq: 0,
            op,
            op_id,
            status,
            edit: None,
            pre: BTreeMap::new(),
            post: BTreeMap::new(),
            report: Vec::new(),
            notes: Vec::new(),
            plan_digest: None,
            group: Some(group),
            members: Vec::new(),
            pre_images: Vec::new(),
        }
    }
}

/// What a resume should do about a trailing `InFlight` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDecision {
    /// Files match the pre-operation hashes — the operation never landed. Run it again.
    ReRun(usize),
    /// Files match the post-operation hashes — it landed but the ack was lost. Mark it done.
    MarkCompleted(usize),
    /// Files match neither. The tree changed underneath the run.
    Abort(usize),
}

#[derive(Debug, Default)]
pub struct Journal {
    pub records: Vec<JournalRecord>,
}

impl Journal {
    /// Load a journal from disk, or an empty one when no journal exists.
    pub fn load(path: &Path) -> Result<Journal> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Journal::default())
            }
            Err(error) => return Err(error.into()),
        };

        let records = contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .map_err(|error| crate::RestructureError::MalformedPlan(error.to_string()))
            })
            .collect::<Result<Vec<JournalRecord>>>()?;

        Ok(Journal { records })
    }

    /// Append one record, flushing before returning so a crash cannot lose it.
    ///
    /// Sequence numbers are assigned here, so callers never have to track them.
    pub fn append(&mut self, path: &Path, record: JournalRecord) -> Result<()> {
        use std::io::Write;

        let record = JournalRecord {
            seq: self.records.len(),
            ..record
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_string(&record)
            .map_err(|error| crate::RestructureError::MalformedPlan(error.to_string()))?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{encoded}")?;
        file.flush()?;
        file.sync_all()?;

        self.records.push(record);
        Ok(())
    }

    /// Rebuild the position ledger by folding every completed record.
    pub fn fold(&self) -> PositionLedger {
        self.fold_records(self.completed())
    }

    /// Fold only the records for operations at or before `op`.
    pub fn fold_through(&self, op: usize) -> PositionLedger {
        self.fold_records(self.completed().filter(|record| record.op <= op))
    }

    /// Compare a persisted checkpoint against the fold of this journal truncated to the operation
    /// the checkpoint records.
    ///
    /// The journal stays authoritative — this exists so that a fold bug or an out-of-band edit
    /// fails the run loudly instead of silently producing wrong coordinates.
    pub fn verify_checkpoint(&self, checkpoint: &crate::LedgerCheckpoint) -> Result<()> {
        let diverged = || crate::RestructureError::CheckpointDivergence { op: checkpoint.op };

        if !self.completed().any(|record| record.op == checkpoint.op) {
            return Err(diverged());
        }
        if self.fold_through(checkpoint.op) != checkpoint.ledger {
            return Err(diverged());
        }
        Ok(())
    }

    /// Index of the first operation a resume should execute.
    pub fn next_op(&self) -> usize {
        self.completed()
            .map(|record| record.op + 1)
            .max()
            .unwrap_or(0)
    }

    /// Decide what to do about a trailing `InFlight` record by hashing the working tree.
    pub fn resume_decision(&self, root: &Path) -> Result<Option<ResumeDecision>> {
        let Some(record) = self
            .records
            .last()
            .filter(|r| r.status == OpStatus::InFlight)
        else {
            return Ok(None);
        };

        let decision = if hashes_match(root, &record.pre)? {
            ResumeDecision::ReRun(record.op)
        } else if hashes_match(root, &record.post)? {
            ResumeDecision::MarkCompleted(record.op)
        } else {
            ResumeDecision::Abort(record.op)
        };
        Ok(Some(decision))
    }

    /// The completed operation furthest into the plan, if any completed.
    pub fn last_completed(&self) -> Option<&JournalRecord> {
        self.completed().max_by_key(|record| record.op)
    }

    /// The digest of the pending anchors the run recorded after operation `op`, if it recorded one.
    pub fn plan_digest_after(&self, op: usize) -> Option<&str> {
        self.records
            .iter()
            .rev()
            .find(|record| record.status == OpStatus::PlanSynced && record.op == op)
            .and_then(|record| record.plan_digest.as_deref())
    }

    /// The group this journal started and never completed or rolled back, with every pre-image its
    /// members journalled — what a resume rolls back before running anything.
    pub fn open_group(&self) -> Option<group::OpenGroup> {
        let started = self
            .records
            .iter()
            .rposition(|record| record.status == OpStatus::GroupStarted)?;
        let records = &self.records[started..];
        let group = records[0].group.clone()?;
        let ended = records.iter().any(|record| {
            matches!(
                record.status,
                OpStatus::GroupCompleted | OpStatus::GroupRolledBack
            ) && record.group.as_deref() == Some(group.as_str())
        });
        if ended {
            return None;
        }
        Some(group::OpenGroup {
            members: records[0].members.clone(),
            pre_images: pre_images_of(records),
            group,
        })
    }

    /// The records of the group `group`'s most recent run, from its `group_started` on. Empty when
    /// the journal never started it.
    pub fn group_records(&self, group: &str) -> &[JournalRecord] {
        match self.records.iter().rposition(|record| {
            record.status == OpStatus::GroupStarted && record.group.as_deref() == Some(group)
        }) {
            Some(started) => &self.records[started..],
            None => &[],
        }
    }

    /// Every pre-image the records of group `group`'s latest run journalled, in the order written.
    pub fn group_pre_images(&self, group: &str) -> Vec<group::PreImage> {
        pre_images_of(self.group_records(group))
    }

    /// The records that still describe the tree: all of them but those of a group that was rolled
    /// back, which the journal keeps as evidence and which no longer hold true of any file.
    pub fn records_in_force(&self) -> impl Iterator<Item = &JournalRecord> {
        let mut undone = vec![false; self.records.len()];
        let mut started = None;
        for (position, record) in self.records.iter().enumerate() {
            match record.status {
                OpStatus::GroupStarted => started = Some(position),
                OpStatus::GroupCompleted => started = None,
                OpStatus::GroupRolledBack => {
                    if let Some(first) = started.take() {
                        undone[first..=position].fill(true);
                    }
                }
                _ => {}
            }
        }
        self.records
            .iter()
            .zip(undone)
            .filter(|(_, undone)| !undone)
            .map(|(record, _)| record)
    }

    /// The operations that completed and were not rolled back with their group.
    pub fn completed(&self) -> impl Iterator<Item = &JournalRecord> {
        self.records_in_force()
            .filter(|record| record.status == OpStatus::Completed)
    }

    fn fold_records<'a>(&self, records: impl Iterator<Item = &'a JournalRecord>) -> PositionLedger {
        let mut ledger = PositionLedger::new();
        for record in records {
            if let Some(edit) = &record.edit {
                ledger.record(edit);
            }
        }
        ledger
    }
}

/// The pre-images the `pre_imaged` records among `records` carry, in order.
fn pre_images_of(records: &[JournalRecord]) -> Vec<group::PreImage> {
    records
        .iter()
        .filter(|record| record.status == OpStatus::PreImaged)
        .flat_map(|record| record.pre_images.iter().cloned())
        .collect()
}

/// Whether every recorded hash still matches the file on disk. An empty record matches nothing —
/// there is no state to confirm, so it cannot stand in for evidence.
fn hashes_match(root: &Path, expected: &BTreeMap<String, String>) -> Result<bool> {
    if expected.is_empty() {
        return Ok(false);
    }
    for (path, hash) in expected {
        if &crate::apply::hash_file(&root.join(path))? != hash {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Position, Range, TextEdit, WorkspaceEdit};
    use std::path::PathBuf;

    fn removal(path: &str, start_line: u32, end_line: u32) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: vec![FileEdit::Change {
                path: path.to_string(),
                edits: vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: start_line,
                            col: 1,
                        },
                        end: Position {
                            line: end_line,
                            col: 1,
                        },
                    },
                    new_text: String::new(),
                }],
            }],
        }
    }

    fn completed(seq: usize, op: usize, edit: WorkspaceEdit) -> JournalRecord {
        JournalRecord {
            seq,
            op,
            op_id: None,
            plan_digest: None,
            status: OpStatus::Completed,
            edit: Some(edit),
            notes: Vec::new(),
            pre: BTreeMap::new(),
            post: BTreeMap::new(),
            report: Vec::new(),
            group: None,
            members: Vec::new(),
            pre_images: Vec::new(),
        }
    }

    #[test]
    fn returns_an_empty_journal_when_none_has_been_written() {
        let workspace = tempfile::tempdir().unwrap();

        let journal = Journal::load(&workspace.path().join("journal.jsonl")).unwrap();

        assert!(journal.records.is_empty());
    }

    #[test]
    fn reads_back_every_record_it_appended() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("journal.jsonl");
        let mut journal = Journal::default();
        journal
            .append(&path, completed(0, 0, removal("src/shapes.ts", 10, 20)))
            .unwrap();
        journal
            .append(&path, completed(1, 1, removal("src/shapes.ts", 30, 33)))
            .unwrap();

        let reloaded = Journal::load(&path).unwrap();

        assert_eq!(reloaded.records.len(), 2);
        assert_eq!(reloaded.records[1].op, 1);
    }

    /// The ledger is a projection over the journal, so replaying the journal must land on exactly
    /// the coordinates a live run would have produced.
    #[test]
    fn folding_the_journal_reproduces_the_coordinates_of_a_live_run() {
        let first = removal("src/shapes.ts", 10, 20);
        let second = removal("src/shapes.ts", 100, 130);

        let mut live = crate::PositionLedger::new();
        live.record(&first);
        live.record(&second);

        let journal = Journal {
            records: vec![completed(0, 0, first), completed(1, 1, second)],
        };
        let folded = journal.fold();

        let probe = Position { line: 200, col: 1 };
        let file = PathBuf::from("src/shapes.ts");
        assert_eq!(
            folded.translate(&file, probe).unwrap(),
            live.translate(&file, probe).unwrap()
        );
    }

    #[test]
    fn skips_records_that_did_not_complete_when_folding() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                JournalRecord {
                    op_id: None,
                    plan_digest: None,
                    seq: 1,
                    op: 1,
                    status: OpStatus::InFlight,
                    edit: None,
                    notes: Vec::new(),
                    pre: BTreeMap::new(),
                    post: BTreeMap::new(),
                    report: Vec::new(),
                    group: None,
                    members: Vec::new(),
                    pre_images: Vec::new(),
                },
            ],
        };

        let translated = journal
            .fold()
            .translate(
                &PathBuf::from("src/shapes.ts"),
                Position { line: 50, col: 1 },
            )
            .unwrap();

        assert_eq!(translated, Position { line: 40, col: 1 });
    }

    #[test]
    fn resumes_at_the_operation_after_the_last_completed_one() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 30, 33)),
            ],
        };

        assert_eq!(journal.next_op(), 2);
    }

    #[test]
    fn resumes_at_the_first_operation_when_the_journal_is_empty() {
        assert_eq!(Journal::default().next_op(), 0);
    }

    #[test]
    fn re_runs_an_in_flight_operation_whose_files_still_match_the_pre_state() {
        let workspace = tempfile::tempdir().unwrap();
        let contents = "before the operation\n";
        std::fs::write(workspace.path().join("shapes.ts"), contents).unwrap();
        let digest = digest_of(contents);
        let journal = Journal {
            records: vec![JournalRecord {
                op_id: None,
                plan_digest: None,
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), digest)]),
                post: BTreeMap::from([("shapes.ts".to_string(), "sha256:other".to_string())]),
                report: Vec::new(),
                group: None,
                members: Vec::new(),
                pre_images: Vec::new(),
            }],
        };

        let decision = journal.resume_decision(workspace.path()).unwrap();

        assert_eq!(decision, Some(ResumeDecision::ReRun(7)));
    }

    #[test]
    fn marks_an_in_flight_operation_complete_when_its_files_match_the_post_state() {
        let workspace = tempfile::tempdir().unwrap();
        let contents = "after the operation\n";
        std::fs::write(workspace.path().join("shapes.ts"), contents).unwrap();
        let digest = digest_of(contents);
        let journal = Journal {
            records: vec![JournalRecord {
                op_id: None,
                plan_digest: None,
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), "sha256:other".to_string())]),
                post: BTreeMap::from([("shapes.ts".to_string(), digest)]),
                report: Vec::new(),
                group: None,
                members: Vec::new(),
                pre_images: Vec::new(),
            }],
        };

        let decision = journal.resume_decision(workspace.path()).unwrap();

        assert_eq!(decision, Some(ResumeDecision::MarkCompleted(7)));
    }

    #[test]
    fn aborts_when_an_in_flight_operation_matches_neither_recorded_state() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("shapes.ts"),
            "edited by someone else\n",
        )
        .unwrap();
        let journal = Journal {
            records: vec![JournalRecord {
                op_id: None,
                plan_digest: None,
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), "sha256:before".to_string())]),
                post: BTreeMap::from([("shapes.ts".to_string(), "sha256:after".to_string())]),
                report: Vec::new(),
                group: None,
                members: Vec::new(),
                pre_images: Vec::new(),
            }],
        };

        let decision = journal.resume_decision(workspace.path()).unwrap();

        assert_eq!(decision, Some(ResumeDecision::Abort(7)));
    }

    #[test]
    fn has_nothing_to_decide_when_the_journal_ends_cleanly() {
        let workspace = tempfile::tempdir().unwrap();
        let journal = Journal {
            records: vec![completed(0, 0, removal("src/shapes.ts", 10, 20))],
        };

        assert_eq!(journal.resume_decision(workspace.path()).unwrap(), None);
    }

    #[test]
    fn folds_only_the_records_at_or_before_the_requested_operation() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 100, 130)),
            ],
        };

        let through_first = journal.fold_through(0);

        assert_eq!(
            through_first
                .translate(
                    &PathBuf::from("src/shapes.ts"),
                    Position { line: 200, col: 1 }
                )
                .unwrap(),
            Position { line: 190, col: 1 }
        );
    }

    #[test]
    fn folding_through_the_last_operation_matches_a_full_fold() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 100, 130)),
            ],
        };

        assert_eq!(journal.fold_through(1), journal.fold());
    }

    #[test]
    fn accepts_a_checkpoint_that_is_level_with_the_journal() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 100, 130)),
            ],
        };
        let checkpoint = crate::LedgerCheckpoint {
            op: 1,
            ledger: journal.fold(),
        };

        assert!(journal.verify_checkpoint(&checkpoint).is_ok());
    }

    /// The checkpoint is written after the journal's `completed` record, so a crash between the two
    /// leaves it one operation behind. That lag is expected, not a divergence.
    #[test]
    fn accepts_a_checkpoint_written_one_operation_behind_the_journal() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 100, 130)),
            ],
        };
        let checkpoint = crate::LedgerCheckpoint {
            op: 0,
            ledger: journal.fold_through(0),
        };

        assert!(journal.verify_checkpoint(&checkpoint).is_ok());
    }

    #[test]
    fn rejects_a_checkpoint_whose_ledger_disagrees_with_the_fold() {
        let journal = Journal {
            records: vec![completed(0, 0, removal("src/shapes.ts", 10, 20))],
        };
        let mut tampered = crate::PositionLedger::new();
        tampered.record(&removal("src/shapes.ts", 10, 40));
        let checkpoint = crate::LedgerCheckpoint {
            op: 0,
            ledger: tampered,
        };

        let outcome = journal.verify_checkpoint(&checkpoint);

        assert!(matches!(
            outcome,
            Err(crate::RestructureError::CheckpointDivergence { op: 0 })
        ));
    }

    #[test]
    fn rejects_a_checkpoint_recorded_at_an_operation_the_journal_never_completed() {
        let journal = Journal {
            records: vec![completed(0, 0, removal("src/shapes.ts", 10, 20))],
        };
        let checkpoint = crate::LedgerCheckpoint {
            op: 5,
            ledger: journal.fold(),
        };

        let outcome = journal.verify_checkpoint(&checkpoint);

        assert!(matches!(
            outcome,
            Err(crate::RestructureError::CheckpointDivergence { op: 5 })
        ));
    }

    #[test]
    fn names_the_operation_at_which_a_checkpoint_diverged() {
        let journal = Journal {
            records: vec![
                completed(0, 0, removal("src/shapes.ts", 10, 20)),
                completed(1, 1, removal("src/shapes.ts", 100, 130)),
            ],
        };
        let mut tampered = crate::PositionLedger::new();
        tampered.record(&removal("src/other.ts", 1, 9));
        let checkpoint = crate::LedgerCheckpoint {
            op: 1,
            ledger: tampered,
        };

        match journal.verify_checkpoint(&checkpoint) {
            Err(crate::RestructureError::CheckpointDivergence { op }) => assert_eq!(op, 1),
            other => panic!("expected a checkpoint divergence, got {other:?}"),
        }
    }

    /// A pre-image is the only thing a rollback has to write a file back from, so what goes into
    /// the journal must come back out of it able to restore the file exactly.
    #[test]
    fn a_pre_image_round_trips_through_the_journal() {
        // Given a file captured before a group member touched it
        let workspace = tempfile::tempdir().unwrap();
        let journal_path = workspace.path().join("journal.jsonl");
        std::fs::write(workspace.path().join("shapes.rs"), "pub struct Circle;\n").unwrap();
        let captured = group::PreImage::capture(workspace.path(), "shapes.rs").unwrap();
        let mut journal = Journal::default();
        journal
            .append(
                &journal_path,
                JournalRecord::pre_imaged(0, None, "shapes".to_string(), vec![captured]),
            )
            .unwrap();
        std::fs::write(workspace.path().join("shapes.rs"), "pub struct Disc;\n").unwrap();

        // When the pre-image is read back from disk and restored
        let reloaded = Journal::load(&journal_path).unwrap();
        reloaded.records[0].pre_images[0]
            .restore(workspace.path())
            .unwrap();

        // Then the file holds exactly what it held before
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("shapes.rs")).unwrap(),
            "pub struct Circle;\n"
        );
    }

    fn a_journal_of(records: Vec<JournalRecord>) -> Journal {
        Journal { records }
    }

    fn an_image(path: &str, contents: Option<&str>) -> group::PreImage {
        group::PreImage {
            path: path.to_string(),
            contents: contents.map(str::to_string),
        }
    }

    fn a_completed_op(op: usize) -> JournalRecord {
        completed(0, op, removal("src/shapes.rs", 1, 2))
    }

    /// A group of the members `0` and `1` that journalled one pre-image for each, and completed
    /// both.
    fn a_group_run_through(group: &str, ending: JournalRecord) -> Vec<JournalRecord> {
        vec![
            JournalRecord::group_started(0, group.to_string(), vec![0, 1]),
            JournalRecord::pre_imaged(
                0,
                None,
                group.to_string(),
                vec![an_image("a.rs", Some("a before"))],
            ),
            a_completed_op(0),
            JournalRecord::pre_imaged(1, None, group.to_string(), vec![an_image("b.rs", None)]),
            a_completed_op(1),
            ending,
        ]
    }

    #[test]
    fn a_started_group_nothing_ended_is_open_with_its_members_and_pre_images_in_order() {
        // Given a group that crashed after both members
        let mut records = a_group_run_through("shapes", a_completed_op(1));
        records.pop();
        let journal = a_journal_of(records);

        // When
        let open = journal.open_group();

        // Then
        assert_eq!(
            open,
            Some(group::OpenGroup {
                group: "shapes".to_string(),
                members: vec![0, 1],
                pre_images: vec![an_image("a.rs", Some("a before")), an_image("b.rs", None)],
            })
        );
    }

    #[test]
    fn a_completed_group_is_not_open() {
        let journal = a_journal_of(a_group_run_through(
            "shapes",
            JournalRecord::group_completed(1, "shapes".to_string()),
        ));

        assert_eq!(journal.open_group(), None);
    }

    #[test]
    fn a_rolled_back_group_is_not_open() {
        let journal = a_journal_of(a_group_run_through(
            "shapes",
            JournalRecord::group_rolled_back(1, "shapes".to_string()),
        ));

        assert_eq!(journal.open_group(), None);
    }

    #[test]
    fn a_group_started_after_a_completed_one_is_the_open_one() {
        // Given a completed group, then another that never ended
        let mut records = a_group_run_through(
            "first",
            JournalRecord::group_completed(1, "first".to_string()),
        );
        records.push(JournalRecord::group_started(
            2,
            "second".to_string(),
            vec![2],
        ));
        let journal = a_journal_of(records);

        // When
        let open = journal.open_group().map(|open| open.group);

        // Then
        assert_eq!(open, Some("second".to_string()));
    }

    #[test]
    fn the_pre_images_of_a_group_are_those_of_its_latest_run_only() {
        // Given another group completed, then `shapes` rolled back and run again
        let mut records = a_group_run_through(
            "other",
            JournalRecord::group_completed(1, "other".to_string()),
        );
        records.extend(a_group_run_through(
            "shapes",
            JournalRecord::group_rolled_back(1, "shapes".to_string()),
        ));
        records.extend([
            JournalRecord::group_started(0, "shapes".to_string(), vec![0]),
            JournalRecord::pre_imaged(
                0,
                None,
                "shapes".to_string(),
                vec![an_image("c.rs", Some("c before"))],
            ),
        ]);
        let journal = a_journal_of(records);

        // When
        let images = journal.group_pre_images("shapes");

        // Then
        assert_eq!(images, vec![an_image("c.rs", Some("c before"))]);
    }

    #[test]
    fn the_operations_of_a_rolled_back_group_do_not_count_as_completed() {
        // Given an ungrouped operation, then a group of two that was rolled back
        let journal = a_journal_of(vec![
            a_completed_op(0),
            JournalRecord::group_started(1, "shapes".to_string(), vec![1, 2]),
            a_completed_op(1),
            a_completed_op(2),
            JournalRecord::group_rolled_back(2, "shapes".to_string()),
        ]);

        // When
        let completed: Vec<usize> = journal.completed().map(|record| record.op).collect();

        // Then only the operation before the group counts
        assert_eq!(completed, vec![0]);
    }

    #[test]
    fn the_operations_of_a_completed_group_count_as_completed() {
        let journal = a_journal_of(a_group_run_through(
            "shapes",
            JournalRecord::group_completed(1, "shapes".to_string()),
        ));

        let completed: Vec<usize> = journal.completed().map(|record| record.op).collect();

        assert_eq!(completed, vec![0, 1]);
    }

    #[test]
    fn records_in_force_drop_every_record_of_a_rolled_back_group_but_keep_the_rest() {
        // Given a group rolled back, then an ungrouped operation
        let mut records = a_group_run_through(
            "shapes",
            JournalRecord::group_rolled_back(1, "shapes".to_string()),
        );
        records.push(a_completed_op(2));
        let journal = a_journal_of(records);

        // When
        let in_force: Vec<(OpStatus, usize)> = journal
            .records_in_force()
            .map(|record| (record.status, record.op))
            .collect();

        // Then
        assert_eq!(in_force, vec![(OpStatus::Completed, 2)]);
    }

    fn digest_of(contents: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("sha256:{:x}", Sha256::digest(contents.as_bytes()))
    }
}
