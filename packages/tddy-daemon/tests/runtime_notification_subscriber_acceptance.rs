//! A process that embeds the daemon can listen to the daemon's session notifications.
//!
//! The notification bus is assembled inside `runtime::build` and the runtime hands out no handle on
//! it, so an embedding host (Tddy Desktop) cannot subscribe after the fact. It registers a
//! subscriber on the options instead, and `build` appends it after the two built-in subscribers.
//!
//! PRD: `docs/ft/desktop/1-WIP/PRD-2026-10-25-desktop-notifications.md`.

use std::path::Path;
use std::sync::Arc;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, DaemonRuntime, RuntimeOptions};
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService, ReportSessionStatusRequest};
use tddy_session_lifecycle::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotification, SessionNotificationKind,
    SessionNotificationSource,
};
use tokio::net::TcpListener;

const THE_OPERATOR: &str = "operator";
const THE_SESSION: &str = "01900000-0000-7000-8000-AABB00000077";
const THE_HOOK_TOKEN: &str = "tok-runtime-notification-subscriber";

#[tokio::test]
async fn a_subscriber_registered_on_runtime_options_receives_the_runtimes_notifications() {
    // Given an embedded runtime built with a subscriber registered on its options
    let data_dir = tempfile::tempdir().expect("a data directory");
    let session_dir = data_dir.path().join("sessions").join(THE_SESSION);
    a_claude_cli_session_in(&session_dir);
    let subscriber = Arc::new(RecordingSessionNotificationSubscriber::new());
    let runtime = an_embedded_runtime_notifying(&subscriber, data_dir.path()).await;

    // When a session of that runtime reports that it is waiting on its operator
    report_waiting_for_input(&runtime).await;

    // Then the subscriber was offered exactly that notification
    assert_notified_of(
        subscriber.received(),
        vec![an_attention_notification_for(THE_SESSION, THE_OPERATOR)],
    );
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The runtime a desktop assembles, with `subscriber` registered the way the desktop registers its
/// own. GitHub is a stub: a daemon with no sign-in mounts no session host and so has no bus.
async fn an_embedded_runtime_notifying(
    subscriber: &Arc<RecordingSessionNotificationSubscriber>,
    data_dir: &Path,
) -> DaemonRuntime {
    runtime::build(
        a_config_mapping_the_operator(data_dir).await,
        RuntimeOptions::for_embedded(
            tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        )
        .with_notification_subscriber(Arc::clone(subscriber) as _),
    )
    .await
    .expect("the embedded runtime did not build")
}

async fn a_config_mapping_the_operator(data_dir: &Path) -> DaemonConfig {
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         users:\n  - github_user: {THE_OPERATOR}\n    os_user: {THE_OPERATOR}\n\
         github:\n  stub: true\n  stub_codes: \"operator-code:{THE_OPERATOR}\"\n",
        port = a_free_tcp_port().await,
        data_dir = data_dir.display(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

/// A port that is free at this moment, so the fixture competes for no fixed one.
async fn a_free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available")
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

/// A claude-cli session directory carrying the hook token its hooks report with.
fn a_claude_cli_session_in(session_dir: &Path) {
    std::fs::create_dir_all(session_dir).expect("the session directory");
    let meta = SessionMetadata {
        session_id: THE_SESSION.to_string(),
        project_id: "test-project".to_string(),
        created_at: "2026-10-25T10:00:00Z".to_string(),
        updated_at: "2026-10-25T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/home/dev/api-gateway".to_string()),
        pid: Some(12345),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-5".to_string()),
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
        hook_token: Some(THE_HOOK_TOKEN.to_string()),
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
    write_session_metadata(session_dir, &meta).expect("the session metadata is written");
}

/// What the runtime's own `ReportSessionStatus` publishes for a hook that reports the session is
/// waiting on its operator — compared on the fields the operator's surfaces key on.
fn an_attention_notification_for(session_id: &str, os_user: &str) -> ExpectedNotification {
    ExpectedNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
    }
}

// ---------------------------------------------------------------------------
// Actions and assertions
// ---------------------------------------------------------------------------

async fn report_waiting_for_input(runtime: &DaemonRuntime) {
    runtime
        .session_host
        .as_ref()
        .expect("a runtime with sign-in mounts the session host")
        .activity_service()
        .report_session_status(Request::direct(ReportSessionStatusRequest {
            session_id: THE_SESSION.to_string(),
            hook_token: THE_HOOK_TOKEN.to_string(),
            os_user: THE_OPERATOR.to_string(),
            status: "WaitingForInput".to_string(),
            message: String::new(),
            ..Default::default()
        }))
        .await
        .expect("the hook's report was refused");
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedNotification {
    session_id: String,
    os_user: String,
    kind: SessionNotificationKind,
    source: SessionNotificationSource,
}

impl From<&SessionNotification> for ExpectedNotification {
    fn from(notification: &SessionNotification) -> Self {
        Self {
            session_id: notification.session_id.clone(),
            os_user: notification.os_user.clone(),
            kind: notification.kind,
            source: notification.source,
        }
    }
}

fn assert_notified_of(received: Vec<SessionNotification>, expected: Vec<ExpectedNotification>) {
    let received: Vec<ExpectedNotification> = received.iter().map(Into::into).collect();
    assert_eq!(
        received, expected,
        "the registered subscriber was not offered the runtime's notification"
    );
}
