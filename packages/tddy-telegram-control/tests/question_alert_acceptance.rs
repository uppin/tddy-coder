//! What a claude-cli session is asking, as Telegram tells the operator: a permission names the tool
//! and its input, a question lists its options, each is sent once per request, and the generic
//! "needs your input" alert does not follow for the same prompt (`#home-inbox` 3/8).
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-elicitation-content.md`. The path under test is the real
//! one from the bus onwards: notifications are published onto a `SessionNotificationBus` that has
//! Telegram's operator channel registered, and the assertions read what an in-memory Bot API was
//! asked to send.

use std::sync::{Arc, Mutex as StdMutex};

use tddy_daemon_kernel::config::{DaemonConfig, TelegramConfig};
use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, QuestionNoticeOption, SessionNotification,
    SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_notifications::{
    OperatorChannelSubscriber, SessionNotificationBus,
};
use tddy_telegram::active_elicitation::ActiveElicitationCoordinator;
use tddy_telegram::sender::RecordedHtmlMessage;
use tddy_telegram::telegram_tracked_session::TelegramTrackedSessionCoordinator;
use tddy_telegram_control::telegram_notifier::{
    InMemoryTelegramSender, TelegramSender, TelegramSessionWatcher,
};
use tddy_telegram_control::telegram_session_subscriber::TelegramDaemonHooks;

const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000601";
const LABEL: &str = "my-feature-branch";
const BROADCAST_CHAT: i64 = 222;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A `PendingQuestion` notification, as `ReportSessionStatus` publishes it for a session of
/// [`SESSION_ID`]. Bare, it is a permission to run `cargo publish`.
struct QuestionNotificationBuilder {
    notification: SessionNotification,
}

fn a_permission_request() -> QuestionNotificationBuilder {
    QuestionNotificationBuilder {
        notification: SessionNotification {
            session_id: SESSION_ID.to_string(),
            os_user: "ada".to_string(),
            label: LABEL.to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::PendingQuestion,
            text: format!("Session {LABEL} asks for a permission"),
            agent: None,
            question: Some(QuestionNotice {
                request_id: "request-1".to_string(),
                kind: "permission".to_string(),
                tool_name: "Bash".to_string(),
                summary: "cargo publish".to_string(),
                questions: Vec::new(),
            }),
            at_unix_ms: 1_761_000_000_000,
        },
    }
}

fn a_question_about_the_language() -> QuestionNotificationBuilder {
    a_permission_request().with_question(QuestionNotice {
        request_id: "request-3".to_string(),
        kind: "select".to_string(),
        tool_name: "AskUserQuestion".to_string(),
        summary: "Which language should the service use?".to_string(),
        questions: vec![QuestionNoticeItem {
            header: "Language".to_string(),
            question: "Which language should the service use?".to_string(),
            options: vec![
                an_option("Rust", "Memory safe, fast"),
                an_option("Go", "Simple to deploy"),
            ],
            multi_select: false,
        }],
    })
}

fn an_option(label: &str, description: &str) -> QuestionNoticeOption {
    QuestionNoticeOption {
        label: label.to_string(),
        description: description.to_string(),
    }
}

impl QuestionNotificationBuilder {
    fn with_question(mut self, question: QuestionNotice) -> Self {
        self.notification.question = Some(question);
        self
    }

    /// A different request: another id, another command.
    fn with_id_and_command(mut self, request_id: &str, command: &str) -> Self {
        let question = self
            .notification
            .question
            .as_mut()
            .expect("a permission request carries its question");
        question.request_id = request_id.to_string();
        question.summary = command.to_string();
        self
    }

    /// The `ActivityStatus` notification the follow-up `Notification` hook raises for the same
    /// prompt: the generic sentence, carrying the question it belongs to.
    fn followed_by_its_needs_input_status(self) -> SessionNotification {
        SessionNotification {
            source: SessionNotificationSource::ActivityStatus,
            text: format!(
                "🔔 Session {LABEL}: Claude Code needs your input (permission, question, or your next prompt)."
            ),
            ..self.notification
        }
    }

    fn build(self) -> SessionNotification {
        self.notification
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

/// The message the chat is expected to receive: loud, because the session is blocked on its
/// operator.
fn an_alert_reading(html: &str) -> RecordedHtmlMessage {
    RecordedHtmlMessage {
        chat_id: BROADCAST_CHAT,
        html: html.to_string(),
        silent: false,
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_permission_alert_names_the_tool_and_its_input_summary_once_per_request() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();

    // When a permission to run a command is published, re-published unchanged as the bus does to
    // keep an indicator alive, and then another request follows
    daemon
        .publish(
            a_permission_request()
                .with_id_and_command("request-1", "cargo build && cargo test")
                .build(),
        )
        .await;
    daemon
        .publish(
            a_permission_request()
                .with_id_and_command("request-1", "cargo build && cargo test")
                .build(),
        )
        .await;
    daemon
        .publish(
            a_permission_request()
                .with_id_and_command("request-2", "cargo publish")
                .build(),
        )
        .await;

    // Then the chat receives one alert per request, each naming the session, the tool and the
    // input summary — escaped, since it is the session's own text inside Telegram's HTML
    assert_eq!(
        daemon.html_sent(),
        vec![
            an_alert_reading(&format!(
                "<i>Session {LABEL} wants to run</i> <code>Bash</code>: <code>cargo build &amp;&amp; cargo test</code>"
            )),
            an_alert_reading(&format!(
                "<i>Session {LABEL} wants to run</i> <code>Bash</code>: <code>cargo publish</code>"
            )),
        ]
    );
    // And none of it was sent as plain text
    assert_eq!(daemon.plain_text_sent(), Vec::<(i64, String)>::new());
}

#[tokio::test]
async fn a_question_alert_lists_the_question_and_its_options() {
    // Given a daemon with Telegram on its bus
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();

    // When a session asks which language to use, offering two options
    daemon
        .publish(a_question_about_the_language().build())
        .await;

    // Then the chat receives the question under its header, and each option with its description
    assert_eq!(
        daemon.html_sent(),
        vec![an_alert_reading(&format!(
            "<i>Session {LABEL} asks</i>\n\n<b>Language</b>\nWhich language should the service use?\n\n• Rust: Memory safe, fast\n• Go: Simple to deploy"
        ))]
    );
    // And none of it was sent as plain text
    assert_eq!(daemon.plain_text_sent(), Vec::<(i64, String)>::new());
}

#[tokio::test]
async fn no_needs_your_input_alert_follows_while_the_same_prompt_is_pending() {
    // Given a daemon with Telegram on its bus, which has told its chat about a permission request
    let daemon = ADaemonWithTelegram::broadcasting_to_one_chat();
    daemon.publish(a_permission_request().build()).await;

    // When the Notification hook of the same prompt raises the generic status notification
    daemon
        .publish(a_permission_request().followed_by_its_needs_input_status())
        .await;

    // Then the chat has the one alert, and was not told a second time that the session needs input
    assert_eq!(
        daemon.html_sent(),
        vec![an_alert_reading(&format!(
            "<i>Session {LABEL} wants to run</i> <code>Bash</code>: <code>cargo publish</code>"
        ))]
    );
    assert_eq!(daemon.plain_text_sent(), Vec::<(i64, String)>::new());
}
