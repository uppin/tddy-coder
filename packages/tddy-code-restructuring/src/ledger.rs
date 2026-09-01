//! The position ledger: a projection that maps *original snapshot* coordinates to current
//! on-disk coordinates.
//!
//! A refactoring on one file changes lines in other files — moving a symbol rewrites imports in
//! every consumer — so the ledger is repo-wide and folds the **entire** edit set of every
//! operation, not just the file the operation named.

use crate::edit::{FileEdit, Position, TextEdit, WorkspaceEdit};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Ordered collections throughout, so two ledgers holding the same history serialise to identical
/// bytes and compare equal structurally.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionLedger {
    files: BTreeMap<PathBuf, Vec<AppliedEdit>>,
    renames: BTreeMap<PathBuf, PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AppliedEdit {
    start: Position,
    end: Position,
    new_line_count: u32,
}

/// A ledger written to disk after a completed operation, tagged with the operation it reflects.
///
/// The checkpoint is written *after* the journal's `completed` record, so a crash between the two
/// leaves it exactly one operation behind. Carrying the index is what lets resume tell that
/// expected lag apart from a genuine divergence.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerCheckpoint {
    pub op: usize,
    pub ledger: PositionLedger,
}

impl LedgerCheckpoint {
    /// Write the checkpoint, replacing any previous one.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_vec(self)
            .map_err(|error| crate::RestructureError::MalformedPlan(error.to_string()))?;
        std::fs::write(path, encoded)?;
        Ok(())
    }

    /// Read the checkpoint, or `None` when no run has written one yet.
    pub fn load(path: &Path) -> Result<Option<LedgerCheckpoint>> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| crate::RestructureError::MalformedPlan(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

impl PositionLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate an original-snapshot position to its current on-disk position.
    ///
    /// Returns [`crate::RestructureError::AnchorInvalidated`] when the position fell inside text an
    /// earlier operation removed — never a guessed nearby position.
    pub fn translate(&self, file: &Path, pos: Position) -> Result<Position> {
        let origin = self.origin_of(file);
        let Some(applied) = self.files.get(&origin) else {
            return Ok(pos);
        };

        let mut line = pos.line;
        for edit in applied {
            if line >= edit.end.line {
                line = line.saturating_add_signed(edit.line_delta());
            } else if line >= edit.start.line {
                return Err(crate::RestructureError::AnchorInvalidated {
                    path: origin.display().to_string(),
                });
            }
        }
        Ok(Position { line, col: pos.col })
    }

    /// Fold one operation's complete multi-file edit set into the ledger.
    ///
    /// Every file the edit touches is folded, not just the one the operation named — moving a
    /// symbol rewrites imports in its consumers, and those files shift too.
    pub fn record(&mut self, edit: &WorkspaceEdit) {
        for change in &edit.changes {
            match change {
                FileEdit::Change { path, edits } => self.record_changes(path, edits),
                FileEdit::Create { .. } => {}
                FileEdit::Rename { from, to } => {
                    self.rename(Path::new(from), Path::new(to));
                }
            }
        }
    }

    /// Re-key a file's history after a `git mv`, so later anchors written against the original
    /// path still resolve.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        let origin = self.origin_of(from);
        self.renames.insert(origin, to.to_path_buf());
    }

    /// Translate an anchor from original-snapshot coordinates into current ones.
    ///
    /// A symbol anchor only needs its path following through renames — the backend re-resolves the
    /// symbol by name. A range anchor additionally has its endpoints shifted.
    pub fn translate_anchor(&self, anchor: &crate::plan::Anchor) -> Result<crate::plan::Anchor> {
        use crate::plan::Anchor;

        Ok(match anchor {
            Anchor::Symbol { file, path } => Anchor::Symbol {
                file: self.current_path(Path::new(file)).display().to_string(),
                path: path.clone(),
            },
            Anchor::Range { file, start, end } => {
                let origin = Path::new(file);
                Anchor::Range {
                    file: self.current_path(origin).display().to_string(),
                    start: self.translate(origin, *start)?,
                    end: self.translate(origin, *end)?,
                }
            }
        })
    }

    /// Current path for a file that may have been moved during the run.
    pub fn current_path(&self, original: &Path) -> PathBuf {
        self.renames
            .get(original)
            .cloned()
            .unwrap_or_else(|| original.to_path_buf())
    }

    /// Edits arriving in one `WorkspaceEdit` share a coordinate space, so each is rebased onto the
    /// space left by its predecessors. That keeps `self.files` a uniform sequence in which every
    /// entry is expressed relative to the state after all earlier entries — which is exactly what
    /// [`Self::translate`] walks.
    fn record_changes(&mut self, path: &str, edits: &[TextEdit]) {
        let mut batch: Vec<&TextEdit> = edits.iter().collect();
        batch.sort_by_key(|edit| (edit.range.start.line, edit.range.start.col));

        let applied = self.files.entry(PathBuf::from(path)).or_default();
        let mut rebase: i32 = 0;
        for edit in batch {
            let entry = AppliedEdit {
                start: shift(edit.range.start, rebase),
                end: shift(edit.range.end, rebase),
                new_line_count: line_count(&edit.new_text),
            };
            rebase += entry.line_delta();
            applied.push(entry);
        }
    }

    /// The path a file's edit history is keyed under, following any renames back to their source.
    fn origin_of(&self, path: &Path) -> PathBuf {
        self.renames
            .iter()
            .find(|(_, current)| current.as_path() == path)
            .map(|(origin, _)| origin.clone())
            .unwrap_or_else(|| path.to_path_buf())
    }
}

