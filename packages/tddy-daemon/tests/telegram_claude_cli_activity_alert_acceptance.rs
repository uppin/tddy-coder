//! Acceptance tests: Telegram alerts for claude-cli session activity status transitions
//! (PRD: docs/ft/daemon/telegram-notifications.md#claude-code-cli-session-activity-alerts).
//!
//! The full RPC path: `ConnectionServiceImpl::report_session_status` is called with a valid
//! `hook_token` and a status string. When Telegram is configured and a chat is tracking the
//! session, the watcher fires `on_claude_cli_activity_status_changed`, which sends to the
//! tracked chats.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
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
const TEST_HOOK_TOKEN: &str = "tok-acceptance-test";
const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000001";
const BOUND_CHAT: i64 = 9999_i64;

/// Build a minimal config with Telegram enabled (no real bot token needed for `InMemoryTelegramSender`).
fn test_daemon_config() -> DaemonConfig {
    DaemonConfig {
        telegram: Some(tddy_daemon::config::TelegramConfig {
            enabled: true,
            bot_token: "test-token".to_string(),
            chat_ids: vec![BOUND_CHAT],
        }),
        ..Default::default()
    }
}

/// Write a claude-cli `.session.yaml` with a hook_token and an initial activity_status.
fn write_claude_cli_session(
    session_dir: &std::path::Path,
    hook_token: &str,
    initial_activity_status: Option<&str>,
) {
    std::fs::create_dir_all(session_dir).unwrap();
    let meta = SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "test-project".to_string(),
        created_at: "2026-06-13T10:00:00Z".to_string(),
        updated_at: "2026-06-13T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/tmp/worktree-stub".to_string()),
        pid: Some(12345),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("claude-opus-4-8".to_string()),
        activity_status: initial_activity_status.map(str::to_owned),
        hook_token: Some(hook_token.to_string()),
        sandbox: None,
    };
    write_session_metadata(session_dir, &meta).unwrap();
}

/// Build a `ConnectionServiceImpl` wired with:
/// - `sessions_base_for_user` pointing at `sessions_base`
/// - `TelegramDaemonHooks` carrying `InMemoryTelegramSender` + a watcher sharing `tracked`
/// - A no-op user resolver (accepts OS_USER)
fn make_service(
    sessions_base: PathBuf,
    sender: Arc<InMemoryTelegramSender>,
    tracked: SharedTelegramTrackedSessionCoordinator,
) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_os_user| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "x" {
            Some(OS_USER.to_string())
        } else {
            None
        }
    });

    let watcher = TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        Arc::new(std::sync::Mutex::new(
            tddy_daemon::active_elicitation::ActiveElicitationCoordinator::new(),
        )),
        Arc::clone(&tracked),
    );
    let watcher_arc = Arc::new(tokio::sync::Mutex::new(watcher));

    let telegram = Arc::new(TelegramDaemonHooks {
        config: test_daemon_config(),
        sender: Arc::clone(&sender)
            as Arc<dyn tddy_daemon::telegram_notifier::TelegramSender + Send + Sync>,
        watcher: watcher_arc,
    });

    ConnectionServiceImpl::new(
        test_daemon_config(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        Some(telegram),
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

/// Shared tracked coordinator with BOUND_CHAT already tracking SESSION_ID.
fn tracked_with_bound_chat() -> SharedTelegramTrackedSessionCoordinator {
    let tracked = Arc::new(std::sync::Mutex::new(
        TelegramTrackedSessionCoordinator::new(),
    ));
    tracked
        .lock()
        .unwrap()
        .bind_chat_to_session_for_telegram_tracking(BOUND_CHAT, SESSION_ID);
    tracked
}

/// **waiting_for_input_transition_alerts_tracked_chat**: when `report_session_status` receives
/// `WaitingForInput` and a chat is tracking the session, exactly one Telegram message must be
/// sent to that chat. The message must mention input or needs.
#[tokio::test]
async fn waiting_for_input_transition_alerts_tracked_chat() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    // unified_session_dir_path appends "sessions/<id>" under the sessions_base the resolver returns.
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN, Some("Running"));

    let sender = Arc::new(InMemoryTelegramSender::new());
    let service = make_service(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        tracked_with_bound_chat(),
    );

    // When
    let response = service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: "WaitingForInput".to_string(),
        }))
        .await
        .expect("report_session_status must succeed for valid claude-cli session");

    // Then
    assert!(response.into_inner().ok, "response.ok must be true");

    let recorded = sender.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one Telegram message must be sent on WaitingForInput transition; got {recorded:?}"
    );
    assert_eq!(
        recorded[0].0, BOUND_CHAT,
        "message must be sent to the tracked chat; got chat_id={}",
        recorded[0].0
    );
    let text = &recorded[0].1;
    assert!(
        text.contains("input") || text.contains("needs"),
        "WaitingForInput alert must mention input or needs; got {text:?}"
    );
}

