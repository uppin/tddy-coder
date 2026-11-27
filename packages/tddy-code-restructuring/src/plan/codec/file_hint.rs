use crate::plan::FileHint;
use crate::Result;

/// What a v2 header says about the file at `path` as it stands: its hash.
pub(crate) fn hint_of(path: &std::path::Path) -> Result<FileHint> {
    hint_of_on(&tddy_fs::DiskFs, path)
}

/// [`hint_of`] through the filesystem port.
pub(crate) fn hint_of_on(fs: &dyn tddy_fs::Fs, path: &std::path::Path) -> Result<FileHint> {
    Ok(FileHint {
        sha256: crate::apply::hash_file_on(fs, path)?,
    })
}
