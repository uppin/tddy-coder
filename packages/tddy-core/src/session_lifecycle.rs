//! Single-session lifecycle bootstrap (PRD: one UUID v7 per process, one `sessions/<id>/` tree).

use std::path::{Path, PathBuf};

use crate::error::WorkflowError;
use crate::output::{create_session_dir_with_id, SESSIONS_SUBDIR};

/// Create or ensure the unified session directory: `{session_base}/sessions/{session_id}/`.
pub fn materialize_unified_session_directory(
    session_base: &Path,
    session_id: &str,
) -> Result<PathBuf, WorkflowError> {
    log::debug!(
        "materialize_unified_session_directory: session_base={:?} session_id={}",
        session_base,
        session_id
    );
    create_session_dir_with_id(session_base, session_id)
}

/// Effective engine / filesystem session id when the backend returns a distinct agent thread id.
/// The process-bound id (allocated at startup or supplied by the spawner) always wins.
#[must_use]
pub fn resolve_effective_session_id(
    process_session_id: Option<&str>,
    backend_session_id: Option<&str>,
) -> Option<String> {
    log::debug!(
        "resolve_effective_session_id: process={:?} backend={:?}",
        process_session_id,
        backend_session_id
    );
    if let Some(pid) = process_session_id.filter(|s| !s.trim().is_empty()) {
        log::info!(
            "session id: keeping process/session id {} (ignoring backend id if different)",
            pid
        );
        return Some(pid.to_string());
    }
    backend_session_id.map(str::to_string)
}

/// Canonical on-disk path for the unified contract: `{base}/sessions/{session_id}/` (no I/O).
#[must_use]
pub fn unified_session_dir_path(session_base: &Path, session_id: &str) -> PathBuf {
    session_base.join(SESSIONS_SUBDIR).join(session_id)
}

/// Reason a `session_id` cannot be used as a single path segment under `sessions/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdValidationError {
    Empty,
    TooLong,
    NotSingleSegment,
    InvalidDot,
    InvalidCharacter,
}

impl SessionIdValidationError {
    /// Message suitable for gRPC `INVALID_ARGUMENT` and CLI errors.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        match self {
            Self::Empty => "session_id is required",
            Self::TooLong => "session_id is too long",
            Self::NotSingleSegment => "session_id must be a single path segment",
            Self::InvalidDot => "invalid session_id",
            Self::InvalidCharacter => "session_id contains invalid characters",
        }
    }
}

/// Ensures `session_id` is safe to join under `{base}/sessions/` (no `..`, separators, or odd characters).
pub fn validate_session_id_segment(session_id: &str) -> Result<(), SessionIdValidationError> {
    let s = session_id.trim();
    if s.is_empty() {
        return Err(SessionIdValidationError::Empty);
    }
    if s.len() > 512 {
        return Err(SessionIdValidationError::TooLong);
    }
    if s.contains('/') || s.contains('\\') || s.contains('\0') {
        return Err(SessionIdValidationError::NotSingleSegment);
    }
    if s == "." || s == ".." {
        return Err(SessionIdValidationError::InvalidDot);
    }
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            continue;
        }
        return Err(SessionIdValidationError::InvalidCharacter);
    }
    Ok(())
}

/// Trait for injecting a single bootstrap path (hooks, presenter, spawners).
pub trait SessionLifecycleBootstrap: Send + Sync {
    fn ensure_session_directory(
        &self,
        session_base: &Path,
        session_id: &str,
    ) -> Result<PathBuf, WorkflowError>;
}

/// Production default: unified `sessions/<id>/` tree via [`materialize_unified_session_directory`].
pub struct UnifiedSessionTreeBootstrap;

impl SessionLifecycleBootstrap for UnifiedSessionTreeBootstrap {
    fn ensure_session_directory(
        &self,
        session_base: &Path,
        session_id: &str,
    ) -> Result<PathBuf, WorkflowError> {
        log::debug!(
            "UnifiedSessionTreeBootstrap::ensure_session_directory base={:?} id={}",
            session_base,
            session_id
        );
        materialize_unified_session_directory(session_base, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::create_session_dir_with_id;
    use std::fs;

    const PROC_ID: &str = "019d357e-48ee-7c11-bd44-a967873f58b2";
    const AGENT_ID: &str = "00000000-0000-7000-8000-000000000099";

    #[test]
    fn materialize_unified_session_directory_matches_cli_sessions_subtree() {
        let base =
            std::env::temp_dir().join(format!("tddy-slc-materialize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let got = materialize_unified_session_directory(&base, PROC_ID).expect("materialize");
        let expected = create_session_dir_with_id(&base, PROC_ID).expect("with_id");

        assert_eq!(
            got, expected,
            "unified materialization must match create_session_dir_with_id (sessions/{{id}})"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_effective_session_id_keeps_process_id_when_backend_differs() {
        let got = resolve_effective_session_id(Some(PROC_ID), Some(AGENT_ID));
        assert_eq!(
            got.as_deref(),
            Some(PROC_ID),
            "process session_id must not be replaced by backend agent session_id"
        );
    }

    #[test]
    fn unified_tree_bootstrap_matches_unified_path() {
        let base =
            std::env::temp_dir().join(format!("tddy-slc-unified-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let bootstrap = UnifiedSessionTreeBootstrap;
        let got = bootstrap
            .ensure_session_directory(&base, PROC_ID)
            .expect("ensure");
        let expected = unified_session_dir_path(&base, PROC_ID);
        assert_eq!(
            got, expected,
            "bootstrap must resolve to unified_session_dir_path"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn validate_session_id_segment_rejects_traversal_and_separators() {
        assert_eq!(
            validate_session_id_segment("..").unwrap_err(),
            SessionIdValidationError::InvalidDot
        );
        assert_eq!(
            validate_session_id_segment("a/b").unwrap_err(),
            SessionIdValidationError::NotSingleSegment
        );
        assert_eq!(
            validate_session_id_segment("").unwrap_err(),
            SessionIdValidationError::Empty
        );
        assert!(validate_session_id_segment("abc-def_012").is_ok());
    }
}
