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
