use serde::{Deserialize, Serialize};

use crate::Result;

use std::path::Path;

/// The bytes of one file before a transactional group first touched it.
///
/// What a rollback writes back. A file the group created has no bytes to restore — `contents` is
/// `None` — so restoring it removes the file; a file a member renamed is two pre-images, the source
/// with its bytes and the destination with none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreImage {
    /// Relative to the run's root.
    pub path: String,
    /// The file's text before the group touched it, or `None` when it did not exist.
    pub contents: Option<String>,
}

impl PreImage {
    /// The file at `path` under `root` as it stands now, or its absence.
    pub fn capture(root: &Path, path: &str) -> Result<PreImage> {
        let contents = match std::fs::read_to_string(root.join(path)) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        Ok(PreImage {
            path: path.to_string(),
            contents,
        })
    }

    /// Put the file at `path` under `root` back as it was captured: rewritten, or removed when it
    /// did not exist.
    pub fn restore(&self, root: &Path) -> Result<()> {
        let file = root.join(&self.path);
        match &self.contents {
            Some(contents) => {
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file, contents)?;
            }
            None => match std::fs::remove_file(&file) {
                Ok(()) => {}
                // Already absent is the state being restored.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
        }
        Ok(())
    }
}

/// A group the journal started and neither completed nor rolled back — what a resume finds after a
/// crash inside one, and must roll back before it runs anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGroup {
    pub group: String,
    /// The plan indices of every member, in plan order.
    pub members: Vec<usize>,
    /// Every pre-image the group's members journalled, in the order they were written.
    pub pre_images: Vec<PreImage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding `files` (relative path, contents).
    fn a_tree_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        for (path, text) in files {
            let file = root.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("created");
            std::fs::write(file, text).expect("written");
        }
        root
    }

    #[test]
    fn restoring_a_file_the_group_created_removes_the_directory_that_emptied() {
        // Given a file the group created in a directory of its own, beside a file that was there
        let root = a_tree_holding(&[
            ("src/lib.rs", "pub mod split;\n"),
            ("src/split/attachments.rs", "pub fn f() {}\n"),
        ]);
        let created = PreImage {
            path: "src/split/attachments.rs".to_string(),
            contents: None,
        };

        // When the rollback restores it
        created
            .restore(root.path())
            .expect("the creation is undone");

        // Then the file and the directory it emptied are gone, and the rest stays
        assert!(
            !root.path().join("src/split").exists(),
            "the rollback left the directory it emptied"
        );
        assert!(root.path().join("src/lib.rs").exists());
    }

    #[test]
    fn restoring_a_renamed_file_recreates_the_directory_the_sweep_removed() {
        // Given a file whose directory a move emptied and the sweep removed
        let root = a_tree_holding(&[("src/lib.rs", "pub mod host;\n")]);
        let renamed_away = PreImage {
            path: "src/host/attachments.rs".to_string(),
            contents: Some("pub fn f() {}\n".to_string()),
        };

        // When the rollback restores it
        renamed_away
            .restore(root.path())
            .expect("the rename is undone");

        // Then the directory is created again and holds the file as it was
        assert_eq!(
            std::fs::read_to_string(root.path().join("src/host/attachments.rs"))
                .expect("the file is back"),
            "pub fn f() {}\n"
        );
    }
}
