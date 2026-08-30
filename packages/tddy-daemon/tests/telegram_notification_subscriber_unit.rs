//! Unit: `TelegramNotificationSubscriber` — Telegram's interest in the notification bus, and the
//! dedupe and routing it keeps from today's `TelegramSessionWatcher`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md (FR2, FR7).
//!
//! Two things must hold after the extraction. **Telegram gets no new traffic**: it declines every
//! `ACTIVITY` notification, and it declines `ATTENTION_REQUIRED` raised by the presenter, whose
//! keyboard-bearing elicitation surface still ships through `telegram_notifier`. And **the
//! recipient rules survive**: tracked chats first, the configured broadcast list otherwise, and a
//! status re-reported without a change in between sends nothing a second time.

use std::sync::{Arc, Mutex as StdMutex};

use tddy_daemon::config::{DaemonConfig, TelegramConfig};
use tddy_daemon::session_notification_subscribers::TelegramNotificationSubscriber;
use tddy_daemon::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
    SessionNotificationSubscriber,
};
use tddy_daemon::telegram_notifier::{InMemoryTelegramSender, TelegramSessionWatcher};
use tddy_daemon::telegram_session_subscriber::TelegramDaemonHooks;
use tddy_daemon::telegram_tracked_session::{
    SharedTelegramTrackedSessionCoordinator, TelegramTrackedSessionCoordinator,
};

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000001";
/// The OS user the session belongs to. Every notification names one: it is what the notification
/// stream scopes a subscribed client to. Telegram routes by tracked chat instead, so this
/// suite's expectations are unaffected by it.
const OS_USER: &str = "testuser";
const LABEL: &str = "my-feature-branch";
const AT: u64 = 1_756_000_000_000;

const TRACKING_CHAT: i64 = 111;
/// A chat that has claimed a *different* session — it must never receive this session's alerts.
const OTHER_SESSIONS_CHAT: i64 = 444;
const OTHER_SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000099";
const BROADCAST_CHAT: i64 = 222;
const SECOND_BROADCAST_CHAT: i64 = 333;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn an_attention_notification() -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: OS_USER.to_string(),
        label: LABEL.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: format!("🔔 Session {LABEL}: Claude Code needs your input."),
        at_unix_ms: AT,
    }
}

fn an_activity_notification() -> SessionNotification {
    SessionNotification {
        kind: SessionNotificationKind::Activity,
        text: format!("Session {LABEL}: agent is working"),
        ..an_attention_notification()
    }
}

/// An attention notification raised by a `tddy-coder` workflow session's presenter — the class
/// whose Telegram surface carries inline keyboards and therefore is not this subscriber's to send.
fn a_presenter_attention_notification() -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::Presenter,
        text: format!("Session {LABEL}: approval needed — review the document above."),
        ..an_attention_notification()
    }
}

/// Config whose broadcast list holds `BROADCAST_CHAT` and nothing else.
fn a_config_broadcasting_to_one_chat() -> DaemonConfig {
    DaemonConfig {
        telegram: Some(TelegramConfig {
            enabled: true,
            bot_token: "test-token".to_string(),
            chat_ids: vec![BROADCAST_CHAT],
        }),
        ..Default::default()
    }
}

/// Config whose broadcast list holds two chats, in order.
fn a_config_broadcasting_to_two_chats() -> DaemonConfig {
    DaemonConfig {
        telegram: Some(TelegramConfig {
            enabled: true,
            bot_token: "test-token".to_string(),
            chat_ids: vec![BROADCAST_CHAT, SECOND_BROADCAST_CHAT],
        }),
        ..Default::default()
    }
}

fn no_chat_tracks_the_session() -> SharedTelegramTrackedSessionCoordinator {
    Arc::new(StdMutex::new(TelegramTrackedSessionCoordinator::new()))
}

fn a_chat_tracking_the_session() -> SharedTelegramTrackedSessionCoordinator {
    let tracked = no_chat_tracks_the_session();
    tracked
        .lock()
        .unwrap()
        .bind_chat_to_session_for_telegram_tracking(TRACKING_CHAT, SESSION_ID);
    tracked
}

