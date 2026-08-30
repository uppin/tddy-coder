//! Acceptance: session notifications are a bus with subscribers, and Telegram is one of them.
//!
//! PRD: docs/ft/daemon/session-notifications.md
//! (FR1/AC1, FR2/AC3, FR7/AC4).
//!
//! The path under test is the real one: `ConnectionServiceImpl::report_session_status` is called
//! with a valid `hook_token`, exactly as the per-worktree `tddy-tools session-hook` calls it when
//! Claude Code fires a hook. One publish onto `SessionNotificationBus` must reach every interested
//! subscriber — the Telegram subscriber that ships today's copy, and any indicator subscriber
//! sitting beside it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::session_notification_subscribers::TelegramNotificationSubscriber;
use tddy_daemon::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotificationBus, SessionNotificationKind,
    SessionNotificationSource,
};
use tddy_daemon::telegram_notifier::{InMemoryTelegramSender, TelegramSessionWatcher};
use tddy_daemon::telegram_session_subscriber::TelegramDaemonHooks;
use tddy_daemon::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ReportSessionStatusRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const OS_USER: &str = "testuser";
const TEST_HOOK_TOKEN: &str = "tok-notifications-acceptance";
const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000001";
const BOUND_CHAT: i64 = 9999_i64;

/// The configured bot token. Named so the secrets assertion can name it rather than repeat a
/// literal that could drift out of step with the config builder below.
const BOT_TOKEN: &str = "test-token-5f3a9c";

/// The worktree the session under test works in. Its basename is what every surface must call the
/// session — the drawer already does, and after this changeset Telegram does too.
const SESSION_REPO_PATH: &str = "/home/dev/my-feature-branch";
const EXPECTED_SESSION_LABEL: &str = "my-feature-branch";

/// The `WaitingForInput` copy Telegram sends today, verbatim. Pinned here because the changeset
/// changes the *label* and nothing else about what an operator reads (FR7).
const WAITING_FOR_INPUT_TELEGRAM_TEXT: &str = "🔔 Session my-feature-branch: Claude Code needs your input (permission, question, or your next prompt). Attach via the web UI or `tddy-tools pty-relay`.";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_daemon_config() -> DaemonConfig {
    DaemonConfig {
        telegram: Some(tddy_daemon::config::TelegramConfig {
            enabled: true,
            bot_token: BOT_TOKEN.to_string(),
            chat_ids: vec![BOUND_CHAT],
        }),
        ..Default::default()
    }
}

/// A claude-cli `.session.yaml` carrying a hook token, an initial status, and the worktree the
/// session's label is derived from.
fn write_claude_cli_session(session_dir: &std::path::Path, initial_activity_status: &str) {
    std::fs::create_dir_all(session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "test-project".to_string(),
        created_at: "2026-08-29T10:00:00Z".to_string(),
        updated_at: "2026-08-29T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(SESSION_REPO_PATH.to_string()),
        pid: Some(12345),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-5".to_string()),
        cursor_chat_id: None,
        activity_status: Some(initial_activity_status.to_string()),
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
    write_session_metadata(session_dir, &meta).unwrap();
}

fn tracked_with_bound_chat() -> SharedTelegramTrackedSessionCoordinator {
    let tracked = Arc::new(StdMutex::new(TelegramTrackedSessionCoordinator::new()));
    tracked
        .lock()
        .unwrap()
        .bind_chat_to_session_for_telegram_tracking(BOUND_CHAT, SESSION_ID);
    tracked
}

fn telegram_hooks(sender: Arc<InMemoryTelegramSender>) -> Arc<TelegramDaemonHooks> {
    let watcher = TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
        Arc::new(StdMutex::new(std::collections::HashMap::new())),
        Arc::new(StdMutex::new(
            tddy_daemon::active_elicitation::ActiveElicitationCoordinator::new(),
        )),
        tracked_with_bound_chat(),
    );

    Arc::new(TelegramDaemonHooks {
        config: a_daemon_config(),
        sender: sender as Arc<dyn tddy_daemon::telegram_notifier::TelegramSender + Send + Sync>,
        watcher: Arc::new(tokio::sync::Mutex::new(watcher)),
    })
}

/// A `ConnectionServiceImpl` whose notification bus carries the Telegram subscriber and one
/// indicator subscriber — the shape `main.rs` assembles in production.
fn a_service_with_both_subscribers(
    sessions_base: PathBuf,
    sender: Arc<InMemoryTelegramSender>,
    indicators: Arc<RecordingSessionNotificationSubscriber>,
) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_os_user| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|_token| Some(OS_USER.to_string()));

    let hooks = telegram_hooks(sender);
    let bus = SessionNotificationBus::new()
        .with_subscriber(Arc::new(TelegramNotificationSubscriber::new(Arc::clone(
            &hooks,
        ))))
        .with_subscriber(indicators);

    ConnectionServiceImpl::new(
        a_daemon_config(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        Some(hooks),
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_session_notification_bus(Arc::new(bus))
}

async fn report_status(service: &ConnectionServiceImpl, status: &str) {
    service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: status.to_string(),
        }))
        .await
        .expect("report_session_status must succeed for a valid claude-cli session");
}

