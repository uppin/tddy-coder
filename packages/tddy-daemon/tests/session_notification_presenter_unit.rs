//! Unit: classifying a `tddy-coder` workflow session's presenter events into notifications.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md (FR5).
//!
//! This is the indicator story for **tool sessions**, which report no Claude Code hook statuses:
//! their dot is driven entirely by the `PresenterObserver.ObserveEvents` stream. The classification
//! is a pure mapping over `ServerMessage`, so it is pinned here as a table of cases.
//!
//! Every notification produced here is `Presenter`-sourced, which the Telegram subscriber declines
//! (`telegram_notification_subscriber_unit.rs`): a workflow session's elicitations already reach
//! Telegram through `telegram_notifier`'s keyboard-bearing surface. These events exist to move a
//! dot, not to send a message.

use tddy_daemon::session_notifications::{
    notification_for_presenter_event, SessionNotificationKind, SessionNotificationSource,
};
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{
    app_mode_proto, AppModeProto, AppModeRunning, AppModeSelect, BackendSelected, GoalStarted,
    ModeChanged, ServerMessage, StateChanged,
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

fn a_message(event: Event) -> ServerMessage {
    ServerMessage { event: Some(event) }
}

/// A clarification the operator has to answer — the presenter is blocked until they do.
fn a_clarification_prompt() -> ServerMessage {
    a_message(Event::ModeChanged(ModeChanged {
        mode: Some(AppModeProto {
            variant: Some(app_mode_proto::Variant::Select(AppModeSelect {
                question: None,
                question_index: 0,
                total_questions: 1,
                initial_selected: 0,
            })),
        }),
    }))
}

/// The presenter working on its own — nobody is being asked anything.
fn an_autonomous_running_mode() -> ServerMessage {
    a_message(Event::ModeChanged(ModeChanged {
        mode: Some(AppModeProto {
            variant: Some(app_mode_proto::Variant::Running(AppModeRunning {})),
        }),
    }))
}

// ---------------------------------------------------------------------------
// Attention
// ---------------------------------------------------------------------------

#[test]
fn classifies_a_clarification_prompt_as_attention_required() {
    // Given / When
    let notification =
        notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &a_clarification_prompt(), AT)
            .expect("an elicitation mode must produce a notification");

    // Then
    assert_eq!(
        notification.kind,
        SessionNotificationKind::AttentionRequired
    );
    assert_eq!(notification.source, SessionNotificationSource::Presenter);
    assert_eq!(notification.session_id, SESSION_ID);
    assert_eq!(notification.label, LABEL);
    assert_eq!(notification.at_unix_ms, AT);
    assert_eq!(
        notification.text,
        "🔔 Session my-feature-branch: waiting for your answer."
    );
}

/// The dot must not go yellow because the presenter changed screens on its own.
#[test]
fn raises_no_notification_for_an_autonomous_mode_change() {
    // Given / When
    let notification = notification_for_presenter_event(
        SESSION_ID,
        OS_USER,
        LABEL,
        &an_autonomous_running_mode(),
        AT,
    );

    // Then
    assert_eq!(notification, None);
}

// ---------------------------------------------------------------------------
// Activity
// ---------------------------------------------------------------------------

#[test]
fn classifies_a_workflow_state_change_as_activity() {
    // Given
    let message = a_message(Event::StateChanged(StateChanged {
        from: "Red".to_string(),
        to: "Green".to_string(),
    }));

    // When
    let notification = notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &message, AT)
        .expect("a state change must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(notification.source, SessionNotificationSource::Presenter);
    assert_eq!(notification.text, "Session my-feature-branch: Red -> Green");
}

#[test]
fn classifies_a_started_goal_as_activity() {
    // Given
    let message = a_message(Event::GoalStarted(GoalStarted {
        goal: "acceptance-tests".to_string(),
    }));

    // When
    let notification = notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &message, AT)
        .expect("a started goal must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.text,
        "Session my-feature-branch: goal started: acceptance-tests"
    );
}

#[test]
fn classifies_a_selected_backend_as_activity() {
    // Given
    let message = a_message(Event::BackendSelected(BackendSelected {
        agent: "claude".to_string(),
        model: "opus".to_string(),
    }));

    // When
    let notification = notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &message, AT)
        .expect("a selected backend must produce a notification");

    // Then
    assert_eq!(notification.kind, SessionNotificationKind::Activity);
    assert_eq!(
        notification.text,
        "Session my-feature-branch: using claude (opus)"
    );
}

// ---------------------------------------------------------------------------
// Nothing to say
// ---------------------------------------------------------------------------

#[test]
fn raises_no_notification_for_a_message_carrying_no_event() {
    // Given — the stream can deliver an envelope whose event field is unset
    let message = ServerMessage { event: None };

    // When
    let notification = notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &message, AT);

    // Then
    assert_eq!(notification, None);
}