/// Two chats have each claimed a different session — the shape a shared daemon actually has.
fn two_chats_tracking_different_sessions() -> SharedTelegramTrackedSessionCoordinator {
    let tracked = a_chat_tracking_the_session();
    tracked
        .lock()
        .unwrap()
        .bind_chat_to_session_for_telegram_tracking(OTHER_SESSIONS_CHAT, OTHER_SESSION_ID);
    tracked
}

/// A subscriber over an in-memory sender, so a test reads what would have been sent.
fn a_telegram_subscriber(
    config: DaemonConfig,
    tracked: SharedTelegramTrackedSessionCoordinator,
) -> (TelegramNotificationSubscriber, Arc<InMemoryTelegramSender>) {
    let sender = Arc::new(InMemoryTelegramSender::new());
    let watcher = TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
        Arc::new(StdMutex::new(std::collections::HashMap::new())),
        Arc::new(StdMutex::new(
            tddy_daemon::active_elicitation::ActiveElicitationCoordinator::new(),
        )),
        tracked,
    );
    let hooks = Arc::new(TelegramDaemonHooks {
        config,
        sender: Arc::clone(&sender)
            as Arc<dyn tddy_daemon::telegram_notifier::TelegramSender + Send + Sync>,
        watcher: Arc::new(tokio::sync::Mutex::new(watcher)),
    });
    (TelegramNotificationSubscriber::new(hooks), sender)
}

// ---------------------------------------------------------------------------
// What Telegram is interested in
// ---------------------------------------------------------------------------

#[test]
fn wants_an_attention_notification_from_the_activity_status_path() {
    // Given
    let (subscriber, _) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );

    // When / Then
    assert!(subscriber.wants(&an_attention_notification()));
}

/// FR7: the indicator feed is why `ACTIVITY` exists. Sending it to a chat would turn every tool
/// call into a message.
#[test]
fn declines_an_activity_notification() {
    // Given
    let (subscriber, _) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );

    // When / Then
    assert!(!subscriber.wants(&an_activity_notification()));
}

/// The presenter's elicitations still ship through `telegram_notifier`, keyboards and per-chat
/// queue included. Taking them here too would send each one twice.
#[test]
fn declines_an_attention_notification_raised_by_the_presenter() {
    // Given
    let (subscriber, _) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );

    // When / Then
    assert!(!subscriber.wants(&a_presenter_attention_notification()));
}

#[test]
fn declines_everything_when_telegram_is_not_configured() {
    // Given — a daemon with no `telegram:` block
    let (subscriber, _) =
        a_telegram_subscriber(DaemonConfig::default(), no_chat_tracks_the_session());

    // When / Then
    assert!(!subscriber.wants(&an_attention_notification()));
}

// ---------------------------------------------------------------------------
// Who it reaches
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sends_only_to_the_chats_tracking_the_session_when_any_do() {
    // Given — one chat is tracking the session, another is on the broadcast list
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        a_chat_tracking_the_session(),
    );

    // When
    subscriber
        .deliver(&an_attention_notification())
        .await
        .expect("delivery to an in-memory sender must succeed");

    // Then — the operator who claimed the session is the one told about it
    let recorded = sender.recorded();
    assert_eq!(recorded.len(), 1, "got {recorded:?}");
    assert_eq!(recorded[0].0, TRACKING_CHAT);
}

/// Tracking is per session, not per daemon: an operator who claimed a different session is not a
/// recipient here, and neither is the broadcast list while anyone holds this one.
#[tokio::test]
async fn does_not_reach_a_chat_that_claimed_a_different_session() {
    // Given — this session is claimed by one chat, another session by a second chat, and a third
    // chat is on the broadcast list
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        two_chats_tracking_different_sessions(),
    );

    // When
    subscriber
        .deliver(&an_attention_notification())
        .await
        .expect("delivery to an in-memory sender must succeed");

    // Then
    let chats: Vec<i64> = sender.recorded().iter().map(|(chat, _)| *chat).collect();
    assert_eq!(chats, vec![TRACKING_CHAT]);
}

