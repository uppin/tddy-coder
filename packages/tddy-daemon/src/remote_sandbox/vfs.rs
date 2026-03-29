//! Sandbox VFS path rules (symlink-safe, no traversal).

use tddy_service::sandbox_path;

/// Returns `Ok(())` when `rel` is a safe relative path under the sandbox root (no `..`, absolute, or empty segments).
pub fn ensure_relative_under_root(rel: &str) -> Result<(), &'static str> {
    sandbox_path::sandbox_relative_path(rel).map(|_| ())
}
