//! Telegram as the daemon's operator channel: the port the session host and the notification bus
//! reach a chat through, with the interest and routing the separate bus subscriber used to keep.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md`, change 1. What a
//! chat receives does not change; only the seam it arrives through does. The exclusions are the
//! ones PRD `docs/ft/daemon/session-notifications.md` FR7 set: `Activity` would turn every tool
//! call into a message, and a presenter's elicitations already reach the chat through
//! `on_presenter_event`, keyboards attached — taking them off the bus too would send each twice.

use std::sync::{Arc, Mutex as StdMutex};

use tddy_daemon_kernel::config::{DaemonConfig, TelegramConfig};
use tddy_daemon_kernel::operator_channel::{OperatorChannel, SharedOperatorChannel};
use tddy_daemon_kernel::session_notification::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};
use tddy_telegram::active_elicitation::ActiveElicitationCoordinator;
use tddy_telegram::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};
use tddy_telegram_control::telegram_notifier::{
    InMemoryTelegramSender, TelegramSender, TelegramSessionWatcher,
};
use tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000101";
const LABEL: &str = "my-feature-branch";
const TRACKING_CHAT: i64 = 111;
const BROADCAST_CHAT: i64 = 222;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn an_attention_notification() -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: "testuser".to_string(),
        label: LABEL.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: format!("🔔 Session {LABEL}: Claude Code needs your input."),
        at_unix_ms: 1_756_000_000_000,
    }
}

fn an_activity_notification() -> SessionNotification {
    SessionNotification {
        kind: SessionNotificationKind::Activity,
        text: format!("Session {LABEL}: agent is working"),
        ..an_attention_notification()
    }
}

fn a_presenter_attention_notification() -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::Presenter,
        text: format!("🔔 Session {LABEL}: waiting for your answer."),
        ..an_attention_notification()
    }
}

fn a_telegram_block(enabled: bool) -> DaemonConfig {
    DaemonConfig {
        telegram: Some(TelegramConfig {
            enabled,
            bot_token: "test-token".to_string(),
            chat_ids: vec![BROADCAST_CHAT],
        }),
        ..Default::default()
    }
}

fn a_chat_tracking_the_session() -> SharedTelegramTrackedSessionCoordinator {
    let tracked = Arc::new(StdMutex::new(TelegramTrackedSessionCoordinator::new()));
    tracked
        .lock()
        .unwrap()
        .bind_chat_to_session_for_telegram_tracking(TRACKING_CHAT, SESSION_ID);
    tracked
}

/// Telegram's channel over an in-memory sender, so a test reads what a chat would have received.
fn a_telegram_channel(config: DaemonConfig) -> (SharedOperatorChannel, Arc<InMemoryTelegramSender>) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let watcher = TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
        Arc::new(StdMutex::new(std::collections::HashMap::new())),
        Arc::new(StdMutex::new(ActiveElicitationCoordinator::new())),
        a_chat_tracking_the_session(),
    );
    let hooks = Arc::new(TelegramDaemonHooks {
        config,
        sender: Arc::clone(&sender) as Arc<dyn TelegramSender + Send + Sync>,
        watcher: Arc::new(tokio::sync::Mutex::new(watcher)),
    });
    (hooks, sender)
}

// ---------------------------------------------------------------------------
// What the channel wants
// ---------------------------------------------------------------------------

#[test]
fn the_telegram_channel_wants_an_attention_alert_from_the_activity_status_path() {
    // Given
    let (channel, _) = a_telegram_channel(a_telegram_block(true));

    // When
    let wants = channel.wants_notification(&an_attention_notification());

    // Then
    assert!(wants);
}

#[test]
fn the_telegram_channel_declines_activity_and_presenter_notifications() {
    // Given
    let (channel, _) = a_telegram_channel(a_telegram_block(true));

    // When
    let wanted = (
        channel.wants_notification(&an_activity_notification()),
        channel.wants_notification(&a_presenter_attention_notification()),
    );

    // Then
    assert_eq!(wanted, (false, false));
}

#[test]
fn a_switched_off_telegram_channel_wants_nothing() {
    // Given
    let (channel, _) = a_telegram_channel(a_telegram_block(false));

    // When
    let wants = channel.wants_notification(&an_attention_notification());

    // Then
    assert!(!wants);
}

#[test]
fn the_telegram_channel_is_identified_as_telegram() {
    // Given
    let (channel, _) = a_telegram_channel(a_telegram_block(true));

    // When / Then — the id it is logged under and reports on the status board
    assert_eq!(channel.id(), "telegram");
}

// ---------------------------------------------------------------------------
// Who it reaches
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_attention_alert_reaches_the_tracked_chat_through_the_channel() {
    // Given
    let (channel, sender) = a_telegram_channel(a_telegram_block(true));

    // When
    channel
        .on_notification(&an_attention_notification())
        .await
        .expect("the in-memory sender delivers");

    // Then — the tracking chat only, never the broadcast list, and the bus's text verbatim
    assert_eq!(
        sender.recorded(),
        vec![(TRACKING_CHAT, an_attention_notification().text)]
    );
}

#[tokio::test]
async fn an_unchanged_alert_reaches_the_chat_once() {
    // Given
    let (channel, sender) = a_telegram_channel(a_telegram_block(true));

    // When — the status is re-reported without changing
    for _ in 0..2 {
        channel
            .on_notification(&an_attention_notification())
            .await
            .expect("the in-memory sender delivers");
    }

    // Then
    assert_eq!(sender.recorded().len(), 1);
}
