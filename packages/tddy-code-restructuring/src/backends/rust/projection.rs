//! The plan's projected tree, shown to the server before an operation asks it anything.
//!
//! A run resolves its operations one after another, and every operation after the first is about a
//! tree the earlier ones changed. The engine's own reads already see that tree — through the
//! [`crate::Overlay`] in a rehearsal, through the disk in an apply — but the server does not: each
//! operation opens only its anchor, a rehearsal's files never reach the disk, and rust-analyzer's own
//! watcher is blind to an apply's writes within one request on this repository's workspace (see
//! `tddy-index-daemon/src/tree_changes.rs`). A seam cut after another one was therefore surveyed,
//! imported and narrowed against a tree without its sibling's file.
//!
//! So the backend remembers which files its resolutions wrote, and opens each of them, at the text
//! the run holds for it, before the next operation's first document.

use crate::edit::WorkspaceEdit;
use crate::registry::Workspace;
use crate::Result;

use super::RustBackend;

/// The files this backend's resolutions created, changed or renamed into, in the order the run first
/// wrote each one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct PlanProjection {
    written: Vec<String>,
}

impl PlanProjection {
    /// Fold one resolved edit in: a created or changed file is added, a rename follows the file to
    /// its new path and forgets the old one. A path already listed keeps its place.
    pub(super) fn record(&mut self, edit: &WorkspaceEdit) {
        // TODO(reshape-multi-seam-extract): implement
        let _ = (&mut self.written, edit);
        todo!("fold a resolved edit into the projection")
    }

    /// Every file the run has written so far, in the order it first wrote it.
    pub(super) fn paths(&self) -> &[String] {
        // TODO(reshape-multi-seam-extract): implement
        let _ = &self.written;
        todo!("the projected paths")
    }
}

impl RustBackend {
    /// Read every projected file through `workspace` and hold it for the operation's first
    /// `did_open`. A recorded file the workspace cannot read is an error naming it.
    pub(super) fn stage_projection(&mut self, workspace: &Workspace<'_>) -> Result<()> {
        // TODO(reshape-multi-seam-extract): implement
        let _ = workspace;
        todo!("stage the projected documents")
    }

    /// Open every staged document but `except`, the one the operation is opening itself.
    #[expect(
        dead_code,
        reason = "TODO(reshape-multi-seam-extract): `did_open` calls this in the green phase"
    )]
    pub(super) fn open_staged_projection(&mut self, except: &str) -> Result<()> {
        // TODO(reshape-multi-seam-extract): implement
        let _ = except;
        todo!("open the staged projected documents")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{FileEdit, Position, Range, TextEdit};
    use crate::Overlay;

    fn creating(path: &str) -> FileEdit {
        FileEdit::Create {
            path: path.to_string(),
        }
    }

    fn changing(path: &str) -> FileEdit {
        writing(path, "// edited\n")
    }

    fn writing(path: &str, text: &str) -> FileEdit {
        FileEdit::Change {
            path: path.to_string(),
            edits: vec![TextEdit {
                range: Range {
                    start: Position { line: 1, col: 1 },
                    end: Position { line: 1, col: 1 },
                },
                new_text: text.to_string(),
            }],
        }
    }

    fn renaming(from: &str, to: &str) -> FileEdit {
        FileEdit::Rename {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    fn an_edit_of(changes: Vec<FileEdit>) -> WorkspaceEdit {
        WorkspaceEdit { changes }
    }

    fn a_projection_of(edits: &[WorkspaceEdit]) -> PlanProjection {
        let mut projection = PlanProjection::default();
        for edit in edits {
            projection.record(edit);
        }
        projection
    }

    #[test]
    fn records_every_file_an_edit_creates_or_changes() {
        // Given a seam written to a file of its own
        let seam = an_edit_of(vec![
            creating("src/outer/pty_handle.rs"),
            changing("src/outer/pty_handle.rs"),
            changing("src/outer.rs"),
        ]);

        // When it is recorded
        let projection = a_projection_of(&[seam]);

        // Then both the new file and its parent are projected
        assert_eq!(
            projection.paths(),
            ["src/outer/pty_handle.rs", "src/outer.rs"]
        );
    }

    #[test]
    fn follows_a_rename_to_its_new_path_and_forgets_the_old_one() {
        // Given a file written by one operation and moved by the next
        let edits = [
            an_edit_of(vec![changing("src/host.rs")]),
            an_edit_of(vec![renaming("src/host.rs", "src/kernel/host.rs")]),
        ];

        // When both are recorded
        let projection = a_projection_of(&edits);

        // Then only the new path is projected
        assert_eq!(projection.paths(), ["src/kernel/host.rs"]);
    }

    #[test]
    fn lists_each_path_once_in_the_order_the_plan_first_wrote_it() {
        // Given three seams cut out of one parent
        let edits = [
            an_edit_of(vec![creating("src/outer/a.rs"), changing("src/outer.rs")]),
            an_edit_of(vec![creating("src/outer/b.rs"), changing("src/outer.rs")]),
            an_edit_of(vec![changing("src/outer/a.rs"), changing("src/outer.rs")]),
        ];

        // When they are recorded
        let projection = a_projection_of(&edits);

        // Then each file is listed once, where the run first wrote it
        assert_eq!(
            projection.paths(),
            ["src/outer/a.rs", "src/outer.rs", "src/outer/b.rs"]
        );
    }

    #[test]
    fn an_empty_record_stages_nothing_so_a_first_operation_sends_what_it_sent_before() {
        // Given a backend that has resolved nothing yet
        let directory = tempfile::tempdir().expect("a temporary directory");
        let overlay = Overlay::new();
        let workspace = Workspace {
            root: directory.path(),
            overlay: &overlay,
        };
        let mut backend = RustBackend::new("rust-analyzer", "/cargo", "/rustup");

        // When the first operation stages the projection
        backend
            .stage_projection(&workspace)
            .expect("an empty projection stages");

        // Then nothing is held for the server
        assert_eq!(backend.projection.paths(), [] as [&str; 0]);
        assert!(backend.staged_projection.is_empty());
    }

    #[test]
    fn staging_reads_each_path_through_the_workspace_and_an_unreadable_path_is_an_error_naming_it()
    {
        // Given a projection holding one file the overlay has and one the tree never held
        let directory = tempfile::tempdir().expect("a temporary directory");
        let root = directory.path();
        let mut overlay = Overlay::new();
        overlay
            .record(
                root,
                &an_edit_of(vec![
                    creating("src/outer/resize.rs"),
                    writing("src/outer/resize.rs", "pub(crate) fn strip_resize() {}\n"),
                ]),
            )
            .expect("the overlay records the seam");
        let workspace = Workspace {
            root,
            overlay: &overlay,
        };
        let mut backend = RustBackend::new("rust-analyzer", "/cargo", "/rustup");
        backend.projection.record(&an_edit_of(vec![
            creating("src/outer/resize.rs"),
            changing("src/outer/gone.rs"),
        ]));

        // When the projection is staged
        let staged = backend.stage_projection(&workspace);

        // Then the file the tree never held is named
        let refusal = staged
            .expect_err("an unreadable projected file is an error")
            .to_string();
        assert!(
            refusal.contains("src/outer/gone.rs"),
            "the error does not name the file it could not read: {refusal}"
        );
    }
}
