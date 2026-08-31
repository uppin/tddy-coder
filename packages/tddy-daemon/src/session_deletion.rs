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

/// Whether a session of this type owns the worktree its `.session.yaml` records, and therefore
/// whether `DeleteSession` should remove it.
///
/// `claude-cli` and `workspace` sessions each get a worktree cut for them alone — a workspace
/// session *is* its worktree and nothing else, so leaving it behind leaks the only thing the session
/// was for (see `docs/ft/daemon/remote-managed-worktree.md` § Teardown). A `tool` (tddy-coder)
/// session records the project's shared checkout instead, and an empty type is a legacy file
/// predating `session_type`; removing either would delete a tree other sessions depend on.
///
/// `cursor-cli` leaks its worktree the same way `workspace` did. Including it here would change
/// behaviour for sessions split placement never touches, so it is tracked in `docs/dev/TODO.md`
/// rather than fixed in passing.
pub fn worktree_removal_applies_to(session_type: &str) -> bool {
    matches!(session_type, "claude-cli" | "workspace")
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

/// Kill the workspace jail runner recorded under `<session_dir>/sandbox/runner.pid`.
///
/// Best-effort: a missing file, an invalid pid, or a process already gone must not fail the delete.
#[cfg(unix)]
fn teardown_workspace_sandbox(session_dir: &Path, metadata: &tddy_core::SessionMetadata) {
    if metadata.sandbox != Some(true) {
        return;
    }
    let pid_path = session_dir
        .join("sandbox")
        .join(crate::workspace_tool_sandbox::RUNNER_PID_FILE);
    let Ok(contents) = std::fs::read_to_string(&pid_path) else {
        return;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        log::warn!(
            "teardown_workspace_sandbox: invalid pid in {:?}: {:?}",
            pid_path,
            contents
        );
        return;
    };
    log::debug!(
        "teardown_workspace_sandbox: terminating jail runner pid={} from {:?}",
        pid,
        pid_path
    );
    if let Err(e) = terminate_session_process(pid) {
        log::warn!(
            "teardown_workspace_sandbox: failed to terminate jail runner pid={}: {}",
            pid,
            e
        );
    }
    // The jail runner this daemon recorded was spawned by a *previous* daemon process; after a
    // restart it is not our child, so `waitpid` returns `ECHILD` and this is a no-op. In a
    // single-process test the host that spawned the runner is also the one running the delete, so
    // the dead child is left as a zombie that `kill(pid, 0)` still reports as alive — reap it so
    // the post-delete liveness check sees `ESRCH` rather than a zombie.
    reap_child_if_ours(pid as i32);
}

#[cfg(not(unix))]
fn teardown_workspace_sandbox(_session_dir: &Path, _metadata: &tddy_core::SessionMetadata) {}

/// Reap `pid` when this process is its parent. A reparented orphan returns immediately with `ECHILD`.
#[cfg(unix)]
fn reap_child_if_ours(pid: i32) {
    let mut status: i32 = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if ret == pid {
            return;
        }
        if ret == 0 {
            if std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ECHILD) {
            return;
        }
        log::warn!("reap_child_if_ours: waitpid({pid}): {err}");
        return;
    }
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

/// Stop hosting the LiveKit room a session opened for its worktree, if it opened one.
///
/// Call this **before** [`delete_session_directory`]: it stops the room's poll loop from starting
/// another measurement, so the checkout is not being polled every couple of seconds while it is
/// removed. It does not promise the directory is untouched the instant this returns — a `git` the
/// loop had already started keeps running until it exits or its own budget kills it
/// ([`crate::session_room`]). What it does guarantee is that the loop stops, rather than warning
/// about a missing directory at the poll rate for the life of the daemon.
///
/// A session that never hosted a room (any type but `workspace`, or a daemon with no LiveKit
/// credentials) has nothing registered under its id and this does nothing.
pub fn close_session_room(rooms: &crate::session_room::SessionRoomRegistry, session_id: &str) {
    rooms.close(session_id.trim());
}

/// Deletes a session directory. On Unix, terminates a live recorded PID first.
///
/// `projects_dir` is optional but recommended for sessions that own a worktree
/// ([`worktree_removal_applies_to`]): when provided, the linked git worktree is removed via
/// `git worktree remove` (git-aware). When absent, the directory is removed with
/// `std::fs::remove_dir_all` (leaving a dangling git worktree registration).
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

    // Extract the session-owned worktree path before the cfg blocks consume or shadow `metadata`.
    let session_worktree = metadata
        .as_ref()
        .filter(|m| worktree_removal_applies_to(m.session_type.as_deref().unwrap_or_default()))
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

    // For session types that own their worktree, remove the linked git worktree.
    if let Some(ref worktree_str) = session_worktree {
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
                    "delete_session_directory: removed session worktree {:?} for {} (remove_dir_all fallback)",
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

    if let Some(ref m) = metadata {
        teardown_workspace_sandbox(&session_dir, m);
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

    // A checkout that lived *inside* the session directory has just gone with it, but its
    // registration is in the project's repository and would survive as a stale `git worktree list`
    // row naming a directory that no longer exists — and, worse, holding the name so a later
    // checkout cannot reuse it.
    //
    // An agent clone's checkout is exactly that shape (docs/ft/daemon/session-agent-roster.md
    // § Clones). It is a one-way mirror, so it always carries the uncommitted work it exists to
    // hold, and `git worktree remove` refuses any checkout with modified or untracked files — the
    // git-aware removal above declines every time. `prune` is the operation for a checkout that is
    // already gone, and it does nothing for every other session, whose worktree lives outside the
    // session directory and is either removed above or deliberately left intact.
    if let Some(worktree) = session_worktree
        .as_deref()
        .map(PathBuf::from)
        .filter(|w| w.starts_with(&session_dir) && !w.exists())
    {
        prune_worktree_registration(projects_dir, metadata.as_ref(), &worktree, session_id);
    }
    Ok(())
}

/// Drop the project repository's registration of a checkout that no longer exists.
///
/// Best-effort and logged rather than returned: the session is already gone by the time this runs,
/// and a stale registration is a tidiness problem an operator can clear with `git worktree prune`,
/// not a reason to report a deletion that succeeded as a failure.
fn prune_worktree_registration(
    projects_dir: Option<&Path>,
    metadata: Option<&tddy_core::SessionMetadata>,
    worktree: &Path,
    session_id: &str,
) {
    let (Some(projects_dir), Some(project_id)) =
        (projects_dir, metadata.map(|m| m.project_id.as_str()))
    else {
        return;
    };
    let Ok(Some(project)) = project_storage::find_project(projects_dir, project_id) else {
        return;
    };
    match std::process::Command::new("git")
        .current_dir(&project.main_repo_path)
        .args(["worktree", "prune"])
        .output()
    {
        Ok(out) if out.status.success() => log::info!(
            "delete_session_directory: pruned the registration of {:?} for {}",
            worktree,
            session_id
        ),
        Ok(out) => log::warn!(
            "delete_session_directory: `git worktree prune` in {} left {:?} registered for {}: {}",
            project.main_repo_path,
            worktree,
            session_id,
            String::from_utf8_lossy(&out.stderr).trim_end()
        ),
        Err(e) => log::warn!(
            "delete_session_directory: could not run `git worktree prune` in {} for {}: {e}",
            project.main_repo_path,
            session_id
        ),
    }
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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: None,
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
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
    #[cfg(unix)]
    fn terminate_session_process_kills_a_running_child() {
        use std::process::Command;

        let mut child = Command::new("sleep").arg("120").spawn().unwrap();
        let pid = child.id();
        terminate_session_process(pid).expect("terminate should succeed");
        // `terminate_session_process` signals the child but does not reap it; the caller owns the
        // `Child` handle and reaps. A killed-but-unreaped child is a zombie that `kill(pid, 0)`
        // still reports as alive, so reap through the handle before asserting liveness.
        child.wait().expect("child should have been signalled");
        let ret = unsafe { libc::kill(pid as i32, 0) };
        assert_ne!(ret, 0, "child should no longer respond to kill(pid, 0)");
    }

    #[test]
    #[cfg(unix)]
    fn delete_kills_workspace_jail_runner_recorded_in_runner_pid() {
        use std::process::Command;

        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let sid = "sandbox-runner-sid";
        let dir = unified_session_dir_path(base, sid);
        let sandbox_dir = dir.join("sandbox");
        std::fs::create_dir_all(&sandbox_dir).unwrap();

        let child = Command::new("sleep").arg("120").spawn().unwrap();
        let pid = child.id();
        std::fs::write(
            sandbox_dir.join(crate::workspace_tool_sandbox::RUNNER_PID_FILE),
            pid.to_string(),
        )
        .unwrap();

        let metadata = SessionMetadata {
            session_id: sid.to_string(),
            project_id: "proj-u".to_string(),
            created_at: "2026-03-21T10:00:00Z".to_string(),
            updated_at: "2026-03-21T10:00:00Z".to_string(),
            status: "active".to_string(),
            repo_path: None,
            pid: None,
            tool: None,
            livekit_room: None,
            pending_elicitation: false,
            previous_session_id: None,
            session_type: Some("workspace".to_string()),
            model: None,
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
        };
        tddy_core::write_session_metadata(&dir, &metadata).unwrap();

        let r = delete_session_directory(base, sid, None);
        assert!(r.is_ok(), "delete should succeed: {r:?}");
        assert!(!dir.exists(), "session directory should be removed");

        let ret = unsafe { libc::kill(pid as i32, 0) };
        assert_ne!(ret, 0, "jail runner should be terminated");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        std::mem::forget(child);
    }

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
            cursor_chat_id: None,
            activity_status: None,
            hook_token: None,
            sandbox: Some(true),
            agent: None,
            recipe: None,
            agents: Vec::new(),
            agents_rev: 0,
            legacy_specialized_agents: Vec::new(),
            codebase_daemon_instance_id: None,
            codebase_session_id: None,
            agent_daemon_instance_id: None,
            agent_session_id: None,
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
