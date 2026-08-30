//! Acceptance: `ConnectionService.StreamSessionNotifications` — the daemon-level feed `tddy-web`
//! subscribes to once for the whole session drawer.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md (FR3, AC5).
//!
//! One stream carries every session on the daemon, so the drawer pays for one subscription
//! however many rows it shows (NFR1). Each event carries the session's drawer label and the
//! operator-facing text, so a row can name and describe what happened without a second lookup.
//!
//! "Every session on the daemon" means every session belonging to the *subscriber*: the bus behind
//! this stream is host-wide, so on a daemon serving several operators the relay is the only thing
//! standing between one of them and another's session ids, repository names and prose. This suite
//! is therefore written against a two-operator daemon throughout.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::config::{DaemonConfig, UserMapping};
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::session_notification_subscribers::SessionNotificationStreamSubscriber;
use tddy_daemon::session_notifications::SessionNotificationBus;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ReportSessionStatusRequest,
    SessionNotificationEvent, SessionNotificationKind as ProtoSessionNotificationKind,
    SessionNotificationSource as ProtoSessionNotificationSource, StreamSessionNotificationsRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The operator these tests subscribe as: a GitHub identity a `session_token` authenticates, and
/// the OS user `users[]` maps it to. The stream is scoped to the latter.
const GITHUB_USER: &str = "octocat";
const OS_USER: &str = "testuser";
const SESSION_TOKEN: &str = "valid-session-token";

/// The daemon's *other* operator. Nothing of theirs may reach the client subscribed above.
const OTHER_USERS_GITHUB_USER: &str = "hubot";
const OTHER_USERS_OS_USER: &str = "otheruser";

/// A token that authenticates a GitHub user `users[]` maps to no OS user — an operator with an
/// account on the daemon and no sessions directory on this host.
const UNMAPPED_GITHUB_USER: &str = "stranger";
const UNMAPPED_SESSION_TOKEN: &str = "unmapped-session-token";

const TEST_HOOK_TOKEN: &str = "tok-notification-stream-acceptance";

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000042";
const SESSION_REPO_PATH: &str = "/home/dev/streaming-branch";
const SESSION_LABEL: &str = "streaming-branch";

const OTHER_SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000043";
const OTHER_REPO_PATH: &str = "/home/dev/other-branch";
const OTHER_SESSION_LABEL: &str = "other-branch";

/// A session belonging to the daemon's other operator. Its repository name is the kind of thing
/// the leak this scoping prevents would disclose.
const OTHER_USERS_SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000044";
const OTHER_USERS_REPO_PATH: &str = "/home/otheruser/private-branch";

/// How long a client is watched for an event that must never arrive. The publish it must not carry
/// has already completed when the wait begins, so this is time for the relay task to run and drop
/// it — not time for it to happen.
const SILENCE_WINDOW: Duration = Duration::from_millis(200);

const WAITING_FOR_INPUT_TEXT: &str = "🔔 Session streaming-branch: Claude Code needs your input (permission, question, or your next prompt). Attach via the web UI or `tddy-tools pty-relay`.";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A claude-cli `.session.yaml` carrying the hook token the tests report with, and the worktree
/// its drawer label is derived from.
fn write_claude_cli_session(sessions_base: &Path, session_id: &str, repo_path: &str) {
    let session_dir = sessions_base.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: "test-project".to_string(),
        created_at: "2026-08-29T10:00:00Z".to_string(),
        updated_at: "2026-08-29T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(repo_path.to_string()),
        pid: Some(12345),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-5".to_string()),
        cursor_chat_id: None,
        activity_status: Some("Running".to_string()),
        hook_token: Some(TEST_HOOK_TOKEN.to_string()),
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
    write_session_metadata(&session_dir, &meta).unwrap();
}

/// A daemon serving two operators, plus a third GitHub identity mapped to no OS user at all.
/// `os_user_for_github` reads this list, and it is what decides whose sessions a subscribed client
/// is shown.
fn a_config_for_two_operators() -> DaemonConfig {
    DaemonConfig {
        users: vec![
            UserMapping {
                github_user: GITHUB_USER.to_string(),
                os_user: OS_USER.to_string(),
            },
            UserMapping {
                github_user: OTHER_USERS_GITHUB_USER.to_string(),
                os_user: OTHER_USERS_OS_USER.to_string(),
            },
        ],
        ..DaemonConfig::default()
    }
}

