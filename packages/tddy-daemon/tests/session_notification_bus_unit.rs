//! Unit: the session-notification domain — how a reported status becomes a notification, and how
//! the bus fans one publish out to its subscribers.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md (FR2, FR7, NFR3).
//!
//! Classification is a pure mapping over the `SessionActivityStatus` wire strings
//! (`packages/tddy-core/src/session_activity.rs`), so it is pinned here as a table of cases rather
//! than through the RPC. Fan-out is exercised against in-memory subscribers: the contract that
//! matters is *who is offered what*, which needs no daemon.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_daemon::session_notifications::{
    notification_for_activity_status, notification_for_agent_tool_call,
    RecordingSessionNotificationSubscriber, SessionNotification, SessionNotificationBus,
    SessionNotificationKind, SessionNotificationSource, SessionNotificationSubscriber,
};

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000001";
/// The OS user the session belongs to. Every notification names one: it is what the notification
/// stream scopes a subscribed client to.
const OS_USER: &str = "testuser";
const LABEL: &str = "my-feature-branch";
const AT: u64 = 1_756_000_000_000;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A notification with valid defaults — an activity event for the session under test.
fn a_notification() -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: OS_USER.to_string(),
        label: LABEL.to_string(),
        kind: SessionNotificationKind::Activity,
        source: SessionNotificationSource::ActivityStatus,
        text: format!("Session {LABEL}: agent is working"),
        at_unix_ms: AT,
    }
}

fn an_attention_notification() -> SessionNotification {
    SessionNotification {
        kind: SessionNotificationKind::AttentionRequired,
        ..a_notification()
    }
}

/// A subscriber whose delivery always fails — models a Telegram send hitting a network error.
struct FailingSubscriber;

#[async_trait]
impl SessionNotificationSubscriber for FailingSubscriber {
    fn name(&self) -> &'static str {
        "failing"
    }

    fn wants(&self, _notification: &SessionNotification) -> bool {
        true
    }

    async fn deliver(&self, _notification: &SessionNotification) -> anyhow::Result<()> {
        anyhow::bail!("delivery failed")
    }
}

// ---------------------------------------------------------------------------
// Classifying a reported activity status
// ---------------------------------------------------------------------------

#[test]
fn classifies_waiting_for_input_as_attention_required() {
    // Given / When
    let notification =
        notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "WaitingForInput", AT)
            .expect("WaitingForInput must produce a notification");

    // Then
    assert_eq!(
        notification.kind,
        SessionNotificationKind::AttentionRequired
    );
    assert_eq!(
        notification.source,
        SessionNotificationSource::ActivityStatus
    );
    assert_eq!(notification.session_id, SESSION_ID);
    assert_eq!(notification.label, LABEL);
    assert_eq!(notification.at_unix_ms, AT);
}

/// The copy an operator reads in Telegram today, unchanged but for the label (FR7).
#[test]
fn writes_the_waiting_for_input_notification_in_the_words_telegram_already_uses() {
    // Given / When
    let notification =
        notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "WaitingForInput", AT)
            .unwrap();

    // Then
    assert_eq!(
        notification.text,
        "🔔 Session my-feature-branch: Claude Code needs your input (permission, question, or your next prompt). Attach via the web UI or `tddy-tools pty-relay`."
    );
}

#[test]
fn classifies_done_as_attention_required() {
    // Given / When
    let notification = notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Done", AT)
        .expect("Done must produce a notification");

    // Then
    assert_eq!(
        notification.kind,
        SessionNotificationKind::AttentionRequired
    );
}

#[test]
fn writes_the_done_notification_in_the_words_telegram_already_uses() {
    // Given / When
    let notification =
        notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Done", AT).unwrap();

    // Then
    assert_eq!(
        notification.text,
        "✅ Session my-feature-branch: Claude Code finished this turn. Attach to continue."
    );
}

#[test]
fn classifies_a_running_status_as_activity() {
    // Given / When
    let notification = notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Running", AT)
        .expect("Running must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.text,
        "Session my-feature-branch: agent is working"
    );
}

#[test]
fn classifies_executing_tool_as_activity() {
    // Given / When
    let notification =
        notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "ExecutingTool", AT)
            .expect("ExecutingTool must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.text,
        "Session my-feature-branch: agent is running a tool"
    );
}

#[test]
fn classifies_a_started_session_as_activity() {
    // Given / When
    let notification = notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Started", AT)
        .expect("Started must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.text,
        "Session my-feature-branch: agent started"
    );
}

/// A session that has ended has nothing an operator needs to act on, and a dot derived from it is
/// grey on liveness alone — a notification here would only raise a blink on a dead row.
#[test]
fn raises_no_notification_for_a_session_that_ended() {
    // Given / When
    let notification = notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Ended", AT);

    // Then
    assert_eq!(notification, None);
}