#[tokio::test]
async fn falls_back_to_the_broadcast_list_when_no_chat_tracks_the_session() {
    // Given — a session started from the web UI, which no chat has claimed
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );

    // When
    subscriber
        .deliver(&an_attention_notification())
        .await
        .expect("delivery to an in-memory sender must succeed");

    // Then
    let recorded = sender.recorded();
    assert_eq!(recorded.len(), 1, "got {recorded:?}");
    assert_eq!(recorded[0].0, BROADCAST_CHAT);
}

/// Every configured chat is a recipient, not just the first — the fan-out an operator relies on
/// when several people watch the same daemon.
#[tokio::test]
async fn sends_to_every_chat_on_the_broadcast_list() {
    // Given
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_two_chats(),
        no_chat_tracks_the_session(),
    );

    // When
    subscriber
        .deliver(&an_attention_notification())
        .await
        .expect("delivery to an in-memory sender must succeed");

    // Then
    let chats: Vec<i64> = sender.recorded().iter().map(|(chat, _)| *chat).collect();
    assert_eq!(chats, vec![BROADCAST_CHAT, SECOND_BROADCAST_CHAT]);
}

#[tokio::test]
async fn sends_the_notification_text_verbatim() {
    // Given
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );
    let notification = an_attention_notification();

    // When
    subscriber.deliver(&notification).await.unwrap();

    // Then — the bus owns the copy; the subscriber is a delivery mechanism, not an author
    assert_eq!(sender.recorded()[0].1, notification.text);
}

#[tokio::test]
async fn sends_nothing_when_no_chat_tracks_the_session_and_the_broadcast_list_is_empty() {
    // Given — Telegram enabled but with nobody to tell
    let config = DaemonConfig {
        telegram: Some(TelegramConfig {
            enabled: true,
            bot_token: "test-token".to_string(),
            chat_ids: Vec::new(),
        }),
        ..Default::default()
    };
    let (subscriber, sender) = a_telegram_subscriber(config, no_chat_tracks_the_session());

    // When
    subscriber
        .deliver(&an_attention_notification())
        .await
        .unwrap();

    // Then
    assert_eq!(sender.recorded().len(), 0);
}

// ---------------------------------------------------------------------------
// Dedupe stays here, not on the bus
// ---------------------------------------------------------------------------

/// The bus publishes every reported status so the web can keep a dot alive; Telegram must not turn
/// that into repeat messages. Suppression therefore lives in this subscriber, and it is the
/// behaviour `repeated_same_status_does_not_realert` already pins through the RPC.
#[tokio::test]
async fn does_not_send_the_same_attention_notification_twice_in_a_row() {
    // Given
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );

    // When — the same notification is published twice, as a re-reported status would be
    subscriber
        .deliver(&an_attention_notification())
        .await
        .unwrap();
    subscriber
        .deliver(&an_attention_notification())
        .await
        .unwrap();

    // Then
    let recorded = sender.recorded();
    assert_eq!(recorded.len(), 1, "got {recorded:?}");
}

#[tokio::test]
async fn sends_again_once_the_session_has_moved_on_and_come_back() {
    // Given
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );
    let waiting = an_attention_notification();
    let done = SessionNotification {
        text: format!("✅ Session {LABEL}: Claude Code finished this turn."),
        ..an_attention_notification()
    };

    // When — waiting, then done, then waiting again
    subscriber.deliver(&waiting).await.unwrap();
    subscriber.deliver(&done).await.unwrap();
    subscriber.deliver(&waiting).await.unwrap();

    // Then — three genuine changes, three messages
    assert_eq!(sender.recorded().len(), 3);
}

#[tokio::test]
async fn keeps_two_sessions_dedupe_state_apart() {
    // Given
    let (subscriber, sender) = a_telegram_subscriber(
        a_config_broadcasting_to_one_chat(),
        no_chat_tracks_the_session(),
    );
    let other = SessionNotification {
        session_id: "01900000-0000-7000-8000-AABB00000002".to_string(),
        label: "other-branch".to_string(),
        ..an_attention_notification()
    };

    // When — the same wording for two different sessions
    subscriber
        .deliver(&an_attention_notification())
        .await
        .unwrap();
    subscriber.deliver(&other).await.unwrap();

    // Then — one message each; a shared dedupe key would have swallowed the second
    assert_eq!(sender.recorded().len(), 2);
}
