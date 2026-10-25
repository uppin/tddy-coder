//! Unit tests: one agent notification as the Telegram HTML messages that carry it
//! (`AgentNotificationTelegram::render`, `#home-inbox` 1/8).

use tddy_daemon_kernel::session_notification::{
    AgentNotification, NotificationLevel, SessionNotification, SessionNotificationKind,
    SessionNotificationSource,
};
use tddy_telegram_control::agent_notification_telegram::AgentNotificationTelegram;

const LINK: &str = "#/sessions/s-1?host=host-a";

fn an_agent_notification(title: &str, body: &str, sound: bool, label: &str) -> SessionNotification {
    SessionNotification {
        session_id: "s-1".to_string(),
        os_user: "ada".to_string(),
        label: label.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::Agent,
        text: title.to_string(),
        agent: Some(AgentNotification {
            title: title.to_string(),
            body_markdown: body.to_string(),
            level: NotificationLevel::Info,
            sound,
            link: LINK.to_string(),
        }),
        at_unix_ms: 1,
    }
}

fn rendered(notification: &SessionNotification) -> AgentNotificationTelegram {
    AgentNotificationTelegram::render(notification).expect("an agent notification renders")
}

#[test]
fn a_short_notification_is_one_message_in_the_fixed_layout() {
    let telegram = rendered(&an_agent_notification(
        "Done",
        "all *green*",
        true,
        "feat-x",
    ));

    assert_eq!(
        telegram.messages(),
        [format!(
            "<b>Done</b>\n<i>feat-x</i>\n\nall <i>green</i>\n\n{LINK}"
        )]
    );
}

#[test]
fn a_title_alone_leaves_no_empty_body_section() {
    let telegram = rendered(&an_agent_notification("Done", "", true, "feat-x"));

    assert_eq!(
        telegram.messages(),
        [format!("<b>Done</b>\n<i>feat-x</i>\n\n{LINK}")]
    );
}

#[test]
fn the_title_and_label_are_escaped_as_text() {
    // They are the agent's and the worktree's words, not markup
    let telegram = rendered(&an_agent_notification("A & <B>", "", true, "x<y>"));

    assert_eq!(
        telegram.messages(),
        [format!(
            "<b>A &amp; &lt;B&gt;</b>\n<i>x&lt;y&gt;</i>\n\n{LINK}"
        )]
    );
}

#[test]
fn it_is_silent_exactly_when_the_agent_asked_for_no_sound() {
    let with_sound = rendered(&an_agent_notification("Done", "", true, "feat-x"));
    let without = rendered(&an_agent_notification("Done", "", false, "feat-x"));

    assert_eq!((with_sound.silent(), without.silent()), (false, true));
}
