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
