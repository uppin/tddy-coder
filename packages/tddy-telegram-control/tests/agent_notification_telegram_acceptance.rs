//! An agent-written notification reaching Telegram: rendered from markdown to Telegram's HTML
//! subset, split to fit its 4,096-byte limit, sent silently unless the agent asked for a sound, and
//! never de-duplicated.
//!
//! Feature doc `docs/ft/daemon/session-notifications.md` § Agent-written notifications (`#home-inbox` 1/8). The path
//! under test is the real one from the bus onwards: the notification is published onto a
//! `SessionNotificationBus` that has Telegram's operator channel registered, and the assertions read
//! what an in-memory Bot API was asked to send.

use std::sync::{Arc, Mutex as StdMutex};

use tddy_daemon_kernel::config::{DaemonConfig, TelegramConfig};
use tddy_daemon_kernel::session_notification::{
    AgentNotification, NotificationLevel, SessionNotification, SessionNotificationKind,
    SessionNotificationSource,
};
use tddy_session_activity::session_notifications::{
    OperatorChannelSubscriber, SessionNotificationBus,
};
use tddy_telegram::active_elicitation::ActiveElicitationCoordinator;
use tddy_telegram::sender::RecordedHtmlMessage;
use tddy_telegram::telegram_tracked_session::TelegramTrackedSessionCoordinator;
use tddy_telegram_control::agent_notification_telegram::TELEGRAM_HTML_MESSAGE_MAX_BYTES;
use tddy_telegram_control::telegram_notifier::{
    InMemoryTelegramSender, TelegramSender, TelegramSessionWatcher,
};
use tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000301";
const LABEL: &str = "my-feature-branch";
const BROADCAST_CHAT: i64 = 222;
const LINK: &str = "#/sessions/01900000-0000-7000-8000-AABB00000301?host=host-a";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

struct AgentNotificationBuilder {
    agent: AgentNotification,
}

fn an_agent_notification() -> AgentNotificationBuilder {
    AgentNotificationBuilder {
        agent: AgentNotification {
            title: "Parallel run finished".to_string(),
            body_markdown: "4 done, **1 blocked**".to_string(),
            level: NotificationLevel::Success,
            sound: true,
            link: LINK.to_string(),
        },
    }
}

impl AgentNotificationBuilder {
    fn with_body(mut self, body_markdown: &str) -> Self {
        self.agent.body_markdown = body_markdown.to_string();
        self
    }

    fn without_sound(mut self) -> Self {
        self.agent.sound = false;
        self
    }

    /// The notification `ReportOperatorNotification` publishes for this agent content.
    fn build(self) -> SessionNotification {
        SessionNotification {
            session_id: SESSION_ID.to_string(),
            os_user: "ada".to_string(),
            label: LABEL.to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::Agent,
            text: self.agent.title.clone(),
            agent: Some(self.agent),
            at_unix_ms: 1_761_000_000_000,
            question: None,
        }
    }
}

/// A daemon whose bus has Telegram's operator channel on it, broadcasting to one configured chat —
/// no chat tracks the session, so the configured list is who receives it.
struct ADaemonWithTelegram {
    bus: SessionNotificationBus,
    bot_api: Arc<InMemoryTelegramSender>,
}

impl ADaemonWithTelegram {
    fn broadcasting_to_one_chat() -> Self {
        let config = DaemonConfig {
            telegram: Some(TelegramConfig {
                enabled: true,
                bot_token: "test-token".to_string(),
                chat_ids: vec![BROADCAST_CHAT],
            }),
            ..Default::default()
        };
        let bot_api = Arc::new(InMemoryTelegramSender::new());
        let watcher =
            TelegramSessionWatcher::with_elicitation_select_options_coordinator_and_tracked(
                Arc::new(StdMutex::new(std::collections::HashMap::new())),
                Arc::new(StdMutex::new(ActiveElicitationCoordinator::new())),
                Arc::new(StdMutex::new(TelegramTrackedSessionCoordinator::new())),
            );
        let channel = Arc::new(TelegramDaemonHooks::new(
            config,
            Arc::clone(&bot_api) as Arc<dyn TelegramSender + Send + Sync>,
            Arc::new(tokio::sync::Mutex::new(watcher)),
        ));
        let bus = SessionNotificationBus::new()
            .with_subscriber(Arc::new(OperatorChannelSubscriber::new(channel)));
        Self { bus, bot_api }
    }

    async fn publish(&self, notification: SessionNotification) {
        self.bus.publish(notification).await;
    }

    fn html_sent(&self) -> Vec<RecordedHtmlMessage> {
        self.bot_api.recorded_html()
    }

