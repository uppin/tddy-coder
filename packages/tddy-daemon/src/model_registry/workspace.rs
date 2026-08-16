//! Where a model chat's tools are allowed to run.
//!
//! `NewSessionRequest.cwd` is a plain string chosen by whoever holds a session token, and an
//! assistant may be assigned `Shell`. Taking that path at face value would let any token holder run
//! commands anywhere the daemon process can reach. So the path is resolved against the *caller's
//! own* roots — the same token → OS user → sessions/projects preamble every other session-addressed
//! surface uses — and anything outside them is refused before a tool exists to run.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::error::ModelRegistryError;

/// The directories one caller's chat tools may run in, resolved from their session token.
///
/// A function rather than a set of paths because the answer depends on who is asking, and a chat
/// stream learns that only when `new_session` presents its token.
pub type ChatWorkspaceRoots =
    Arc<dyn Fn(&str) -> Result<Vec<PathBuf>, ModelRegistryError> + Send + Sync>;

/// Resolve the `cwd` a client named to a real directory inside one of `roots`.
///
/// Canonicalization is what makes the containment check mean anything: it resolves every symlink,
/// so a link inside a root pointing at `/etc` is compared as `/etc` and refused, rather than
/// passing the prefix test on its own name and then being followed by the tool engine.
pub fn resolve_chat_workspace(cwd: &str, roots: &[PathBuf]) -> Result<PathBuf, ModelRegistryError> {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return Err(ModelRegistryError::InvalidWorkspace(
            "new_session named no cwd, and this assistant has tools to run in one".to_string(),
        ));
    }
    let named = Path::new(cwd);
    if !named.is_absolute() {
        return Err(ModelRegistryError::InvalidWorkspace(format!(
            "'{cwd}' is relative; a chat workspace is named absolutely, since this daemon's own \
             working directory is not the operator's"
        )));
    }
    let resolved = named.canonicalize().map_err(|e| {
        ModelRegistryError::InvalidWorkspace(format!(
            "'{cwd}' is not a directory on this host: {e}"
        ))
    })?;
    if !resolved.is_dir() {
        return Err(ModelRegistryError::InvalidWorkspace(format!(
            "'{cwd}' is a file, not a directory"
        )));
    }

    let reachable = roots.iter().any(|root| {
        root.canonicalize()
            .map(|root| resolved.starts_with(root))
            .unwrap_or(false)
    });
    if !reachable {
        return Err(ModelRegistryError::PermissionDenied(format!(
            "'{cwd}' is outside every directory this operator's sessions and projects live in"
        )));
    }
    Ok(resolved)
}
