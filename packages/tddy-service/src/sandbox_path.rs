//! Shared sandbox-relative path rules for daemon VFS, `RemoteSandboxService`, and `tddy-remote`.
//!
//! All paths must be relative to the per-session sandbox root: no `..`, no absolute paths, UTF-8 segments only.

use std::path::{Component, Path, PathBuf};

/// Normalize a user-supplied path within the sandbox root (reject `..`, absolute paths, empty).
pub fn sandbox_relative_path(raw: &str) -> Result<PathBuf, &'static str> {
    let p = raw.trim().trim_start_matches('/');
    if p.is_empty() {
        return Err("path is empty");
    }
    let pb = Path::new(p);
    if pb.is_absolute() {
        return Err("absolute paths are not allowed");
    }
    let mut out = PathBuf::new();
    for c in pb.components() {
        match c {
            Component::Normal(s) => {
                let Some(seg) = s.to_str() else {
                    return Err("path must be UTF-8");
                };
                if seg.is_empty() {
                    continue;
                }
                if seg == ".." {
                    return Err("path escapes sandbox root (..)");
                }
                out.push(seg);
            }
            Component::CurDir => {}
            Component::ParentDir => return Err("path escapes sandbox root (..)"),
            Component::Prefix(_) | Component::RootDir => {
                return Err("absolute paths are not allowed");
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err("path resolves to empty");
    }
    Ok(out)
}
