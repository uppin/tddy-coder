use crate::launch_ports::LaunchSessions;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_work_queue::{
    starts_with_work_queue, SessionWorkQueueError, SessionWorkQueues,
};
use tddy_core::SESSION_METADATA_FILENAME;
use tddy_rpc::Response;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionResponse;

/// Give the session at `session_dir` its work queue when it starts with one — asked for
/// (`with_work_queue`) or an orchestrator-role session (`#workqueue` 5/14, the one role-aware rule) —
/// unless it already owns one. Returns whether a queue was created.
///
/// A session whose `.session.yaml` is not written yet — a tool session, whose `tddy-coder` writes it
/// after the start is answered — has no recorded role, so it starts with a queue only when one was
/// asked for; a queue asked for still needs the metadata's project and fails without it.
pub(crate) fn give_the_session_its_queue(
    session_dir: &std::path::Path,
    with_work_queue: bool,
) -> Result<bool, String> {
    if !with_work_queue && !session_dir.join(SESSION_METADATA_FILENAME).exists() {
        return Ok(false);
    }
    let metadata = tddy_core::read_session_metadata(session_dir).map_err(|e| e.to_string())?;
    if !starts_with_work_queue(with_work_queue, metadata.session_role.as_deref()) {
        return Ok(false);
    }
    let sessions_dir = session_dir
        .parent()
        .ok_or_else(|| format!("{} is not a session directory", session_dir.display()))?;
    let session_id = session_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{} is not a session directory", session_dir.display()))?;
    let created_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|e| format!("the system clock is before the Unix epoch: {e}"))?;
    match SessionWorkQueues::new(sessions_dir).create_for_session(
        &session_id,
        &metadata.project_id,
        created_at_unix_ms,
    ) {
        Ok(_) => Ok(true),
        Err(SessionWorkQueueError::AlreadyExists { .. }) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

impl LaunchSessions {
    /// Give the session `started` names its work queue, once the start has created it here: when it
    /// was started "with work queue", or is an orchestrator-role session. A forwarded start is
    /// handled by the peer that creates it; a queue that cannot be created fails the start's answer
    /// rather than leaving a session that was asked for a queue without one.
    pub(super) async fn create_the_started_sessions_queue(
        &self,
        os_user: &str,
        started: &Response<StartSessionResponse>,
        with_work_queue: bool,
    ) -> Result<(), Status> {
        let session_id = started.get_ref().session_id.clone();
        if session_id.is_empty() {
            return Ok(());
        }
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        let session_dir = unified_session_dir_path(&sessions_base, &session_id);
        tokio::task::spawn_blocking(move || {
            give_the_session_its_queue(&session_dir, with_work_queue)
        })
        .await
        .map_err(|e| Status::internal(format!("creating the work queue failed: {e}")))?
        .map(drop)
        .map_err(|e| {
            Status::internal(format!(
                "session {session_id} started, but its work queue could not be created: {e}"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::give_the_session_its_queue;

    #[test]
    fn a_session_with_no_metadata_yet_and_no_queue_asked_for_starts_without_one() {
        // Given — a tool session's directory before its tddy-coder writes `.session.yaml`
        let sessions = tempfile::tempdir().unwrap();
        let session_dir = sessions.path().join("01a2-tool-session");
        std::fs::create_dir_all(&session_dir).unwrap();

        // When
        let created = give_the_session_its_queue(&session_dir, false);

        // Then — no queue, no error, nothing written
        assert_eq!(created, Ok(false));
        assert!(!session_dir.join("work-queue.yaml").exists());
    }

    #[test]
    fn a_queue_asked_for_a_session_with_no_metadata_is_refused_rather_than_skipped() {
        // Given
        let sessions = tempfile::tempdir().unwrap();
        let session_dir = sessions.path().join("01a2-tool-session");
        std::fs::create_dir_all(&session_dir).unwrap();

        // When
        let created = give_the_session_its_queue(&session_dir, true);

        // Then
        assert!(created.is_err(), "{created:?}");
    }
}
