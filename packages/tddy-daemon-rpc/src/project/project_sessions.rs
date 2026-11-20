//! A project's sessions on this host, as deleting the project needs them: the ordinary ones still
//! running, which refuse the delete, and the project-home ones, which the delete stops. Nothing
//! here removes a session's data.

use std::path::Path;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::live_sessions::LiveSessions;
use tddy_rpc::Status;
use tddy_service::proto::session::Signal;
use tddy_session_activity::session_reader::{list_sessions_in_dir, SessionEntry};
use tddy_session_lifecycle::terminal_session_adapter::DaemonTerminalSessionStore;

/// This host's sessions of `project_id` that hold a live agent.
fn running_sessions(
    sessions_base: &Path,
    project_id: &str,
    live: &LiveSessions,
) -> Result<Vec<SessionEntry>, Status> {
    Ok(list_sessions_in_dir(sessions_base, live)
        .map_err(|e| Status::internal(format!("list sessions: {e}")))?
        .into_iter()
        .filter(|s| s.project_id == project_id && s.is_active)
        .collect())
}

/// The project's running sessions that block a delete — every one but its project-home sessions —
/// each as `<id> (<name>) on <host>`, or just `<id> on <host>` when the session has no name.
pub(super) fn sessions_blocking_a_delete(
    sessions_base: &Path,
    project_id: &str,
    host: &str,
    live: &LiveSessions,
) -> Result<Vec<String>, Status> {
    Ok(running_sessions(sessions_base, project_id, live)?
        .into_iter()
        .filter(|s| s.session_role != PROJECT_HOME_ROLE)
        .map(|s| {
            let dir = unified_session_dir_path(sessions_base, &s.session_id);
            match tddy_core::read_changeset(&dir).ok().and_then(|c| c.name) {
                Some(name) => format!("{} ({name}) on {host}", s.session_id),
                None => format!("{} on {host}", s.session_id),
            }
        })
        .collect())
}

/// Stop the project's running project-home sessions with SIGTERM — the path the project-home
/// `TerminateSession` tool takes — keeping each session's record, worktree and branch. Returns the
/// ids it signalled; one whose process ended meanwhile is skipped.
pub(super) async fn stop_project_home_sessions(
    terminals: &DaemonTerminalSessionStore,
    sessions_base: &Path,
    project_id: &str,
) -> Result<Vec<String>, Status> {
    let live = terminals.live_sessions().await;
    let mut stopped = Vec::new();
    for session in running_sessions(sessions_base, project_id, &live)? {
        if session.session_role != PROJECT_HOME_ROLE {
            continue;
        }
        let dir = unified_session_dir_path(sessions_base, &session.session_id);
        match terminals
            .signal_session_dir(&dir, &session.session_id, Signal::Sigterm as i32)
            .await
        {
            Ok(_) => stopped.push(session.session_id),
            Err(status) => log::info!(
                "stop_project_home_sessions: session {} not signalled: {}",
                session.session_id,
                status.message
            ),
        }
    }
    Ok(stopped)
}
