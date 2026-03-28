//! Filesystem-safe deletion of inactive session directories under a resolved sessions tree.

use std::path::{Component, Path, PathBuf};

use tddy_core::read_session_metadata;
use tddy_rpc::Status;

use crate::session_reader::is_pid_alive;

/// Validates `session_id` for use as a single path segment under the sessions base.
pub fn validate_session_id_for_delete(session_id: &str) -> Result<(), Status> {
    let s = session_id.trim();
    if s.is_empty() {
        log::debug!("validate_session_id_for_delete: empty session_id");
        return Err(Status::invalid_argument("session_id is required"));
    }
    if s.len() > 512 {
        return Err(Status::invalid_argument("session_id is too long"));
    }
    if s.contains('/') || s.contains('\\') || s.contains('\0') {
        log::debug!("validate_session_id_for_delete: rejected path separators in session_id");
        return Err(Status::invalid_argument(
            "session_id must be a single path segment",
        ));
    }
    if s == "." || s == ".." {
        return Err(Status::invalid_argument("invalid session_id"));
    }
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            continue;
        }
        log::debug!(
            "validate_session_id_for_delete: invalid character in session_id (codepoint={:?})",
            ch
        );
        return Err(Status::invalid_argument(
            "session_id contains invalid characters",
        ));
    }
    Ok(())
}

/// Returns true when `candidate` is `base` plus exactly one additional normal path segment.
fn path_is_single_segment_under_base(base: &Path, candidate: &Path) -> bool {
    let base_components: Vec<Component<'_>> = base.components().collect();
    let cand_components: Vec<Component<'_>> = candidate.components().collect();
    if cand_components.len() != base_components.len() + 1 {
        return false;
    }
    for i in 0..base_components.len() {
        if base_components[i] != cand_components[i] {
            return false;
        }
    }
    matches!(cand_components.last(), Some(Component::Normal(_)))
}

/// Resolves `sessions_base.join(session_id)` after validating the id (directory may not exist yet).
pub fn resolve_session_directory_for_delete(
    sessions_base: &Path,
    session_id: &str,
) -> Result<PathBuf, Status> {
    validate_session_id_for_delete(session_id)?;
    let joined = sessions_base.join(session_id.trim());
    if !path_is_single_segment_under_base(sessions_base, &joined) {
        log::debug!(
            "resolve_session_directory_for_delete: path not a single segment under base (base={:?}, joined={:?})",
            sessions_base,
            joined
        );
        return Err(Status::invalid_argument(
            "session directory path must be directly under the sessions base",
        ));
    }
    log::debug!(
        "resolve_session_directory_for_delete: resolved {:?}",
        joined
    );
    Ok(joined)
}

/// Deletes an on-disk session directory only when the session is inactive (no live PID).
pub fn delete_inactive_session_directory(
    sessions_base: &Path,
    session_id: &str,
) -> Result<(), Status> {
    let session_id = session_id.trim();
    let session_dir = resolve_session_directory_for_delete(sessions_base, session_id)?;
    log::debug!(
        "delete_inactive_session_directory: session_dir={:?} session_id={} sessions_base={:?}",
        session_dir,
        session_id,
        sessions_base
    );

    if !session_dir.is_dir() {
        log::info!(
            "delete_inactive_session_directory: no session directory on this daemon (session_id={}); refusing as wrong routing / ownership",
            session_id
        );
        return Err(Status::failed_precondition(
            "session is not present on this daemon; use the daemon that owns it (check routing / host)",
        ));
    }

    let metadata = match read_session_metadata(&session_dir) {
        Ok(m) => m,
        Err(e) => {
            log::debug!(
                "delete_inactive_session_directory: failed to read session metadata: {}",
                e
            );
            return Err(Status::not_found("session not found"));
        }
    };

    let is_active = metadata.pid.map(is_pid_alive).unwrap_or(false);
    log::debug!(
        "delete_inactive_session_directory: pid={:?} is_active={}",
        metadata.pid,
        is_active
    );

    if is_active {
        log::info!(
            "delete_inactive_session_directory: refusing delete for active session {}",
            session_id
        );
        return Err(Status::failed_precondition(
            "session is still active; stop the process before deleting",
        ));
    }

    std::fs::remove_dir_all(&session_dir).map_err(|e| {
        log::error!(
            "delete_inactive_session_directory: remove_dir_all failed for session_id={}: {}",
            session_id,
            e
        );
        Status::internal("failed to remove session directory")
    })?;
    log::info!(
        "delete_inactive_session_directory: removed session directory for {}",
        session_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::SessionMetadata;

    fn write_dead_pid_session(dir: &Path, sid: &str, pid: u32) {
        let metadata = SessionMetadata {
            session_id: sid.to_string(),
            project_id: "proj-u".to_string(),
            created_at: "2026-03-21T10:00:00Z".to_string(),
            updated_at: "2026-03-21T10:00:00Z".to_string(),
            status: "exited".to_string(),
            repo_path: Some("/tmp".to_string()),
            pid: Some(pid),
            tool: None,
            livekit_room: None,
        };
        tddy_core::write_session_metadata(dir, &metadata).unwrap();
    }

    /// Lower-level: non-empty session ids should be accepted once validation exists.
    #[test]
    fn validate_accepts_typical_session_id() {
        let r = validate_session_id_for_delete("inactive-delete-me");
        assert!(
            r.is_ok(),
            "expected valid session id to pass validation (green phase)"
        );
    }

    /// Lower-level: resolution should yield a directory under the base for safe ids.
    #[test]
    fn resolve_returns_directory_under_sessions_base() {
        let base = Path::new("/tmp/tddy-sessions-test");
        let r = resolve_session_directory_for_delete(base, "abc-def-123");
        assert!(r.is_ok(), "expected resolved path under base");
        assert_eq!(r.unwrap(), base.join("abc-def-123"));
    }

    /// Lower-level: full delete succeeds for an inactive fixture directory.
    #[test]
    fn delete_inactive_removes_session_directory() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        let _ = child.wait();

        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("sessions");
        let sid = "unit-inactive-sid";
        let dir = base.join(sid);
        std::fs::create_dir_all(&dir).unwrap();
        write_dead_pid_session(&dir, sid, pid);

        let r = delete_inactive_session_directory(&base, sid);
        assert!(r.is_ok(), "expected delete to succeed for inactive session");
        assert!(!dir.exists(), "directory should be removed");
    }

    #[test]
    fn validate_rejects_dot_dot() {
        let e = validate_session_id_for_delete("..").unwrap_err();
        assert_eq!(e.code, tddy_rpc::Code::InvalidArgument);
    }

    #[test]
    fn validate_rejects_slash_in_session_id() {
        let e = validate_session_id_for_delete("evil/id").unwrap_err();
        assert_eq!(e.code, tddy_rpc::Code::InvalidArgument);
    }

    /// Delete for a session id not present on this daemon’s tree signals wrong-daemon / routing (failed_precondition).
    #[test]
    fn delete_missing_session_uses_failed_precondition_for_cross_daemon_routing() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("sessions_this_daemon");
        std::fs::create_dir_all(&base).unwrap();
        let err = delete_inactive_session_directory(&base, "session-owned-on-another-daemon")
            .unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    }
}
