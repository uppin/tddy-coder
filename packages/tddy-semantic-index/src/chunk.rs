//! Worktree walk + text chunking.

use std::path::Path;


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
    chunk_worktree_on(&tddy_fs::DiskFs, root)
}

/// [`chunk_worktree`] through the filesystem port: the same walk, with symlinks not
/// followed (a symlink is neither a file nor a directory to descend into here).
pub fn chunk_worktree_on(fs: &dyn tddy_fs::Fs, root: &Path) -> anyhow::Result<Vec<Chunk>> {
    let mut chunks = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for path in fs.read_dir(&dir)? {
            let metadata = fs.metadata(&path)?;
            match metadata.kind {
                tddy_fs::FsKind::Dir => pending.push(path),
                tddy_fs::FsKind::File => {
                    if metadata.len > MAX_FILE_BYTES {
                        continue;
                    }
                    // Non-UTF-8 bytes signal a binary file; skip rather than index garbage.
                    let text = match fs.read(&path) {
                        Ok(bytes) => match String::from_utf8(bytes) {
                            Ok(text) => text,
                            Err(_) => continue,
                        },
                        Err(_) => continue,
                    };
                    let relative = path.strip_prefix(root).unwrap_or(&path);
                    chunks.push(Chunk {
                        source_path: relative.to_string_lossy().into_owned(),
                        text,
                    });
                }
                _ => continue,
            }
        }
    }
    Ok(chunks)
}
