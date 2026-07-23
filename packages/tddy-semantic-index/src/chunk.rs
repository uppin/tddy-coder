//! Worktree walk + text chunking.

use std::path::Path;

use walkdir::WalkDir;

/// Skip files larger than this — oversized/generated files aren't useful index material and bloat
/// the store. One whole-file chunk per text file is the v1 chunking strategy.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// One indexable slice of a worktree file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Worktree-relative path of the source file.
    pub source_path: String,
    /// The chunk's text content.
    pub text: String,
}

/// Walk `root` and split its indexable text files into [`Chunk`]s (worktree-relative paths).
///
/// v1 emits one whole-file chunk per text file. Directories, symlinks, oversized files, and files
/// whose bytes are not valid UTF-8 (a cheap binary filter) are skipped.
pub fn chunk_worktree(root: &Path) -> anyhow::Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        // Non-UTF-8 bytes signal a binary file; skip rather than index garbage.
        let text = match std::fs::read(path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let relative = path.strip_prefix(root).unwrap_or(path);
        chunks.push(Chunk {
            source_path: relative.to_string_lossy().into_owned(),
            text,
        });
    }
    Ok(chunks)
}