/// A service whose only notification subscriber is the one backing `StreamSessionNotifications`.
/// Telegram is deliberately absent: this suite is about the second subscriber standing on its own.
///
/// Both operators' sessions live under the one `sessions_base`, because that is what
/// `sessions_base_for_user` answers for every user once a data directory is configured — the shape
/// a real daemon runs in. A session's owner is therefore not something the directory layout says;
/// it is what the reporting hook names, and what the notification carries.
fn a_service_with_a_notification_stream(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_os_user| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| match token {
        SESSION_TOKEN => Some(GITHUB_USER.to_string()),
        UNMAPPED_SESSION_TOKEN => Some(UNMAPPED_GITHUB_USER.to_string()),
        _ => None,
    });

    let bus = SessionNotificationBus::new()
        .with_subscriber(Arc::new(SessionNotificationStreamSubscriber::new()));

    ConnectionServiceImpl::new(
        a_config_for_two_operators(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_session_notification_bus(Arc::new(bus))
}

type NotificationStream =
    <ConnectionServiceImpl as ConnectionServiceTrait>::StreamSessionNotificationsStream;

async fn a_subscribed_client(service: &ConnectionServiceImpl) -> NotificationStream {
    service
        .stream_session_notifications(Request::new(StreamSessionNotificationsRequest {
            session_token: SESSION_TOKEN.to_string(),
        }))
        .await
        .expect("StreamSessionNotifications must accept a valid session token")
        .into_inner()
}

async fn report_status(service: &ConnectionServiceImpl, session_id: &str, status: &str) {
    report_status_owned_by(service, OS_USER, session_id, status).await;
}

/// The same hook, reported for a session belonging to `os_user`. The owner a hook names is the
/// owner the notification carries.
async fn report_status_owned_by(
    service: &ConnectionServiceImpl,
    os_user: &str,
    session_id: &str,
    status: &str,
) {
    service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: session_id.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: os_user.to_string(),
            status: status.to_string(),
        }))
        .await
        .expect("report_session_status must succeed for a valid claude-cli session");
}

async fn next_event(stream: &mut NotificationStream) -> SessionNotificationEvent {
    tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("no notification arrived within the timeout")
        .expect("notification stream closed unexpectedly")
        .expect("notification stream yielded an error")
}

/// Asserts `stream` carries nothing at all. A stream that must stay silent can only be observed by
/// waiting on it, so the wait itself is the assertion; it names the session that broke it.
async fn assert_stream_stays_silent(stream: &mut NotificationStream) {
    let leaked = tokio::time::timeout(SILENCE_WINDOW, stream.next())
        .await
        .ok()
        .flatten()
        .and_then(|event| event.ok())
        .map(|event| event.session_id);
    assert_eq!(
        leaked, None,
        "the stream carried a notification it should never have been offered"
    );
}

// ---------------------------------------------------------------------------
// AC5 — an attention event names and describes the session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn streams_an_attention_event_carrying_the_session_drawer_label_and_its_operator_text() {
    // Given a claude-cli session and a client subscribed to the daemon's notification feed
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When the session's Notification hook reports that it is waiting on the operator
    report_status(&service, SESSION_ID, "WaitingForInput").await;

    // Then the subscribed client is told which session needs attention, and what to read
    let event = next_event(&mut client).await;
    assert_eq!(event.session_id, SESSION_ID);
    assert_eq!(event.label, SESSION_LABEL);
    assert_eq!(
        event.kind,
        ProtoSessionNotificationKind::AttentionRequired as i32
    );
    assert_eq!(
        event.source,
        ProtoSessionNotificationSource::ActivityStatus as i32
    );
    assert_eq!(event.text, WAITING_FOR_INPUT_TEXT);
}

/// The web ages activity out of the blink window against this stamp, so an unstamped event would
/// leave a dot blinking for as long as the tab stayed open.
#[tokio::test]
async fn stamps_every_notification_with_the_moment_it_happened() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When
    report_status(&service, SESSION_ID, "WaitingForInput").await;

    // Then — a wall-clock stamp cannot be asserted exactly; it must be a real epoch millisecond,
    // which rules out both an unset field and a seconds-resolution one.
    let event = next_event(&mut client).await;
    assert!(
        event.at_unix_ms > 1_700_000_000_000,
        "notification must be stamped in epoch milliseconds; got {}",
        event.at_unix_ms
    );
}

#[tokio::test]
async fn streams_an_activity_event_that_telegram_would_never_have_sent() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When the agent reports it is executing a tool
    report_status(&service, SESSION_ID, "ExecutingTool").await;

    // Then the indicator feed carries it as activity
    let event = next_event(&mut client).await;
    assert_eq!(event.session_id, SESSION_ID);
    assert_eq!(event.label, SESSION_LABEL);
    assert_eq!(event.kind, ProtoSessionNotificationKind::Activity as i32);
}

#[tokio::test]
async fn rejects_a_notification_stream_opened_without_a_valid_session_token() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());

    // When
    let result = service
        .stream_session_notifications(Request::new(StreamSessionNotificationsRequest {
            session_token: "not-a-token".to_string(),
        }))
        .await;

    // Then
    let status = result.err().expect("an invalid token must be rejected");
    assert_eq!(status.code(), tddy_rpc::Code::Unauthenticated);
}

