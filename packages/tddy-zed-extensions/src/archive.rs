//! Unpacks a registry archive into a staged extension directory (R2, R3). Each entry is checked
//! before anything is written: a path that is absolute or climbs out of the extension is refused,
//! and so is a link whose target is absolute or climbs out, and any entry whose path, or whose link
//! target, already runs through a symlink in the tree. `tar`'s `unpack_in` is not used, because it
//! reports a different refusal.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::error::ExtensionError;

/// Writes the tar stream `reader` (already gunzipped) into `staged`, the partial directory of
/// `extension`. The first refused entry stops the unpack; the caller removes `staged`.
pub(crate) fn unpack<R: Read>(
    reader: R,
    staged: &Path,
    extension: &str,
) -> Result<(), ExtensionError> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|e| ExtensionError::io(staged, e))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| ExtensionError::io(staged, e))?;
        let name = entry
            .path()
            .map_err(|e| ExtensionError::io(staged, e))?
            .into_owned();
        if !is_contained(&name) || crosses_symlink(staged, &name)? {
            return Err(unsafe_entry(extension, &name));
        }
        let dest = staged.join(&name);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| ExtensionError::io(&dest, e))?;
        } else if kind.is_symlink() {
            let target = link_target(&entry, staged, extension, &name)?;
            if !link_stays_inside(&name, &target, false) {
                return Err(unsafe_entry(extension, &name));
            }
            create_parent(&dest)?;
            entry
                .unpack(&dest)
                .map_err(|e| ExtensionError::io(&dest, e))?;
        } else if kind.is_hard_link() {
            let target = link_target(&entry, staged, extension, &name)?;
            if !link_stays_inside(&name, &target, true) || crosses_symlink(staged, &target)? {
                return Err(unsafe_entry(extension, &name));
            }
            create_parent(&dest)?;
            // `Entry::unpack` resolves a hard link's target against the working directory, so the
            // link is made here, against the staged tree.
            fs::hard_link(staged.join(&target), &dest).map_err(|e| ExtensionError::io(&dest, e))?;
        } else if kind.is_file() {
            create_parent(&dest)?;
            entry
                .unpack(&dest)
                .map_err(|e| ExtensionError::io(&dest, e))?;
        } else {
            return Err(ExtensionError::io(
                &dest,
                io::Error::new(io::ErrorKind::Unsupported, "not a file, directory or link"),
            ));
        }
    }
    Ok(())
}

/// Whether `path` is relative and never climbs out: only normal names and `.`.
pub(crate) fn is_contained(path: &Path) -> bool {
    path.components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

/// Whether a link at `link` (an archive path) to `target` stays under the extension root. A
/// symlink's target resolves from the directory holding the link; a hard link's target is an
/// archive path, so it resolves from the root.
fn link_stays_inside(link: &Path, target: &Path, hard: bool) -> bool {
    if target
        .components()
        .any(|part| matches!(part, Component::RootDir | Component::Prefix(_)))
    {
        return false;
    }
    let mut depth = if hard {
        0
    } else {
        link.components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count()
            .saturating_sub(1)
    };
    for part in target.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::ParentDir => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    true
}

/// Whether any component of `rel` that exists under `staged` is a symlink, the last included.
/// Writing or linking through one could land outside the tree.
fn crosses_symlink(staged: &Path, rel: &Path) -> Result<bool, ExtensionError> {
    let mut walked = staged.to_path_buf();
    for part in rel.components() {
        if let Component::Normal(name) = part {
            walked.push(name);
            match fs::symlink_metadata(&walked) {
                Ok(meta) if meta.file_type().is_symlink() => return Ok(true),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(ExtensionError::io(&walked, e)),
            }
        }
    }
    Ok(false)
}

/// The target a symlink or hard-link entry names. A link with no target is refused.
fn link_target<R: Read>(
    entry: &tar::Entry<'_, R>,
    staged: &Path,
    extension: &str,
    name: &Path,
) -> Result<PathBuf, ExtensionError> {
    match entry.link_name() {
        Ok(Some(target)) => Ok(target.into_owned()),
        Ok(None) => Err(unsafe_entry(extension, name)),
        Err(e) => Err(ExtensionError::io(staged, e)),
    }
}

fn create_parent(dest: &Path) -> Result<(), ExtensionError> {
    match dest.parent() {
        Some(parent) => fs::create_dir_all(parent).map_err(|e| ExtensionError::io(parent, e)),
        None => Ok(()),
    }
}

fn unsafe_entry(extension: &str, entry: &Path) -> ExtensionError {
    ExtensionError::UnsafeArchiveEntry {
        extension: extension.to_string(),
        entry: entry.to_path_buf(),
    }
}
