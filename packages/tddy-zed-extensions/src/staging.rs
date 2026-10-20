//! The install's file moves: a staged tree (`installed/.<id>.partial`), the swap that puts it in
//! place of `installed/<id>` without losing the previous install on a refusal, and the copy and
//! removal they use.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;

/// Moves `installed/<id>` aside to `installed/.<id>.previous`, promotes the staged tree, then runs
/// `record` (the index save). A failed promote or record puts the previous tree back, so a refusal
/// at any step leaves the previous install where it was.
pub(crate) fn swap_in(
    dir: &ExtensionsDir,
    staging: Staging,
    id: &str,
    record: impl FnOnce() -> Result<(), ExtensionError>,
) -> Result<(), ExtensionError> {
    let target = dir.extension(id);
    let previous = dir.installed().join(format!(".{id}.previous"));
    remove_tree(&previous)?;
    let had_previous = fs::symlink_metadata(&target).is_ok();
    if had_previous {
        fs::rename(&target, &previous).map_err(|e| ExtensionError::io(&target, e))?;
    }
    let restore = |error: ExtensionError| {
        // Best effort: the failure that caused the restore is the error the caller sees.
        let _ = remove_tree(&target);
        if had_previous {
            let _ = fs::rename(&previous, &target);
        }
        error
    };
    staging.promote(&target).map_err(restore)?;
    record().map_err(restore)?;
    // Best effort: the install is complete; a leftover `.previous` is cleared by the next swap.
    let _ = remove_tree(&previous);
    Ok(())
}

/// Copies the regular files and directories under `from` into `to`, following symlinks so the copy
/// holds the content they name.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), ExtensionError> {
    let entries = fs::read_dir(from).map_err(|e| ExtensionError::io(from, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| ExtensionError::io(from, e))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = fs::metadata(&source).map_err(|e| ExtensionError::io(&source, e))?;
        if kind.is_dir() {
            fs::create_dir(&target).map_err(|e| ExtensionError::io(&target, e))?;
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(&source, &target).map_err(|e| ExtensionError::io(&source, e))?;
        } else {
            return Err(ExtensionError::io(
                &source,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "only regular files and directories are copied",
                ),
            ));
        }
    }
    Ok(())
}

/// `installed/.<id>.partial`: the tree being staged. It is removed on drop unless
/// [`Staging::promote`] renamed it into place, so a refusal at any step leaves nothing behind.
pub(crate) struct Staging {
    path: PathBuf,
    promoted: bool,
}

impl Staging {
    /// Creates `installed/` as needed and an empty partial directory for `id`, clearing any partial
    /// an interrupted install left.
    pub(crate) fn begin(dir: &ExtensionsDir, id: &str) -> Result<Self, ExtensionError> {
        let installed = dir.installed();
        fs::create_dir_all(&installed).map_err(|e| ExtensionError::io(&installed, e))?;
        let path = installed.join(format!(".{id}.partial"));
        remove_tree(&path)?;
        fs::create_dir(&path).map_err(|e| ExtensionError::io(&path, e))?;
        Ok(Self {
            path,
            promoted: false,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn promote(mut self, target: &Path) -> Result<(), ExtensionError> {
        fs::rename(&self.path, target).map_err(|e| ExtensionError::io(target, e))?;
        self.promoted = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.promoted {
            // Best effort: the refusal that caused this drop is the error the caller sees.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Removes `path` if it is there: a directory with its contents, or a single file or link.
pub(crate) fn remove_tree(path: &Path) -> Result<(), ExtensionError> {
    let removed = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) => Err(e),
    };
    match removed {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ExtensionError::io(path, e)),
    }
}