impl AppliedEdit {
    /// How many lines this edit added (positive) or removed (negative).
    fn line_delta(&self) -> i32 {
        self.new_line_count as i32 - (self.end.line - self.start.line) as i32
    }
}

fn shift(pos: Position, lines: i32) -> Position {
    Position {
        line: pos.line.saturating_add_signed(lines),
        col: pos.col,
    }
}

fn line_count(text: &str) -> u32 {
    text.matches('\n').count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Range, TextEdit};
    use std::path::PathBuf;

    fn at(line: u32, col: u32) -> Position {
        Position { line, col }
    }

    /// Replaces `[start_line, end_line)` in `path` with `replacement_lines` lines of text.
    fn replacement(path: &str, start_line: u32, end_line: u32, replacement_lines: u32) -> FileEdit {
        FileEdit::Change {
            path: path.to_string(),
            edits: vec![TextEdit {
                range: Range {
                    start: at(start_line, 1),
                    end: at(end_line, 1),
                },
                new_text: "x\n".repeat(replacement_lines as usize),
            }],
        }
    }

    fn edit(changes: Vec<FileEdit>) -> WorkspaceEdit {
        WorkspaceEdit { changes }
    }

    #[test]
    fn shifts_a_position_up_when_earlier_lines_in_the_same_file_are_removed() {
        let mut ledger = PositionLedger::new();
        // Lines 10..20 collapse to nothing, so everything below rises by ten lines.
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(50, 3))
            .unwrap();

        assert_eq!(translated, at(40, 3));
    }

    #[test]
    fn leaves_a_position_above_the_edit_untouched() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 40, 60, 0)]));

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(12, 5))
            .unwrap();

        assert_eq!(translated, at(12, 5));
    }

    #[test]
    fn shifts_a_position_down_when_earlier_lines_grow() {
        let mut ledger = PositionLedger::new();
        // Two lines become seven: everything below moves down five.
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 12, 7)]));

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(30, 1))
            .unwrap();

        assert_eq!(translated, at(35, 1));
    }

    /// A refactoring on one file rewrites imports in its consumers, so the ledger has to carry
    /// coordinate shifts for files the operation never named.
    #[test]
    fn shifts_positions_in_a_file_the_operation_did_not_name() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![
            replacement("src/shapes.ts", 10, 20, 0),
            replacement("src/consumer.ts", 1, 2, 4),
        ]));

        let translated = ledger
            .translate(&PathBuf::from("src/consumer.ts"), at(9, 1))
            .unwrap();

        assert_eq!(translated, at(12, 1));
    }

    #[test]
    fn accumulates_shifts_across_successive_operations() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));
        ledger.record(&edit(vec![replacement("src/shapes.ts", 100, 130, 0)]));

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(200, 1))
            .unwrap();

        assert_eq!(translated, at(160, 1));
    }

    #[test]
    fn counts_two_edits_to_one_file_in_a_single_operation_exactly_once_each() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![FileEdit::Change {
            path: "src/shapes.ts".to_string(),
            edits: vec![
                TextEdit {
                    range: Range {
                        start: at(10, 1),
                        end: at(15, 1),
                    },
                    new_text: String::new(),
                },
                TextEdit {
                    range: Range {
                        start: at(20, 1),
                        end: at(23, 1),
                    },
                    new_text: String::new(),
                },
            ],
        }]));

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(40, 1))
            .unwrap();

        assert_eq!(translated, at(32, 1));
    }

    #[test]
    fn reports_an_invalidated_anchor_when_the_position_was_inside_removed_text() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));

        let outcome = ledger.translate(&PathBuf::from("src/shapes.ts"), at(15, 1));

        assert!(matches!(
            outcome,
            Err(crate::RestructureError::AnchorInvalidated { .. })
        ));
    }

    #[test]
    fn resolves_an_original_path_to_its_destination_after_a_rename() {
        let mut ledger = PositionLedger::new();
        ledger.rename(
            &PathBuf::from("src/shapes.ts"),
            &PathBuf::from("src/geometry/shapes.ts"),
        );

        assert_eq!(
            ledger.current_path(&PathBuf::from("src/shapes.ts")),
            PathBuf::from("src/geometry/shapes.ts")
        );
    }

    #[test]
    fn keeps_translating_positions_written_against_a_path_that_was_later_renamed() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));
        ledger.rename(
            &PathBuf::from("src/shapes.ts"),
            &PathBuf::from("src/geometry/shapes.ts"),
        );

        let translated = ledger
            .translate(&PathBuf::from("src/shapes.ts"), at(50, 1))
            .unwrap();

        assert_eq!(translated, at(40, 1));
    }

    #[test]
    fn treats_an_untouched_file_as_having_no_shift() {
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));

        let translated = ledger
            .translate(&PathBuf::from("src/untouched.ts"), at(7, 2))
            .unwrap();

        assert_eq!(translated, at(7, 2));
    }

    #[test]
    fn reports_no_checkpoint_before_any_run_has_written_one() {
        let workspace = tempfile::tempdir().unwrap();

        let loaded = LedgerCheckpoint::load(&workspace.path().join("ledger.json")).unwrap();

        assert!(loaded.is_none());
    }

    #[test]
    fn round_trips_a_checkpoint_through_its_serialised_form() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("ledger.json");
        let mut ledger = PositionLedger::new();
        ledger.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));
        ledger.rename(
            &PathBuf::from("src/shapes.ts"),
            &PathBuf::from("src/geometry/shapes.ts"),
        );
        let checkpoint = LedgerCheckpoint { op: 4, ledger };

        checkpoint.write(&path).unwrap();
        let reloaded = LedgerCheckpoint::load(&path).unwrap().unwrap();

        assert_eq!(reloaded, checkpoint);
    }

    #[test]
    fn preserves_the_operation_index_a_checkpoint_was_written_at() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("ledger.json");
        LedgerCheckpoint {
            op: 12,
            ledger: PositionLedger::new(),
        }
        .write(&path)
        .unwrap();

        let reloaded = LedgerCheckpoint::load(&path).unwrap().unwrap();

        assert_eq!(reloaded.op, 12);
    }

    #[test]
    fn a_reloaded_checkpoint_translates_positions_exactly_as_the_live_ledger_did() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("ledger.json");
        let mut live = PositionLedger::new();
        live.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));
        live.record(&edit(vec![replacement("src/consumer.ts", 1, 2, 5)]));
        LedgerCheckpoint {
            op: 1,
            ledger: live,
        }
        .write(&path)
        .unwrap();

        let reloaded = LedgerCheckpoint::load(&path).unwrap().unwrap().ledger;

        assert_eq!(
            reloaded
                .translate(&PathBuf::from("src/shapes.ts"), at(50, 1))
                .unwrap(),
            at(40, 1)
        );
        assert_eq!(
            reloaded
                .translate(&PathBuf::from("src/consumer.ts"), at(10, 1))
                .unwrap(),
            at(14, 1)
        );
    }

    #[test]
    fn two_ledgers_that_recorded_the_same_history_compare_equal() {
        let mut first = PositionLedger::new();
        let mut second = PositionLedger::new();
        for ledger in [&mut first, &mut second] {
            ledger.record(&edit(vec![
                replacement("src/shapes.ts", 10, 20, 0),
                replacement("src/consumer.ts", 1, 2, 4),
            ]));
        }

        assert_eq!(first, second);
    }

    #[test]
    fn ledgers_that_recorded_different_histories_compare_unequal() {
        let mut first = PositionLedger::new();
        first.record(&edit(vec![replacement("src/shapes.ts", 10, 20, 0)]));
        let mut second = PositionLedger::new();
        second.record(&edit(vec![replacement("src/shapes.ts", 10, 25, 0)]));

        assert_ne!(first, second);
    }

    /// Deterministic serialisation is what makes the resume comparison meaningful: the same history
    /// must always produce the same bytes, whatever order the files were touched in.
    #[test]
    fn serialises_the_same_history_to_identical_bytes_regardless_of_insertion_order() {
        let mut forwards = PositionLedger::new();
        forwards.record(&edit(vec![replacement("src/a.ts", 1, 2, 0)]));
        forwards.record(&edit(vec![replacement("src/z.ts", 1, 2, 0)]));
        let mut backwards = PositionLedger::new();
        backwards.record(&edit(vec![replacement("src/z.ts", 1, 2, 0)]));
        backwards.record(&edit(vec![replacement("src/a.ts", 1, 2, 0)]));

        assert_eq!(
            serde_json::to_string(&forwards).unwrap(),
            serde_json::to_string(&backwards).unwrap()
        );
    }
}
