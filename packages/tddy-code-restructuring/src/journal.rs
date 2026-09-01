//! The append-only event journal.
//!
//! `plan.jsonl` is the command log and is never rewritten. This journal is the event log: one
//! record per operation, holding the [`WorkspaceEdit`] the operation actually produced. The
//! [`crate::PositionLedger`] is `fold(journal)`, so the same code path serves a live run and a
//! resume.
//!
//! Write-ahead discipline: `InFlight` is recorded *before* the disk write, `Completed` *after*.

use crate::edit::WorkspaceEdit;
use crate::ledger::PositionLedger;
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub seq: usize,
    /// Index into the plan's operation list.
    pub op: usize,
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
}

impl JournalRecord {
    /// Written before an operation touches disk, so a crash leaves evidence that it was attempted.
    pub fn in_flight(op: usize, pre: BTreeMap<String, String>) -> Self {
        Self {
            seq: 0,
            op,
            status: OpStatus::InFlight,
            edit: None,
            report: Vec::new(),
            notes: Vec::new(),
            pre,
            post: BTreeMap::new(),
        }
    }

    /// Written once the operation's edit has landed.
    pub fn completed(
        op: usize,
        edit: WorkspaceEdit,
        pre: BTreeMap<String, String>,
        post: BTreeMap<String, String>,
        report: Vec<crate::edit::VisibilityChange>,
        notes: Vec<String>,
    ) -> Self {
        Self {
            seq: 0,
            op,
            status: OpStatus::Completed,
            edit: Some(edit),
            pre,
            post,
            report,
            notes,
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

    fn completed(&self) -> impl Iterator<Item = &JournalRecord> {
        self.records
            .iter()
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
            status: OpStatus::Completed,
            edit: Some(edit),
            notes: Vec::new(),
            pre: BTreeMap::new(),
            post: BTreeMap::new(),
            report: Vec::new(),
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
                    seq: 1,
                    op: 1,
                    status: OpStatus::InFlight,
                    edit: None,
                    notes: Vec::new(),
                    pre: BTreeMap::new(),
                    post: BTreeMap::new(),
                    report: Vec::new(),
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
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), digest)]),
                post: BTreeMap::from([("shapes.ts".to_string(), "sha256:other".to_string())]),
                report: Vec::new(),
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
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), "sha256:other".to_string())]),
                post: BTreeMap::from([("shapes.ts".to_string(), digest)]),
                report: Vec::new(),
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
                seq: 0,
                op: 7,
                status: OpStatus::InFlight,
                edit: None,
                notes: Vec::new(),
                pre: BTreeMap::from([("shapes.ts".to_string(), "sha256:before".to_string())]),
                post: BTreeMap::from([("shapes.ts".to_string(), "sha256:after".to_string())]),
                report: Vec::new(),
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

    fn digest_of(contents: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("sha256:{:x}", Sha256::digest(contents.as_bytes()))
    }
}