// ---------------------------------------------------------------------------
// AC1 — Telegram names a session the way the drawer names it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_waiting_for_input_hook_names_the_session_after_its_repository_directory() {
    // Given a claude-cli session working in /home/dev/my-feature-branch
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, "Running");

    let sender = Arc::new(InMemoryTelegramSender::new());
    let service = a_service_with_both_subscribers(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        Arc::new(RecordingSessionNotificationSubscriber::new()),
    );

    // When Claude Code's Notification hook reports that it is waiting on the operator
    report_status(&service, "WaitingForInput").await;

    // Then the chat reads the session's drawer name, not its uuid prefix
    let recorded = sender.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one Telegram message must be sent; got {recorded:?}"
    );
    assert_eq!(recorded[0].0, BOUND_CHAT);
    assert_eq!(recorded[0].1, WAITING_FOR_INPUT_TELEGRAM_TEXT);
}

// ---------------------------------------------------------------------------
// AC3 — one publish, every interested subscriber
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_waiting_for_input_hook_reaches_the_indicator_subscriber_alongside_telegram() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, "Running");

    let sender = Arc::new(InMemoryTelegramSender::new());
    let indicators = Arc::new(RecordingSessionNotificationSubscriber::new());
    let service = a_service_with_both_subscribers(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        Arc::clone(&indicators),
    );

    // When
    report_status(&service, "WaitingForInput").await;

    // Then Telegram sent its message ...
    assert_eq!(
        sender.recorded().len(),
        1,
        "Telegram must still be notified"
    );

    // ... and the indicator subscriber saw the same event, named the same way
    let seen = indicators.received();
    assert_eq!(
        seen.len(),
        1,
        "the indicator subscriber must receive exactly one notification; got {seen:?}"
    );
    assert_eq!(seen[0].session_id, SESSION_ID);
    assert_eq!(seen[0].label, EXPECTED_SESSION_LABEL);
    assert_eq!(seen[0].kind, SessionNotificationKind::AttentionRequired);
    assert_eq!(seen[0].source, SessionNotificationSource::ActivityStatus);
    assert_eq!(seen[0].text, WAITING_FOR_INPUT_TELEGRAM_TEXT);
}

// ---------------------------------------------------------------------------
// NFR4 — secrets stay out of notifications
// ---------------------------------------------------------------------------

/// A notification is operator-facing text that fans out to a chat and to every connected browser,
/// and it is built beside two secrets: the caller's `hook_token` and the configured `bot_token`.
/// Neither may ride along — not in the text, not in the label, not in the id.
#[tokio::test]
async fn keeps_the_hook_and_bot_tokens_out_of_everything_it_publishes() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, "Running");

    let sender = Arc::new(InMemoryTelegramSender::new());
    let indicators = Arc::new(RecordingSessionNotificationSubscriber::new());
    let service = a_service_with_both_subscribers(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        Arc::clone(&indicators),
    );

    // When — the hook authenticates with its token, on a daemon holding a bot token
    report_status(&service, "WaitingForInput").await;

    // Then — neither secret appears anywhere a person or a browser can read
    let notification = indicators
        .received()
        .into_iter()
        .next()
        .expect("the indicator subscriber must receive the notification");
    let published = format!(
        "{} {} {}",
        notification.text, notification.label, notification.session_id
    );
    assert!(
        !published.contains(TEST_HOOK_TOKEN),
        "a published notification must not carry the hook token; got {published:?}"
    );
    assert!(
        !published.contains(BOT_TOKEN),
        "a published notification must not carry the bot token; got {published:?}"
    );

    let sent = sender.recorded();
    assert_eq!(sent.len(), 1, "got {sent:?}");
    assert!(
        !sent[0].1.contains(TEST_HOOK_TOKEN) && !sent[0].1.contains(BOT_TOKEN),
        "a Telegram message must not carry either token; got {:?}",
        sent[0].1
    );
}

// ---------------------------------------------------------------------------
// AC4 — activity drives indicators and nothing else
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_executing_tool_hook_reaches_the_indicator_subscriber_but_sends_no_telegram_message() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, "Started");

    let sender = Arc::new(InMemoryTelegramSender::new());
    let indicators = Arc::new(RecordingSessionNotificationSubscriber::new());
    let service = a_service_with_both_subscribers(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        Arc::clone(&indicators),
    );

    // When the agent's PostToolUse hook reports it is running a tool
    report_status(&service, "ExecutingTool").await;

    // Then no chat is disturbed ...
    assert_eq!(
        sender.recorded().len(),
        0,
        "agent activity must not add Telegram traffic; got {:?}",
        sender.recorded()
    );

    // ... and the indicator subscriber has what it needs to blink the dot
    let seen = indicators.received();
    assert_eq!(
        seen.len(),
        1,
        "the indicator subscriber must receive exactly one notification; got {seen:?}"
    );
    assert_eq!(seen[0].session_id, SESSION_ID);
    assert_eq!(seen[0].label, EXPECTED_SESSION_LABEL);
    assert_eq!(seen[0].kind, SessionNotificationKind::Activity);
    assert_eq!(seen[0].source, SessionNotificationSource::ActivityStatus);
}