/// **done_transition_alerts_tracked_chat**: `Done` status → one message to the tracked chat,
/// text mentions finished or turn.
#[tokio::test]
async fn done_transition_alerts_tracked_chat() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN, Some("Running"));

    let sender = Arc::new(InMemoryTelegramSender::new());
    let service = make_service(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        tracked_with_bound_chat(),
    );

    // When
    service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: "Done".to_string(),
        }))
        .await
        .expect("report_session_status must succeed");

    // Then
    let recorded = sender.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "Done must send exactly one Telegram message; got {recorded:?}"
    );
    let text = &recorded[0].1;
    assert!(
        text.contains("finished") || text.contains("turn"),
        "Done alert must mention finished or turn; got {text:?}"
    );
}

/// **repeated_same_status_does_not_realert**: calling `report_session_status` twice with
/// `WaitingForInput` (no change in between) must produce exactly one Telegram message.
#[tokio::test]
async fn repeated_same_status_does_not_realert() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN, Some("Running"));

    let sender = Arc::new(InMemoryTelegramSender::new());
    let service = make_service(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        tracked_with_bound_chat(),
    );

    // When
    for _ in 0..2 {
        service
            .report_session_status(Request::new(ReportSessionStatusRequest {
                session_id: SESSION_ID.to_string(),
                hook_token: TEST_HOOK_TOKEN.to_string(),
                os_user: OS_USER.to_string(),
                status: "WaitingForInput".to_string(),
            }))
            .await
            .expect("report_session_status must succeed");
    }

    // Then
    assert_eq!(
        sender.recorded().len(),
        1,
        "repeated same status must not send a second Telegram alert; got {:?}",
        sender.recorded()
    );
}

/// **no_alert_when_no_chat_tracks_session**: `report_session_status` with `WaitingForInput`
/// when no chat is tracking the session must NOT send any Telegram message. The RPC must still
/// return `ok = true`.
#[tokio::test]
async fn no_alert_when_no_chat_tracks_session() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN, Some("Running"));

    let sender = Arc::new(InMemoryTelegramSender::new());
    // No bind — untracked session.
    let tracked = Arc::new(std::sync::Mutex::new(
        TelegramTrackedSessionCoordinator::new(),
    ));
    let service = make_service(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        tracked,
    );

    // When
    let resp = service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: "WaitingForInput".to_string(),
        }))
        .await
        .expect("report_session_status must succeed even when session is untracked");

    // Then
    assert!(resp.into_inner().ok, "response.ok must be true");
    assert!(
        sender.recorded().is_empty(),
        "no chat tracks the session — no Telegram message must be sent; got {:?}",
        sender.recorded()
    );
}

/// **running_status_does_not_alert**: `Running` is a non-alerting status; calling
/// `report_session_status` with `Running` must not produce any Telegram message.
#[tokio::test]
async fn running_status_does_not_alert() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let session_dir = sessions_tmp.path().join("sessions").join(SESSION_ID);
    write_claude_cli_session(&session_dir, TEST_HOOK_TOKEN, Some("Started"));

    let sender = Arc::new(InMemoryTelegramSender::new());
    let service = make_service(
        sessions_tmp.path().to_path_buf(),
        Arc::clone(&sender),
        tracked_with_bound_chat(),
    );

    // When
    service
        .report_session_status(Request::new(ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: TEST_HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: "Running".to_string(),
        }))
        .await
        .expect("report_session_status must succeed");

    // Then
    assert!(
        sender.recorded().is_empty(),
        "Running status must not produce a Telegram alert; got {:?}",
        sender.recorded()
    );
}
