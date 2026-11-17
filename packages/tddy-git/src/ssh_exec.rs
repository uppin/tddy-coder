//! OpenSSH helpers for exec-catalog and worktree operations on an SSH target.
//!
//! The code-managing daemon shells out to `ssh -o BatchMode=yes`; there is no fallback to the
//! host filesystem when the child fails.

use std::path::Component;

/// Escape a string for a single-quoted POSIX shell argument.
pub fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// Contain `arg_path` against an absolute worktree root on the SSH target (string paths only).
pub fn contain_remote_path(remote_root: &str, arg_path: &str) -> Result<String, String> {
    let root = remote_root.trim().trim_end_matches('/');
    if root.is_empty() {
        return Err("remote worktree root is empty".to_string());
    }

    let candidate = if arg_path.starts_with('/') {
        arg_path.to_string()
    } else {
        format!("{}/{}", root, arg_path.trim_start_matches('/'))
    };

    for component in std::path::Path::new(&candidate).components() {
        if component == Component::ParentDir {
            return Err(format!("path contains '..' component: {candidate}"));
        }
    }

    let normalized = candidate.replace("//", "/");
    if normalized != root && !normalized.starts_with(&format!("{}/", root)) {
        return Err(format!("resolved path escapes worktree: {normalized}"));
    }
    Ok(normalized)
}
