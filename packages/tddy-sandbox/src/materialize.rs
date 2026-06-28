//! Host-side materialization of a [`crate::SandboxPlan`]'s copies, symlinks, and secrets.
//!
//! These run just before spawn (both backends share them): they create the concrete files,
//! symlinks, and `0600` secret files the rendered policy already accounts for.

use std::path::Path;

use crate::builder::{CopySpec, SecretSource, SecretSpec, SymlinkSpec};

/// Copy each [`CopySpec`] into the jail tree. Missing sources are skipped when `optional`.
pub fn materialize_copies(copies: &[CopySpec]) -> Result<(), String> {
    for c in copies {
        if !c.src.exists() {
            if c.optional {
                continue;
            }
            return Err(format!("copy source missing: {}", c.src.display()));
        }
        if let Some(parent) = c.dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create copy dest dir {}: {e}", parent.display()))?;
        }
        std::fs::copy(&c.src, &c.dest)
            .map_err(|e| format!("copy {} -> {}: {e}", c.src.display(), c.dest.display()))?;
        if let Some(mode) = c.mode {
            set_mode(&c.dest, mode)?;
        }
    }
    Ok(())
}

/// Create each [`SymlinkSpec`] inside the jail tree (parent dirs created as needed).
pub fn materialize_symlinks(symlinks: &[SymlinkSpec]) -> Result<(), String> {
    for s in symlinks {
        if let Some(parent) = s.link.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create symlink dir {}: {e}", parent.display()))?;
        }
        let _ = std::fs::remove_file(&s.link);
        symlink(&s.target, &s.link).map_err(|e| {
            format!(
                "symlink {} -> {}: {e}",
                s.link.display(),
                s.target.display()
            )
        })?;
    }
    Ok(())
}

/// Write each secret value to `scratch_dir/.secrets/<env_name>` at mode `0600`. The path matches
/// the `TDDY_SECRET_<env_name>` env reference the builder placed in the plan.
pub fn materialize_secrets(secrets: &[SecretSpec], scratch_dir: &Path) -> Result<(), String> {
    if secrets.is_empty() {
        return Ok(());
    }
    let dir = scratch_dir.join(".secrets");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create secrets dir {}: {e}", dir.display()))?;
    set_mode(&dir, 0o700)?;
    for sec in secrets {
        let value = match &sec.source {
            SecretSource::Value(v) => v.clone(),
            SecretSource::HostFile(path) => std::fs::read_to_string(path)
                .map_err(|e| format!("read secret file {}: {e}", path.display()))?,
        };
        let dest = dir.join(&sec.env_name);
        std::fs::write(&dest, value)
            .map_err(|e| format!("write secret {}: {e}", dest.display()))?;
        set_mode(&dest, 0o600)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks unsupported on this platform",
    ))
}