    fn plain_text_sent(&self) -> Vec<(i64, String)> {
        self.bot_api.recorded()
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// The tags Telegram's HTML parse mode accepts that this rendering emits.
const ALLOWED_TAGS: [&str; 5] = ["b", "i", "code", "pre", "a"];

trait TelegramHtmlAssertions {
    /// Within the Bot API's size limit, and parseable on its own: every `<` opens or closes one of
    /// [`ALLOWED_TAGS`], and every tag opened is closed, in order, within this same message.
    fn assert_well_formed_on_its_own(&self) -> &Self;
}

impl TelegramHtmlAssertions for RecordedHtmlMessage {
    fn assert_well_formed_on_its_own(&self) -> &Self {
        let html = &self.html;
        assert!(
            html.len() <= TELEGRAM_HTML_MESSAGE_MAX_BYTES,
            "a message of {} bytes is over Telegram's {TELEGRAM_HTML_MESSAGE_MAX_BYTES}: {html:?}",
            html.len()
        );
        let mut open: Vec<String> = Vec::new();
        let mut rest = html.as_str();
        while let Some(start) = rest.find('<') {
            let after = &rest[start + 1..];
            let end = after
                .find('>')
                .unwrap_or_else(|| panic!("an unterminated tag in {html:?}"));
            let tag = &after[..end];
            let closing = tag.strip_prefix('/');
            let name = closing
                .unwrap_or(tag)
                .split_whitespace()
                .next()
                .unwrap_or_default();
            assert!(
                ALLOWED_TAGS.contains(&name),
                "<{tag}> is not a tag Telegram's HTML mode accepts, in {html:?}"
            );
            match closing {
                Some(_) => assert_eq!(
                    open.pop().as_deref(),
                    Some(name),
                    "</{name}> closes a tag that is not the innermost open one, in {html:?}"
                ),
                None => open.push(name.to_string()),
            }
            rest = &after[end + 1..];
        }
        assert!(
            open.is_empty(),
            "the message leaves {open:?} open, in {html:?}"
        );
        self
    }
}

/// A body of `steps` bold markdown spans, each a few multi-byte characters long — within the
/// 2,000-character body limit, but more than 4,096 bytes once rendered.
fn a_long_multibyte_body(steps: usize) -> String {
    "**🚀🚀🚀🚀🚀 done** ".repeat(steps)
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_notification_reaches_its_chats_as_html_with_the_title_bold_and_the_link_last() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();

    // When an agent's notification is published
    daemon.publish(an_agent_notification().build()).await;

    // Then the chat receives one HTML message: the title in bold, the session's label, the body
    // with its markdown rendered, and the session link last
    assert_eq!(
        daemon.html_sent(),
        vec![RecordedHtmlMessage {
            chat_id: BROADCAST_CHAT,
            html: format!(
                "<b>Parallel run finished</b>\n<i>{LABEL}</i>\n\n4 done, <b>1 blocked</b>\n\n{LINK}"
            ),
            silent: false,
        }]
    );
    // And no plain-text copy of it was sent besides
    assert_eq!(daemon.plain_text_sent(), Vec::<(i64, String)>::new());
}

#[tokio::test]
async fn a_body_over_4096_bytes_arrives_in_several_html_messages_each_well_formed() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();
    let steps = 130;

    // When an agent's notification carries a body whose rendering is over Telegram's limit
    daemon
        .publish(
            an_agent_notification()
                .with_body(&a_long_multibyte_body(steps))
                .build(),
        )
        .await;

    // Then it arrives as more than one message — how many is the splitter's choice; what is fixed
    // is that one message could not have carried it
    let sent = daemon.html_sent();
    assert!(
        sent.len() > 1,
        "a body over {TELEGRAM_HTML_MESSAGE_MAX_BYTES} bytes must be split, got {} message(s)",
        sent.len()
    );
    // And every one of them is within the limit and well-formed on its own
    for message in &sent {
        message.assert_well_formed_on_its_own();
    }
    // And together they carry the whole body: none of it was cut away
    let rockets: usize = sent.iter().map(|m| m.html.matches('🚀').count()).sum();
    assert_eq!(rockets, steps * 5);
    // And the title opens the first message and the link closes the last
    assert!(
        sent[0].html.starts_with("<b>Parallel run finished</b>\n"),
        "the first message opens with the title: {:?}",
        sent[0].html
    );
    let last = &sent[sent.len() - 1].html;
    assert!(
        last.ends_with(&format!("\n{LINK}")),
        "the last message ends with the link: {last:?}"
    );
}

#[tokio::test]
async fn without_sound_the_message_is_sent_silently() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();

    // When an agent's notification asks for no sound
    daemon
        .publish(an_agent_notification().without_sound().build())
        .await;

    // Then the chat receives it with notifications disabled
    let silences: Vec<bool> = daemon.html_sent().iter().map(|m| m.silent).collect();
    assert_eq!(silences, vec![true]);
}

#[tokio::test]
async fn two_identical_agent_notifications_are_both_delivered() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();

    // When an agent sends the same notification twice
    daemon.publish(an_agent_notification().build()).await;
    daemon.publish(an_agent_notification().build()).await;

    // Then the chat receives both: two identical messages are two events, not a repeated status
    let chats: Vec<i64> = daemon.html_sent().iter().map(|m| m.chat_id).collect();
    assert_eq!(chats, vec![BROADCAST_CHAT, BROADCAST_CHAT]);
}