/// NFR1: one subscription, every session. A per-session feed would carry only the session it was
/// opened for, and the drawer would need one stream per row.
#[tokio::test]
async fn carries_every_session_on_the_daemon_over_a_single_subscription() {
    // Given two claude-cli sessions in different worktrees, and one subscribed client
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    write_claude_cli_session(sessions_tmp.path(), OTHER_SESSION_ID, OTHER_REPO_PATH);
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When each session reports, in turn
    report_status(&service, SESSION_ID, "ExecutingTool").await;
    report_status(&service, OTHER_SESSION_ID, "WaitingForInput").await;

    // Then the one subscription carries both, each named after its own worktree
    let first = next_event(&mut client).await;
    let second = next_event(&mut client).await;
    assert_eq!(first.session_id, SESSION_ID);
    assert_eq!(first.label, SESSION_LABEL);
    assert_eq!(second.session_id, OTHER_SESSION_ID);
    assert_eq!(second.label, OTHER_SESSION_LABEL);
}

/// Two browser tabs are two clients; neither may consume the other's copy.
#[tokio::test]
async fn delivers_the_same_notification_to_every_connected_client() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut first_client = a_subscribed_client(&service).await;
    let mut second_client = a_subscribed_client(&service).await;

    // When
    report_status(&service, SESSION_ID, "WaitingForInput").await;

    // Then
    assert_eq!(
        next_event(&mut first_client).await.text,
        WAITING_FOR_INPUT_TEXT
    );
    assert_eq!(
        next_event(&mut second_client).await.text,
        WAITING_FOR_INPUT_TEXT
    );
}

// ---------------------------------------------------------------------------
// One operator's feed is only their own
// ---------------------------------------------------------------------------

/// Every session on the host passes through the bus this stream subscribes to, so the relay is the
/// only thing scoping it. A notification names a session id, the basename of the repository being
/// worked on and the operator-facing sentence describing it — none of which belongs to a client
/// that does not own the session.
#[tokio::test]
async fn withholds_a_notification_for_a_session_owned_by_another_user() {
    // Given the daemon's other operator has a session, and our client is subscribed to the feed
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(
        sessions_tmp.path(),
        OTHER_USERS_SESSION_ID,
        OTHER_USERS_REPO_PATH,
    );
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When that session reports that it is waiting on its own operator
    report_status_owned_by(
        &service,
        OTHER_USERS_OS_USER,
        OTHER_USERS_SESSION_ID,
        "WaitingForInput",
    )
    .await;

    // Then our client is told nothing
    assert_stream_stays_silent(&mut client).await;
}

/// The filter must drop what it withholds, not stall on it. A relay that stopped forwarding at the
/// first foreign notification would satisfy the test above and leave the drawer dark from then on.
#[tokio::test]
async fn keeps_delivering_its_own_sessions_while_another_users_notifications_flow() {
    // Given a session for each operator, and our client subscribed to the feed
    let sessions_tmp = tempfile::tempdir().unwrap();
    write_claude_cli_session(sessions_tmp.path(), SESSION_ID, SESSION_REPO_PATH);
    write_claude_cli_session(
        sessions_tmp.path(),
        OTHER_USERS_SESSION_ID,
        OTHER_USERS_REPO_PATH,
    );
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());
    let mut client = a_subscribed_client(&service).await;

    // When the other operator's session reports first, and ours second
    report_status_owned_by(
        &service,
        OTHER_USERS_OS_USER,
        OTHER_USERS_SESSION_ID,
        "WaitingForInput",
    )
    .await;
    report_status(&service, SESSION_ID, "ExecutingTool").await;

    // Then the first thing our client sees is our own session — the other was dropped, not queued
    let event = next_event(&mut client).await;
    assert_eq!(event.session_id, SESSION_ID);
    assert_eq!(event.label, SESSION_LABEL);
}

/// A token can be genuine and still own nothing here. `ListSessions` answers such a caller with
/// `permission_denied`; a feed of every session on the host must not be the one RPC that answers
/// it with data instead.
#[tokio::test]
async fn rejects_a_notification_stream_opened_by_a_user_mapped_to_no_os_user() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = a_service_with_a_notification_stream(sessions_tmp.path().to_path_buf());

    // When
    let result = service
        .stream_session_notifications(Request::new(StreamSessionNotificationsRequest {
            session_token: UNMAPPED_SESSION_TOKEN.to_string(),
        }))
        .await;

    // Then
    let status = result
        .err()
        .expect("a token mapped to no OS user must be rejected");
    assert_eq!(status.code(), tddy_rpc::Code::PermissionDenied);
}
