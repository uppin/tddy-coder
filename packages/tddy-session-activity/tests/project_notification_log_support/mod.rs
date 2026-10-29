//! The world the project notification log's suites share: a data directory holding each OS user's
//! sessions, and the log opened over it and registered on a bus (`#home-inbox` 7/8).
#![allow(dead_code)] // each suite uses its own part of the world

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_notifications::SessionNotificationBus;

/// Where `os_user`'s sessions live under `data_dir`.
pub fn sessions_of(data_dir: &Path, os_user: &str) -> PathBuf {
    data_dir.join("users").join(os_user)
}

/// The log over `data_dir`, as a daemon starting over it opens it, and a bus it is registered on.
pub fn a_log_over(data_dir: &Path) -> (Arc<ProjectNotificationLog>, SessionNotificationBus) {
    let root = data_dir.to_path_buf();
    let log = Arc::new(
        ProjectNotificationLog::open(
            data_dir,
            Arc::new(move |os_user: &str| Some(sessions_of(&root, os_user))),
        )
        .expect("the log opens over the data directory"),
    );
    let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&log));
    (log, bus)
}

/// The record of `os_user`'s session `session_id` in `project_id` (empty: a session of no project).
pub fn write_session(data_dir: &Path, os_user: &str, session_id: &str, project_id: &str) {
    let session_dir = sessions_of(data_dir, os_user)
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        created_at: "2026-10-25T10:00:00Z".to_string(),
        updated_at: "2026-10-25T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{os_user}/{project_id}")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
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
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
    };
    write_session_metadata(&session_dir, &meta).expect("the session's .session.yaml");
}