#[test]
fn raises_no_notification_for_a_status_it_does_not_recognise() {
    // Given / When
    let notification =
        notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "Rebooting", AT);

    // Then
    assert_eq!(notification, None);
}

#[test]
fn raises_no_notification_for_an_empty_status() {
    // Given / When
    let notification = notification_for_activity_status(SESSION_ID, OS_USER, LABEL, "", AT);

    // Then
    assert_eq!(notification, None);
}

// ---------------------------------------------------------------------------
// Classifying the agent's own tool call
// ---------------------------------------------------------------------------

#[test]
fn describes_an_agent_tool_call_as_activity_naming_the_tool() {
    // Given / When
    let notification = notification_for_agent_tool_call(SESSION_ID, OS_USER, LABEL, "Bash", AT);

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.source,
        SessionNotificationSource::AgentToolCall
    );
    assert_eq!(notification.text, "Session my-feature-branch: Bash");
    assert_eq!(notification.at_unix_ms, AT);
}

// ---------------------------------------------------------------------------
// Fan-out
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delivers_a_notification_to_every_subscriber_that_wants_it() {
    // Given
    let first = Arc::new(RecordingSessionNotificationSubscriber::new());
    let second = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = SessionNotificationBus::new()
        .with_subscriber(Arc::clone(&first))
        .with_subscriber(Arc::clone(&second));

    // When
    bus.publish(an_attention_notification()).await;

    // Then
    assert_eq!(first.received(), vec![an_attention_notification()]);
    assert_eq!(second.received(), vec![an_attention_notification()]);
}

#[tokio::test]
async fn withholds_a_notification_from_a_subscriber_that_does_not_want_it() {
    // Given — one subscriber takes everything, one takes attention only
    let everything = Arc::new(RecordingSessionNotificationSubscriber::new());
    let attention_only = Arc::new(RecordingSessionNotificationSubscriber::wanting_only(
        SessionNotificationKind::AttentionRequired,
    ));
    let bus = SessionNotificationBus::new()
        .with_subscriber(Arc::clone(&everything))
        .with_subscriber(Arc::clone(&attention_only));

    // When — an activity notification is published
    bus.publish(a_notification()).await;

    // Then
    assert_eq!(everything.received(), vec![a_notification()]);
    assert_eq!(attention_only.received(), Vec::new());
}

#[tokio::test]
async fn delivers_each_notification_in_the_order_it_was_published() {
    // Given
    let subscriber = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&subscriber));

    // When
    bus.publish(a_notification()).await;
    bus.publish(an_attention_notification()).await;

    // Then
    assert_eq!(
        subscriber.received(),
        vec![a_notification(), an_attention_notification()]
    );
}

/// NFR3: a publish is a side effect of an RPC that must still return `ok`. A bus with nothing
/// listening is the ordinary state of a daemon with Telegram off and no browser attached.
#[tokio::test]
async fn publishing_to_a_bus_with_no_subscribers_is_a_no_op() {
    // Given
    let bus = SessionNotificationBus::new();

    // When / Then — the absence of a panic is the assertion; a recording subscriber added
    // afterwards proves the bus is still usable.
    bus.publish(a_notification()).await;

    let subscriber = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = bus.with_subscriber(Arc::clone(&subscriber));
    bus.publish(an_attention_notification()).await;

    assert_eq!(subscriber.received(), vec![an_attention_notification()]);
}

/// NFR3: one subscriber's failure — a Telegram send that times out — must not cost the indicator
/// subscriber its event.
#[tokio::test]
async fn keeps_delivering_to_the_remaining_subscribers_when_one_fails() {
    // Given — a failing subscriber registered ahead of a healthy one
    let healthy = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = SessionNotificationBus::new()
        .with_subscriber(Arc::new(FailingSubscriber))
        .with_subscriber(Arc::clone(&healthy));

    // When
    bus.publish(an_attention_notification()).await;

    // Then
    assert_eq!(healthy.received(), vec![an_attention_notification()]);
}

/// The recording double is the library's own test seam (as `InMemoryTelegramSender` is), so its
/// per-session isolation is worth stating: a spec that publishes for two sessions must be able to
/// tell their events apart.
#[tokio::test]
async fn records_the_session_each_notification_belongs_to() {
    // Given
    let subscriber = Arc::new(RecordingSessionNotificationSubscriber::new());
    let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&subscriber));

    // When
    bus.publish(a_notification()).await;
    bus.publish(SessionNotification {
        session_id: "01900000-0000-7000-8000-AABB00000002".to_string(),
        label: "other-branch".to_string(),
        ..a_notification()
    })
    .await;

    // Then
    let sessions: Vec<String> = subscriber
        .received()
        .into_iter()
        .map(|n| n.session_id)
        .collect();
    assert_eq!(
        sessions,
        vec![
            SESSION_ID.to_string(),
            "01900000-0000-7000-8000-AABB00000002".to_string()
        ]
    );
}
