use crate::plan::FileHint;
use crate::Result;

/// What a v2 header says about the file at `path` as it stands: its hash.
pub(crate) fn hint_of(path: &std::path::Path) -> Result<FileHint> {
    Ok(FileHint {
        sha256: crate::apply::hash_file(path)?,
    })
}
