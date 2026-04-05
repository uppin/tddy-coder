//! Filesystem-safe deletion of session directories under a resolved sessions tree.
//!
//! On Unix, if `.session.yaml` records a live PID, the process is sent **SIGTERM**, waited on,
//! then **SIGKILL** if needed, before removing the directory.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::Status;

use crate::session_reader::is_pid_alive;

/// After SIGKILL the child may be a zombie until its parent reaps it; `kill(pid, 0)` still succeeds.
#[cfg(all(unix, target_os = "linux"))]
fn pid_is_zombie(pid: u32) -> bool {
    let path = format!("/proc/{pid}/stat");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return true;
    };
    let Some(rparen) = contents.find(')') else {
        return false;
    };
    let rest = contents[rparen + 1..].trim_start();
    rest.starts_with('Z')
}

#[cfg(all(unix, not(target_os = "linux")))]
fn pid_is_zombie(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn pid_stopped_or_zombie(pid: u32) -> bool {
    !is_pid_alive(pid) || pid_is_zombie(pid)
}

/// Validates `session_id` for use as a single path segment under the sessions base.
#[inline]
pub fn validate_session_id_for_delete(session_id: &str) -> Result<(), Status> {
    validate_session_id_segment(session_id).map_err(|e| {
        log::debug!("validate_session_id_for_delete: {:?}", e);
        Status::invalid_argument(e.message())
    })
}

/// Resolves `{sessions_base}/sessions/{session_id}/` after validating the id (directory may not exist yet).
pub fn resolve_session_directory_for_delete(
    sessions_base: &Path,
    session_id: &str,
) -> Result<PathBuf, Status> {
    validate_session_id_for_delete(session_id)?;
    let joined = unified_session_dir_path(sessions_base, session_id.trim());
    log::debug!(
        "resolve_session_directory_for_delete: resolved {:?}",
        joined
    );
    Ok(joined)
}

#[cfg(unix)]
fn signal_pid(pid: i32, sig: libc::c_int) -> Result<(), Status> {
    let ret = unsafe { libc::kill(pid, sig) };
    if ret == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(Status::internal(format!("kill(pid, signal {sig}): {err}")))
}

/// SIGTERM, wait, then SIGKILL if the PID from session metadata is still alive.
#[cfg(unix)]
fn terminate_session_process(pid: u32) -> Result<(), Status> {
    if pid_stopped_or_zombie(pid) {
        return Ok(());
    }
    let pid_i = pid as i32;
    signal_pid(pid_i, libc::SIGTERM)?;
    if wait_until_pid_stopped(pid, Duration::from_secs(5), Duration::from_millis(100)) {
        return Ok(());
    }
    signal_pid(pid_i, libc::SIGKILL)?;
    if wait_until_pid_stopped(pid, Duration::from_secs(3), Duration::from_millis(100)) {
        return Ok(());
    }
    Err(Status::failed_precondition(
        "session process did not exit after SIGTERM and SIGKILL",
    ))
}

#[cfg(unix)]
fn wait_until_pid_stopped(pid: u32, total: Duration, step: Duration) -> bool {
    let mut waited = Duration::ZERO;
    loop {
        if pid_stopped_or_zombie(pid) {
            return true;
        }
        if waited >= total {
            return pid_stopped_or_zombie(pid);
        }
        std::thread::sleep(step);
        waited += step;
    }
}

/// Deletes a session directory. On Unix, terminates a live recorded PID first.
pub fn delete_session_directory(sessions_base: &Path, session_id: &str) -> Result<(), Status> {
    let session_id = session_id.trim();
    let session_dir = resolve_session_directory_for_delete(sessions_base, session_id)?;
    log::debug!(
        "delete_session_directory: session_dir={:?} session_id={} sessions_base={:?}",
        session_dir,
        session_id,
        sessions_base
    );

    if !session_dir.is_dir() {
        log::info!(
            "delete_session_directory: no session directory on this daemon (session_id={}); refusing as wrong routing / ownership",
            session_id
        );
        return Err(Status::failed_precondition(
            "session is not present on this daemon; use the daemon that owns it (check routing / host)",
        ));
    }

    let metadata = match read_session_metadata(&session_dir) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!(
                "delete_session_directory: no readable .session.yaml in {:?}: {} — removing directory without PID termination",
                session_dir,
                e
            );
            None
        }
    };

    #[cfg(unix)]
    {
        if let Some(ref m) = metadata {
            if let Some(pid) = m.pid {
                log::debug!(
                    "delete_session_directory: pid={} is_active={}",
                    pid,
                    is_pid_alive(pid)
                );
                terminate_session_process(pid)?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = metadata;

    std::fs::remove_dir_all(&session_dir).map_err(|e| {
        log::error!(
            "delete_session_directory: remove_dir_all failed for session_id={}: {}",
            session_id,
            e
        );
        Status::internal("failed to remove session directory")
    })?;
    log::info!(
        "delete_session_directory: removed session directory for {}",
        session_id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::session_lifecycle::unified_session_dir_path;
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
            pending_elicitation: false,
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
        assert_eq!(r.unwrap(), unified_session_dir_path(base, "abc-def-123"));
    }

    /// Lower-level: delete succeeds when `.session.yaml` is missing (orphan dir).
    #[test]
    fn delete_removes_directory_without_session_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("tddy-home");
        let sid = "unit-no-yaml-sid";
        let dir = unified_session_dir_path(&base, sid);
        std::fs::create_dir_all(dir.join("logs")).unwrap();

        let r = delete_session_directory(&base, sid);
        assert!(r.is_ok(), "expected delete to succeed without metadata");
        assert!(!dir.exists(), "directory should be removed");
    }

    /// Lower-level: full delete succeeds for an inactive fixture directory.
    #[test]
    fn delete_inactive_removes_session_directory() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        let _ = child.wait();

        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("tddy-home");
        let sid = "unit-inactive-sid";
        let dir = unified_session_dir_path(&base, sid);
        std::fs::create_dir_all(&dir).unwrap();
        write_dead_pid_session(&dir, sid, pid);

        let r = delete_session_directory(&base, sid);
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
        let err = delete_session_directory(&base, "session-owned-on-another-daemon").unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    }
}
