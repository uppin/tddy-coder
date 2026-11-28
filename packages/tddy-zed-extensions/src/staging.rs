//! The install's file moves: a staged tree (`installed/.<id>.partial`), the swap that puts it in
//! place of `installed/<id>` without losing the previous install on a refusal, and the copy and
//! removal they use.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;

/// Moves `installed/<id>` aside to `installed/.<id>.previous`, promotes the staged tree, then runs
/// `record` (the index save). A failed promote or record puts the previous tree back, so a refusal
/// at any step leaves the previous install where it was.
pub(crate) fn swap_in_on(
    fs: &dyn tddy_fs::Fs,
    dir: &ExtensionsDir,
    mut staging: Staging,
    id: &str,
    record: impl FnOnce() -> Result<(), ExtensionError>,
) -> Result<(), ExtensionError> {
    let target = dir.extension(id);
    let previous = dir.installed().join(format!(".{id}.previous"));
    remove_tree_on(fs, &previous)?;
    let had_previous = fs.metadata(&target).is_ok();
    if had_previous {
        fs.rename(&target, &previous).map_err(|e| ExtensionError::io(&target, e))?;
    }
    let restore = |error: ExtensionError| {
        // Best effort: the failure that caused the restore is the error the caller sees.
        let _ = remove_tree_on(fs, &target);
        if had_previous {
            let _ = fs.rename(&previous, &target);
        }
        error
    };
    staging.promote(fs, &target).map_err(restore)?;
    record().map_err(restore)?;
    // Best effort: the install is complete; a leftover `.previous` is cleared by the next swap.
    let _ = remove_tree_on(fs, &previous);
    Ok(())
}

/// Copies the regular files and directories under `from` into `to`, following symlinks so the copy
/// holds the content they name.
pub(crate) fn copy_tree_on(
    fs: &dyn tddy_fs::Fs,
    from: &Path,
    to: &Path,
) -> Result<(), ExtensionError> {
    // The port's `read_dir` hands back full paths, sorted.
    let entries = fs.read_dir(from).map_err(|e| ExtensionError::io(from, e))?;
    for source in entries {
        let target = to.join(
            source
                .file_name()
                .map(|name| name.to_os_string())
                .unwrap_or_default(),
        );
        let kind = fs
            .metadata_follow(&source)
            .map_err(|e| ExtensionError::io(&source, e))?;
        if kind.kind == tddy_fs::FsKind::Dir {
            fs.create_dir_all(&target).map_err(|e| ExtensionError::io(&target, e))?;
            copy_tree_on(fs, &source, &target)?;
        } else if kind.kind == tddy_fs::FsKind::File {
            fs.copy(&source, &target).map_err(|e| ExtensionError::io(&source, e))?;
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
    /// The port the drop's best-effort clean-up removes the tree through.
    fs: Arc<dyn tddy_fs::Fs>,
}

impl Staging {
    /// Creates `installed/` as needed and an empty partial directory for `id`, clearing any partial
    /// an interrupted install left.
    pub(crate) fn begin_on(
        fs: Arc<dyn tddy_fs::Fs>,
        dir: &ExtensionsDir,
        id: &str,
    ) -> Result<Self, ExtensionError> {
        let installed = dir.installed();
        fs.create_dir_all(&installed).map_err(|e| ExtensionError::io(&installed, e))?;
        let path = installed.join(format!(".{id}.partial"));
        remove_tree_on(&*fs, &path)?;
        fs.create_dir_all(&path).map_err(|e| ExtensionError::io(&path, e))?;
        Ok(Self {
            path,
            promoted: false,
            fs,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn promote(&mut self, fs: &dyn tddy_fs::Fs, target: &Path) -> Result<(), ExtensionError> {
        fs.rename(&self.path, target).map_err(|e| ExtensionError::io(target, e))?;
        self.promoted = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.promoted {
            // Best effort: the refusal that caused this drop is the error the caller sees.
            let _ = self.fs.remove_dir_all(&self.path);
        }
    }
}

/// Removes `path` if it is there: a directory with its contents, or a single file or link.
pub(crate) fn remove_tree_on(
    fs: &dyn tddy_fs::Fs,
    path: &Path,
) -> Result<(), ExtensionError> {
    let removed = match fs.metadata(path) {
        Ok(meta) if meta.kind == tddy_fs::FsKind::Dir => fs.remove_dir_all(path),
        Ok(_) => fs.remove_file(path),
        Err(e) => Err(e),
    };
    match removed {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ExtensionError::io(path, e)),
    }
}
