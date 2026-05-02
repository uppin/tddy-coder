//! Resolve declared path bindings strictly inside the session tree and optional repo root.

use std::path::{Component, Path, PathBuf};

use log::debug;

use super::error::SessionActionsError;

/// Locate `actions/<action_id>.{yaml,yml}` under a session directory (same rules as `invoke-action` CLI).
pub fn resolve_action_manifest_path(
    session_dir: &Path,
    action_id: &str,
) -> Result<PathBuf, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::paths",
        "resolve_action_manifest_path action_id={} session_dir={}",
        action_id,
        session_dir.display()
    );
    let actions_dir = session_dir.join("actions");
    for ext in ["yaml", "yml"] {
        let cand = actions_dir.join(format!("{action_id}.{ext}"));
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(SessionActionsError::UnknownActionId(action_id.to_string()))
}

/// Ensure a path requested by JSON args stays within allowed roots (session + declared repo).
pub fn resolve_allowlisted_path(
    session_dir: &Path,
    repo_root: Option<&Path>,
    raw: &str,
    purpose: &'static str,
) -> Result<PathBuf, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::paths",
        "resolve_allowlisted_path: purpose={purpose} raw={raw} session_dir={} repo_root={:?}",
        session_dir.display(),
        repo_root.map(|p| p.display().to_string())
    );

    if raw.trim().is_empty() {
        return Err(SessionActionsError::PathTraversalAttempt {
            purpose,
            reason: "path is empty".into(),
        });
    }

    let session_canon = std::fs::canonicalize(session_dir).map_err(SessionActionsError::Io)?;

    let repo_canon = if let Some(r) = repo_root.filter(|p| !p.as_os_str().is_empty()) {
        Some(std::fs::canonicalize(r).map_err(SessionActionsError::Io)?)
    } else {
        None
    };

    let normalized = if Path::new(raw).is_absolute() {
        lexical_normalize_path(Path::new(raw))
    } else {
        join_under_root(&session_canon, raw, purpose)?
    };

    debug!(
        target: "tddy_core::session_actions::paths",
        "normalized candidate={} session_canon={} repo_canon={:?} ({purpose})",
        normalized.display(),
        session_canon.display(),
        repo_canon.as_ref().map(|p| p.display().to_string())
    );

    if path_is_within(&normalized, &session_canon) {
        return Ok(normalized);
    }
    if let Some(ref rr) = repo_canon {
        if path_is_within(&normalized, rr) {
            return Ok(normalized);
        }
    }

    Err(SessionActionsError::PathOutsideAllowlist {
        path: normalized.display().to_string(),
        purpose,
    })
}

/// Join relative `user` onto `session_root` (canonical), rejecting traversal above `session_root`.
fn join_under_root(
    session_root: &Path,
    user: &str,
    purpose: &'static str,
) -> Result<PathBuf, SessionActionsError> {
    let mut cur = session_root.to_path_buf();
    for part in split_path_parts(user) {
        match part.as_str() {
            "" | "." => {}
            ".." => {
                if cur == *session_root {
                    return Err(SessionActionsError::PathTraversalAttempt {
                        purpose,
                        reason: "`..` escapes above the session root".into(),
                    });
                }
                cur.pop();
            }
            seg => cur.push(seg),
        }
        if !path_is_within(&cur, session_root) {
            return Err(SessionActionsError::PathTraversalAttempt {
                purpose,
                reason: "normalized path escapes the session directory".into(),
            });
        }
    }
    Ok(cur)
}

fn split_path_parts(user: &str) -> Vec<String> {
    user.split(['/', '\\'])
        .map(std::borrow::ToOwned::to_owned)
        .collect()
}

/// Normalize `.`, `..`, and duplicate separators using path components only (no filesystem access).
fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => out.push(Component::Prefix(p)),
            Component::RootDir => {
                out.push(c);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    let _ = out.pop();
                }
            }
            Component::Normal(name) => out.push(name),
        }
    }
    out
}

fn path_is_within(child: &Path, root: &Path) -> bool {
    child.starts_with(root)
}
