//! A session notification on the wire carries what the bus carried (`#activity-wake` 1/3).

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::session_notification::SessionEndNotice;
use tddy_service::proto::activity::{
    SessionEndNotice as ProtoSessionEndNotice,
    SessionNotificationSource as ProtoSessionNotificationSource,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::streams::session_notification_event;

fn a_session_end(outcome: &str, exit_code: Option<i32>) -> SessionNotification {
    SessionNotification {
        session_id: "s-1".to_string(),
        os_user: "ada".to_string(),
        label: "fix-login".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::SessionEnded,
        text: format!("⏹ Session fix-login ended: {outcome}"),
        agent: None,
        question: None,
        ended: Some(SessionEndNotice {
            outcome: outcome.to_string(),
            exit_code,
        }),
        at_unix_ms: 1_790_000_000_000,
        activity_status: None,
        message: None,
    }
}

#[test]
fn a_session_ended_notification_converts_with_its_end() {
    // When a session end is put on the wire
    let event = session_notification_event(a_session_end("exit code 3", Some(3)));

    // Then it is a session-ended event carrying how the agent ended
    assert_eq!(
        (event.source, event.ended),
        (
            ProtoSessionNotificationSource::SessionEnded as i32,
            Some(ProtoSessionEndNotice {
                outcome: "exit code 3".to_string(),
                exit_code: Some(3),
            }),
        )
    );
}

#[test]
fn an_end_with_no_exit_code_has_none_on_the_wire() {
    // When an end nobody saw the code of is put on the wire
    let event = session_notification_event(a_session_end("exit code unknown", None));

    // Then its end has no exit code, rather than a zero
    assert_eq!(
        event.ended,
        Some(ProtoSessionEndNotice {
            outcome: "exit code unknown".to_string(),
            exit_code: None,
        })
    );
}

// ── messages between sessions never reach the web (`#activity-wake` 5/5) ──────────────────────

/// The child's message to its orchestrator, as `PostSessionMessage` publishes it.
fn a_message_between_sessions() -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::Message,
        text: "Tests pass; pushed 3f2a1c.".to_string(),
        ended: None,
        message: Some(tddy_daemon_kernel::session_notification::SessionMessage {
            to_session_id: "s-main".to_string(),
            message_id: "m-1".to_string(),
            reply_to: None,
            text: "Tests pass; pushed 3f2a1c.".to_string(),
        }),
        ..a_session_end("unused", None)
    }
}

#[tokio::test]
async fn the_web_stream_never_carries_a_message() {
    // Given a browser of the session's operator subscribed to the notification stream
    let (published, subscribed) = tokio::sync::broadcast::channel(8);
    let (to_the_browser, mut the_browser) = tokio::sync::mpsc::unbounded_channel();
    let relay = tokio::spawn(tddy_session_activity::streams::relay_session_notifications(
        subscribed,
        to_the_browser,
        "ada".to_string(),
    ));

    // When a message between sessions is published, and then a session ends
    published.send(a_message_between_sessions()).unwrap();
    published
        .send(a_session_end("exit code 0", Some(0)))
        .unwrap();
    drop(published);
    relay.await.expect("the relay ends with the bus");

    // Then the browser is sent the end alone
    let mut sources = Vec::new();
    while let Ok(event) = the_browser.try_recv() {
        sources.push(event.source);
    }
    assert_eq!(
        sources,
        vec![ProtoSessionNotificationSource::SessionEnded as i32]
    );
}

// ── a trunk commit is its orchestrator's own activity, never the web's (`#orchestra` 5/9) ─────

/// One new trunk commit, as the trunk watch publishes it for one subscribed orchestrator.
fn a_trunk_commit() -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::TrunkCommit,
        kind: SessionNotificationKind::Activity,
        text: "origin/main: 3f2a1c0 Add the outbox (dev)".to_string(),
        ended: None,
        ..a_session_end("unused", None)
    }
}

#[tokio::test]
async fn the_web_stream_never_carries_a_trunk_commit() {
    // Given a browser of the session's operator subscribed to the notification stream
    let (published, subscribed) = tokio::sync::broadcast::channel(8);
    let (to_the_browser, mut the_browser) = tokio::sync::mpsc::unbounded_channel();
    let relay = tokio::spawn(tddy_session_activity::streams::relay_session_notifications(
        subscribed,
        to_the_browser,
        "ada".to_string(),
    ));

    // When a trunk commit is published for an orchestrator, and then a session ends
    published.send(a_trunk_commit()).unwrap();
    published
        .send(a_session_end("exit code 0", Some(0)))
        .unwrap();
    drop(published);
    relay.await.expect("the relay ends with the bus");

    // Then the browser is sent the end alone
    let mut sources = Vec::new();
    while let Ok(event) = the_browser.try_recv() {
        sources.push(event.source);
    }
    assert_eq!(
        sources,
        vec![ProtoSessionNotificationSource::SessionEnded as i32]
    );
}
