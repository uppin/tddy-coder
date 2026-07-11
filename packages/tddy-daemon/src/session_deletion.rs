//! Filesystem-safe deletion of session directories under a resolved sessions tree.
//!
//! On Unix, if `.session.yaml` records a live PID, the process is sent **SIGTERM**, waited on,
//! then **SIGKILL** if needed, before removing the directory.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::Status;

use crate::project_storage;
use crate::session_reader::is_pid_alive;
use crate::worktrees;

/// Pure: does `worktree` sit under the daemon's managed worktree layout?
///
/// Every worktree the daemon creates for a claude-cli/cursor-cli session lives under a
/// `.worktrees` directory inside the project's main repo (see `tddy_core::worktree::worktree_dir`).
/// A session started against a client-supplied `repo_path` (an arbitrary local checkout) does not,
/// so this returns `false` for it — the signal used to keep the user's checkout from being removed
/// on session deletion.
pub fn is_daemon_managed_worktree(worktree: &Path) -> bool {
    worktree
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(".worktrees"))
}

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
pub(crate) fn signal_pid(pid: i32, sig: libc::c_int) -> Result<(), Status> {
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
    // SIGKILL cannot be blocked or ignored; the process is dead. On some platforms (macOS) the
    // zombie remains visible to kill(pid,0) until the parent calls waitpid, so we do a brief
    // best-effort wait but do not return an error — the process has no running threads.
    let _ = wait_until_pid_stopped(pid, Duration::from_secs(3), Duration::from_millis(100));
    Ok(())
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
///
/// `projects_dir` is optional but recommended for claude-cli sessions: when provided, the
/// linked git worktree is removed via `git worktree remove` (git-aware). When absent, the
/// directory is removed with `std::fs::remove_dir_all` (leaving a dangling git worktree registration).
pub fn delete_session_directory(
    sessions_base: &Path,
    session_id: &str,
    projects_dir: Option<&Path>,
) -> Result<(), Status> {
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

    // Extract the claude-cli worktree path before the cfg blocks consume or shadow `metadata`.
    let claude_cli_worktree = metadata
        .as_ref()
        .filter(|m| m.session_type.as_deref() == Some("claude-cli"))
        .and_then(|m| m.repo_path.clone());

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

    // For claude-cli sessions, remove the linked git worktree.
    if let Some(ref worktree_str) = claude_cli_worktree {
        let worktree = PathBuf::from(worktree_str);
        // Attempt git-aware removal when we have a projects_dir and a project_id.
        let removed_git_aware = if let (Some(pd), Some(ref project_id)) = (
            projects_dir,
            metadata.as_ref().map(|m| m.project_id.as_str()),
        ) {
            match project_storage::find_project(pd, project_id) {
                Ok(Some(ref project)) => {
                    let repo_root = PathBuf::from(&project.main_repo_path);
                    match worktrees::remove_worktree_under_repo(&repo_root, &worktree) {
                        Ok(()) => {
                            log::info!(
                                "delete_session_directory: git worktree remove {:?} for {}",
                                worktree,
                                session_id
                            );
                            true
                        }
                        Err(e) => {
                            log::warn!(
                                "delete_session_directory: git worktree remove failed for {:?} ({:?}); falling back to remove_dir_all",
                                worktree,
                                e
                            );
                            false
                        }
                    }
                }
                Ok(None) => {
                    log::warn!(
                        "delete_session_directory: project {} not found; falling back to remove_dir_all for {:?}",
                        project_id,
                        worktree
                    );
                    false
                }
                Err(e) => {
                    log::warn!(
                        "delete_session_directory: find_project error ({}); falling back to remove_dir_all for {:?}",
                        e,
                        worktree
                    );
                    false
                }
            }
        } else {
            false
        };

        if !removed_git_aware {
            // The git-aware removal was skipped or failed. Only fall back to `remove_dir_all` for a
            // daemon-managed worktree (created under `<repo>/.worktrees/`). A session started
            // against a client-supplied `repo_path` (an arbitrary local checkout) records that path
            // here verbatim; wiping it would destroy the user's working tree, so it is left intact.
            if is_daemon_managed_worktree(&worktree) {
                let _ = std::fs::remove_dir_all(&worktree);
                log::info!(
                    "delete_session_directory: removed claude-cli worktree {:?} for {} (remove_dir_all fallback)",
                    worktree,
                    session_id
                );
            } else {
                log::info!(
                    "delete_session_directory: leaving worktree {:?} for {} intact (not a daemon-managed worktree — e.g. a client-supplied repo_path checkout)",
                    worktree,
                    session_id
                );
            }
        }
    }

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
            previous_session_id: None,
            session_type: None,
            model: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(dir, &metadata).unwrap();
    }

    /// Lower-level: non-empty session ids should be accepted once validation exists.
    #[test]
    fn validate_accepts_typical_session_id() {
        let r = validate_session_id_for_delete("inactive-delete-me");
        assert!(r.is_ok(), "expected valid session id to pass validation");
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

        let r = delete_session_directory(&base, sid, None);
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

        let r = delete_session_directory(&base, sid, None);
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
        let err =
            delete_session_directory(&base, "session-owned-on-another-daemon", None).unwrap_err();
        assert_eq!(err.code, tddy_rpc::Code::FailedPrecondition);
    }

    #[test]
    fn daemon_managed_worktree_recognised_by_dot_worktrees_layout() {
        // Given — a worktree created under a project's `.worktrees` dir
        let worktree = Path::new("/home/dev/my-repo/.worktrees/claude-cli-abc123");

        // When / Then
        assert!(is_daemon_managed_worktree(worktree));
    }

    #[test]
    fn client_supplied_checkout_is_not_treated_as_daemon_managed() {
        // Given — an arbitrary local checkout passed via `repo_path`
        let checkout = Path::new("/home/dev/some-project");

        // When / Then
        assert!(!is_daemon_managed_worktree(checkout));
    }

    /// A `repo_path` session records the user's checkout as its worktree. Deleting the session must
    /// terminate/clean the session dir but never remove that external checkout.
    #[test]
    fn delete_preserves_a_client_supplied_repo_path_checkout() {
        // Given — an external checkout with a file the user would lose if it were wiped
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("external-checkout");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::write(checkout.join("keep-me.txt"), b"important").unwrap();

        // And — a claude-cli session whose worktree IS that checkout
        let base = temp.path().join("tddy-home");
        let sid = "unit-repo-path-sid";
        let dir = unified_session_dir_path(&base, sid);
        std::fs::create_dir_all(&dir).unwrap();
        let metadata = SessionMetadata {
            session_id: sid.to_string(),
            project_id: String::new(),
            created_at: "2026-07-11T10:00:00Z".to_string(),
            updated_at: "2026-07-11T10:00:00Z".to_string(),
            status: "exited".to_string(),
            repo_path: Some(checkout.to_string_lossy().to_string()),
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("claude-cli".to_string()),
            model: Some("claude-opus-4-8".to_string()),
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            specialized_agents: Vec::new(),
        };
        tddy_core::write_session_metadata(&dir, &metadata).unwrap();

        // When — the session is deleted with no projects dir (external checkout, no project)
        let r = delete_session_directory(&base, sid, None);

        // Then — the session dir is gone but the user's checkout is left intact
        assert!(r.is_ok(), "expected delete to succeed");
        assert!(!dir.exists(), "session directory should be removed");
        assert!(checkout.is_dir(), "external checkout must not be removed");
        assert!(
            checkout.join("keep-me.txt").exists(),
            "files in the external checkout must be preserved"
        );
    }
}
