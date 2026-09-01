//! What the working tree *would* hold, for a run that is resolving without writing.
//!
//! A dry run folds the coordinate ledger exactly as a real run does, so every operation after the
//! first is addressed at translated coordinates. Read the file from disk at that point and the
//! coordinates describe a tree that does not exist: the anchor has moved and the text has not. The
//! overlay closes that gap by carrying each resolved edit forward in memory, so a rehearsal answers
//! for the whole plan rather than only for its first operation.
//!
//! A real run leaves the overlay empty and every read falls through to disk, which is the tree that
//! is actually being edited.

use crate::edit::{FileEdit, WorkspaceEdit};
use crate::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Resolved-but-unwritten file contents, keyed by path relative to the workspace root.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Overlay {
    files: BTreeMap<PathBuf, String>,
}

impl Overlay {
    pub fn new() -> Self {
        Overlay::default()
    }

    /// Whether anything has been recorded. An empty overlay is a real run.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The contents of `relative` as this run last resolved it, or as it stands on disk.
    pub fn read(&self, root: &Path, relative: &Path) -> Result<String> {
        match self.files.get(relative) {
            Some(contents) => Ok(contents.clone()),
            None => Ok(std::fs::read_to_string(root.join(relative))?),
        }
    }

    /// Fold one resolved edit in, as `apply_workspace_edit` would fold it into the tree.
    ///
    /// A creation seeds an empty entry so a later read does not look for a file that was never
    /// written; a rename moves the entry, and drops the old key so reading it falls through to the
    /// path on disk rather than answering with text that has moved away.
    pub fn record(&mut self, root: &Path, edit: &WorkspaceEdit) -> Result<()> {
        for change in &edit.changes {
            if let FileEdit::Create { path } = change {
                self.files.insert(PathBuf::from(path), String::new());
            }
        }
        for change in &edit.changes {
            if let FileEdit::Change { path, edits } = change {
                let relative = PathBuf::from(path);
                let before = self.read(root, &relative)?;
                self.files
                    .insert(relative, crate::apply::edited(before, edits)?);
            }
        }
        for change in &edit.changes {
            if let FileEdit::Rename { from, to } = change {
                if let Some(contents) = self.files.remove(Path::new(from)) {
                    self.files.insert(PathBuf::from(to), contents);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Position, Range, TextEdit};

    fn replacing(path: &str, start_line: u32, end_line: u32, text: &str) -> WorkspaceEdit {
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
                    new_text: text.to_string(),
                }],
            }],
        }
    }

    fn workspace() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.rs"), "one\ntwo\nthree\n").unwrap();
        root
    }

    #[test]
    fn reads_a_file_it_holds_nothing_for_from_disk() {
        let root = workspace();
        let overlay = Overlay::new();

        let read = overlay.read(root.path(), Path::new("a.rs")).unwrap();

        assert_eq!(read, "one\ntwo\nthree\n");
    }

    /// The whole point: the second operation of a plan must see what the first one resolved.
    #[test]
    fn reads_back_the_text_an_earlier_edit_produced_rather_than_the_text_on_disk() {
        let root = workspace();
        let mut overlay = Overlay::new();

        overlay
            .record(root.path(), &replacing("a.rs", 1, 2, "ONE\nEXTRA\n"))
            .unwrap();

        assert_eq!(
            overlay.read(root.path(), Path::new("a.rs")).unwrap(),
            "ONE\nEXTRA\ntwo\nthree\n"
        );
    }

    #[test]
    fn leaves_the_file_on_disk_untouched() {
        let root = workspace();
        let mut overlay = Overlay::new();

        overlay
            .record(root.path(), &replacing("a.rs", 1, 2, "ONE\n"))
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(root.path().join("a.rs")).unwrap(),
            "one\ntwo\nthree\n"
        );
    }

    #[test]
    fn folds_a_second_edit_onto_the_text_the_first_one_left() {
        let root = workspace();
        let mut overlay = Overlay::new();

        overlay
            .record(root.path(), &replacing("a.rs", 1, 2, "ONE\n"))
            .unwrap();
        overlay
            .record(root.path(), &replacing("a.rs", 2, 3, "TWO\n"))
            .unwrap();

        assert_eq!(
            overlay.read(root.path(), Path::new("a.rs")).unwrap(),
            "ONE\nTWO\nthree\n"
        );
    }

    /// A file the engine invented has no disk baseline, so reading it must not go looking for one.
    #[test]
    fn treats_a_created_file_as_empty_rather_than_missing() {
        let root = workspace();
        let mut overlay = Overlay::new();

        overlay
            .record(
                root.path(),
                &WorkspaceEdit {
                    changes: vec![FileEdit::Create {
                        path: "new.rs".to_string(),
                    }],
                },
            )
            .unwrap();

        assert_eq!(overlay.read(root.path(), Path::new("new.rs")).unwrap(), "");
    }

    #[test]
    fn carries_recorded_text_across_a_rename() {
        let root = workspace();
        let mut overlay = Overlay::new();

        overlay
            .record(root.path(), &replacing("a.rs", 1, 2, "ONE\n"))
            .unwrap();
        overlay
            .record(
                root.path(),
                &WorkspaceEdit {
                    changes: vec![FileEdit::Rename {
                        from: "a.rs".to_string(),
                        to: "b.rs".to_string(),
                    }],
                },
            )
            .unwrap();

        assert_eq!(
            overlay.read(root.path(), Path::new("b.rs")).unwrap(),
            "ONE\ntwo\nthree\n"
        );
    }

    #[test]
    fn reports_an_overlay_that_has_recorded_nothing_as_empty() {
        assert!(Overlay::new().is_empty());
    }
}
