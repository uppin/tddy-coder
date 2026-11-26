//! A session's agent ending reaches its project's notification log, named and owned like every
//! other notification (`#activity-wake` 1/3).
//!
//! Real all the way down: a session manager running a stub agent as a session's main, the
//! forwarder the daemon starts, a bus with the project notification log registered on it, and a
//! data directory holding the session's `.session.yaml`.
//!
//! Feature: docs/ft/daemon/session-notifications.md

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::session_notification::SessionEndNotice;
use tddy_daemon_kernel::SessionsBaseResolver;
use tddy_session_activity::project_notification_log::{
    ProjectNotificationLog, ProjectNotificationRecord, ProjectNotificationScope,
};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::session_notifications::{
    forward_agent_ends, RecordingSessionNotificationSubscriber, SessionNotificationBus,
    SessionNotificationKind, SessionNotificationSource,
};
use tddy_testing_commons::wait::eventually;

const PROJECT: &str = "project-a";
/// The OS user the daemon runs as — whom an agent spawned with no OS user of its own belongs to.
const DAEMON_USER: &str = "ada";
const HOME: &str = "01900000-0000-7000-8000-AABB00000400";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000401";

/// A safety net for the stub to exit and its end to be published, not a prediction.
const OBSERVE: Duration = Duration::from_secs(10);
/// How long the forwarder is given to publish an end it was handed, before "nothing" is believed.
const HELD: Duration = Duration::from_millis(300);

/// A daemon's data directory with the child session of `PROJECT`, named `fix-login`.
struct AHost {
    data_dir: tempfile::TempDir,
}

impl AHost {
    fn holding_a_named_child_session() -> Self {
        let host = Self {
            data_dir: tempfile::tempdir().unwrap(),
        };
        a_session_record(&host.sessions_of(DAEMON_USER), CHILD, "fix-login");
        host
    }

    fn sessions_of(&self, os_user: &str) -> PathBuf {
        self.data_dir.path().join("users").join(os_user)
    }

    /// Every OS user resolves to its directory under the data dir.
    fn resolver(&self) -> SessionsBaseResolver {
        let root = self.data_dir.path().join("users");
        Arc::new(move |os_user: &str| Some(root.join(os_user)))
    }

    fn the_log(&self) -> Arc<ProjectNotificationLog> {
        Arc::new(
            ProjectNotificationLog::open(self.data_dir.path(), self.resolver())
                .expect("the log opens"),
        )
    }
}

fn a_session_record(sessions_base: &Path, session_id: &str, name: &str) {
    let session_dir = sessions_base.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        owner_subject: None,
        display_name: Some(name.to_string()),
        effort: None,
        session_id: session_id.to_string(),
        project_id: PROJECT.to_string(),
        created_at: "2026-11-16T10:00:00Z".to_string(),
        updated_at: "2026-11-16T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: None,
        pid: None,
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
        target: None,
        managed_codebase: false,
        session_role: None,
        tmux_session: None,
        orchestration_mark: None,
        agent_conversations: Vec::new(),
    };
    write_session_metadata(&session_dir, &meta).expect("the session's .session.yaml");
}

/// A stub agent that exits with `code`, run as `session_id`'s main.
async fn a_main_exiting_with(
    manager: &CliSessionManager,
    session_id: &str,
    code: i32,
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stub_agent.sh");
    std::fs::write(&path, format!("#!/bin/sh\nexit {code}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    manager
        .spawn_rendered_main(
            session_id,
            "claude-cli",
            dir.path().to_path_buf(),
            "",
            vec![path.to_string_lossy().into_owned()],
            Vec::new(),
            None,
            None,
        )
        .await
        .expect("the stub agent spawned");
    dir
}

/// What an orchestrator of `PROJECT` sees: every session of the project but its own.
fn the_orchestrators_scope() -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT.to_string(),
        os_user: DAEMON_USER.to_string(),
        exclude_session_id: HOME.to_string(),
        started_by: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ended_agent_lands_in_its_projects_notification_log() {
    // Given a host whose session manager's ends are forwarded onto a bus carrying the project log
    let host = AHost::holding_a_named_child_session();
    let log = host.the_log();
    let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&log)));
    let manager = CliSessionManager::new();
    let _forwarder = forward_agent_ends(
        manager.agent_ends(),
        bus,
        None,
        host.resolver(),
        DAEMON_USER.to_string(),
    );

    // When the child session's agent exits cleanly
    let _agent = a_main_exiting_with(&manager, CHILD, 0).await;

    // Then the project's log holds one record of it: attention, session ended, how it ended
    let records: Vec<ProjectNotificationRecord> =
        eventually("the end to be logged", OBSERVE, || {
            let read = log.read(&the_orchestrators_scope(), "", None)?;
            (!read.page.items.is_empty())
                .then_some(read.page.items)
                .ok_or_else(|| "the log holds nothing yet".to_string())
        })
        .await;
    assert_eq!(
        records
            .iter()
            .map(|r| (
                r.session_id.as_str(),
                r.kind,
                r.source,
                r.text.as_str(),
                r.ended.clone()
            ))
            .collect::<Vec<_>>(),
        vec![(
            CHILD,
            SessionNotificationKind::AttentionRequired,
            SessionNotificationSource::SessionEnded,
            "⏹ Session fix-login ended: exit code 0",
            Some(SessionEndNotice {
                outcome: "exit code 0".to_string(),
                exit_code: Some(0),
            }),
        )]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_end_whose_owner_has_no_sessions_base_is_not_published() {
    // Given a forwarder whose daemon user resolves to no sessions directory
    let recorder = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = Arc::new(SessionNotificationBus::new().with_subscriber(Arc::clone(&recorder)));
    let manager = CliSessionManager::new();
    let mut seen_by_the_manager = manager.agent_ends();
    let nowhere: SessionsBaseResolver = Arc::new(|_os_user: &str| None);
    let _forwarder = forward_agent_ends(
        manager.agent_ends(),
        bus,
        None,
        nowhere,
        DAEMON_USER.to_string(),
    );

    // When a session's agent ends, and the forwarder has had it
    let _agent = a_main_exiting_with(&manager, CHILD, 0).await;
    tokio::time::timeout(OBSERVE, seen_by_the_manager.recv())
        .await
        .expect("the manager never announced the end")
        .expect("the announcement channel closed");
    tokio::time::sleep(HELD).await;

    // Then nothing was published: a notification naming no owner reaches nobody
    assert_eq!(recorder.received(), Vec::new());
}
