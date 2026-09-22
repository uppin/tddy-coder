//! Resolve declared path bindings strictly inside the session tree and optional repo root.

use std::path::{Component, Path, PathBuf};

use log::debug;

use super::error::SessionActionsError;

/// Derive a stable, filesystem-safe directory key from a canonical repo root path.
///
/// Strips the leading `/` and replaces `/` with `--` so the result is a safe single directory
/// name under `~/.tddy/actions/`.
///
/// Example: `/home/user/projects/myrepo` → `home--user--projects--myrepo`.
pub fn derive_repo_key(canonical_repo_root: &Path) -> String {
    canonical_repo_root
        .to_string_lossy()
        .trim_start_matches('/')
        .replace('/', "--")
}

/// Per-repo action store root: `<tddy_data_dir>/actions/<repo_key>/`.
///
/// `tddy_data_dir` is typically `~/.tddy/` (from [`crate::output::tddy_data_dir_path`]).
pub fn repo_actions_root(tddy_data_dir: &Path, repo_key: &str) -> PathBuf {
    tddy_data_dir.join("actions").join(repo_key)
}

/// Locate `actions/<action_id>.{yaml,yml}` under one or more roots.
///
/// Lookup order:
/// 1. Per-repo store: `<store_root>/<action_id>.{yaml,yml}` (if `store_root` is provided).
/// 2. Session overlay: `<session_dir>/actions/<action_id>.{yaml,yml}` (if `session_dir` is provided).
///
/// `action_id` may contain subdirectory components (e.g. `packages/foo/build`); the traversal-safe
/// helpers below prevent escape outside each root.
pub fn resolve_action_manifest_path(
    session_dir: Option<&Path>,
    store_root: Option<&Path>,
    action_id: &str,
) -> Result<PathBuf, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::paths",
        "resolve_action_manifest_path action_id={} session_dir={:?} store_root={:?}",
        action_id,
        session_dir.map(|p| p.display().to_string()),
        store_root.map(|p| p.display().to_string()),
    );

    // Validate that the action_id doesn't attempt traversal (it may contain '/' for subdirs
    // but must not contain '..' components).
    for part in action_id.split('/') {
        if part == ".." || part == "." {
            return Err(SessionActionsError::PathTraversalAttempt {
                purpose: "action_id",
                reason: format!("action_id segment `{part}` is not allowed"),
            });
        }
    }

    // 1. Try per-repo store first.
    if let Some(store) = store_root {
        for ext in ["yaml", "yml"] {
            let cand = store.join(format!("{action_id}.{ext}"));
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }

    // 2. Try session overlay (legacy flat or nested).
    if let Some(session) = session_dir {
        let actions_dir = session.join("actions");
        for ext in ["yaml", "yml"] {
            let cand = actions_dir.join(format!("{action_id}.{ext}"));
            if cand.is_file() {
                return Ok(cand);
            }
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
