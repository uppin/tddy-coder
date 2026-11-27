//! Host-side materialization of a [`crate::SandboxPlan`]'s copies, symlinks, and secrets.
//!
//! These run just before spawn (both backends share them): they create the concrete files,
//! symlinks, and `0600` secret files the rendered policy already accounts for.

use std::path::Path;

use crate::builder::{CopySpec, SecretSource, SecretSpec, SymlinkSpec};

/// Copy each [`CopySpec`] into the jail tree. Missing sources are skipped when `optional`.
pub fn materialize_copies(copies: &[CopySpec]) -> Result<(), String> {
    materialize_copies_on(&tddy_fs::DiskFs, copies)
}

/// [`materialize_copies`] through the filesystem port.
pub fn materialize_copies_on(fs: &dyn tddy_fs::Fs, copies: &[CopySpec]) -> Result<(), String> {
    for c in copies {
        if !fs.exists(&c.src) {
            if c.optional {
                continue;
            }
            return Err(format!("copy source missing: {}", c.src.display()));
        }
        if let Some(parent) = c.dest.parent() {
            fs.create_dir_all(parent)
                .map_err(|e| format!("create copy dest dir {}: {e}", parent.display()))?;
        }
        fs.copy(&c.src, &c.dest)
            .map_err(|e| format!("copy {} -> {}: {e}", c.src.display(), c.dest.display()))?;
        if let Some(mode) = c.mode {
            set_mode(fs, &c.dest, mode)?;
        }
    }
    Ok(())
}

/// Create each [`SymlinkSpec`] inside the jail tree (parent dirs created as needed).
pub fn materialize_symlinks(symlinks: &[SymlinkSpec]) -> Result<(), String> {
    materialize_symlinks_on(&tddy_fs::DiskFs, symlinks)
}

/// [`materialize_symlinks`] through the filesystem port.
pub fn materialize_symlinks_on(fs: &dyn tddy_fs::Fs, symlinks: &[SymlinkSpec]) -> Result<(), String> {
    for s in symlinks {
        if let Some(parent) = s.link.parent() {
            fs.create_dir_all(parent)
                .map_err(|e| format!("create symlink dir {}: {e}", parent.display()))?;
        }
        let _ = fs.remove_file(&s.link);
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
    materialize_secrets_on(&tddy_fs::DiskFs, secrets, scratch_dir)
}

/// [`materialize_secrets`] through the filesystem port.
pub fn materialize_secrets_on(fs: &dyn tddy_fs::Fs, secrets: &[SecretSpec], scratch_dir: &Path) -> Result<(), String> {
    if secrets.is_empty() {
        return Ok(());
    }
    let dir = scratch_dir.join(".secrets");
    fs.create_dir_all(&dir)
        .map_err(|e| format!("create secrets dir {}: {e}", dir.display()))?;
    set_mode(fs, &dir, 0o700)?;
    for sec in secrets {
        let value = match &sec.source {
            SecretSource::Value(v) => v.clone(),
            SecretSource::HostFile(path) => fs
                .read_to_string(path)
                .map_err(|e| format!("read secret file {}: {e}", path.display()))?,
        };
        let dest = dir.join(&sec.env_name);
        fs.write(&dest, value.as_bytes())
            .map_err(|e| format!("write secret {}: {e}", dest.display()))?;
        set_mode(fs, &dest, 0o600)?;
    }
    Ok(())
}

fn set_mode(fs: &dyn tddy_fs::Fs, path: &Path, mode: u32) -> Result<(), String> {
    // The port's `set_mode` is a no-op off Unix, as the `cfg` pair this replaces was.
    fs.set_mode(path, mode)
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    // `libc::symlink` directly: the port has no link-creation operation, and creating a
    // link is a name-space act, not file content. Same call `std::os::unix::fs::symlink` made.
    use std::os::unix::ffi::OsStrExt as _;
    let c_target = std::ffi::CString::new(target.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidFilename, "target contains NUL"))?;
    let c_link = std::ffi::CString::new(link.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidFilename, "link contains NUL"))?;
    if unsafe { libc::symlink(c_target.as_ptr(), c_link.as_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks unsupported on this platform",
    ))
}
