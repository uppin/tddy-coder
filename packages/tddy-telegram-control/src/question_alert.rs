//! What a claude-cli session is asking, as the one Telegram message that tells the operator
//! (`#home-inbox` 3/8).
//!
//! A permission names the tool and its input summary; a question gives its text and lists its
//! options. The message is HTML in Telegram's tag subset, so every piece of the session's own
//! words is escaped, and a long question set is split with the same well-formed-HTML splitter an
//! agent notification uses, so no message passes Telegram's 4,096-byte limit.
//!
//! A module of its own rather than more of `telegram_notifier.rs` or
//! `telegram_session_subscriber.rs`: both are over budget, and rendering is a pure function of the
//! notification.

use crate::agent_notification_telegram::{split_html, TELEGRAM_HTML_MESSAGE_MAX_BYTES};
use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, SessionNotification,
};

/// The HTML one pending question is sent as, whole and as the messages that carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionAlert {
    html: String,
    messages: Vec<String>,
}

impl QuestionAlert {
    /// The alert for `notification`, or `None` when it asks no question.
    #[must_use]
    pub fn render(notification: &SessionNotification) -> Option<Self> {
        let question = notification.question.as_ref()?;
        let label = escape_text(&notification.label);
        let html = if question.kind == "permission" {
            permission_html(&label, question)
        } else {
            question_html(&label, question)
        };
        let messages = split_html(&html, TELEGRAM_HTML_MESSAGE_MAX_BYTES);
        Some(Self { html, messages })
    }

    /// The whole alert, in Telegram's HTML parse mode, before it is split.
    #[must_use]
    pub fn html(&self) -> &str {
        &self.html
    }

    /// The messages the alert is sent as, in order: each at most 4,096 bytes and well-formed on
    /// its own.
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

/// `Session {label} wants to run {tool}: {summary}`, with the tool and its summary in code.
fn permission_html(label: &str, question: &QuestionNotice) -> String {
    let mut html = format!(
        "<i>Session {label} wants to run</i> <code>{}</code>",
        escape_text(&question.tool_name)
    );
    if !question.summary.is_empty() {
        html.push_str(&format!(
            ": <code>{}</code>",
            escape_text(&question.summary)
        ));
    }
    html
}

/// `Session {label} asks`, then each question with its options, every block after a blank line.
fn question_html(label: &str, question: &QuestionNotice) -> String {
    let mut blocks = vec![format!("<i>Session {label} asks</i>")];
    for item in &question.questions {
        blocks.push(question_block(item));
        blocks.push(options_block(item));
    }
    blocks
        .into_iter()
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The header in bold (when there is one) over the question text, with a note when several options
/// may be chosen.
fn question_block(item: &QuestionNoticeItem) -> String {
    let mut text = escape_text(&item.question);
    if item.multi_select {
        text.push_str(" (choose several)");
    }
    if item.header.is_empty() {
        text
    } else {
        format!("<b>{}</b>\n{text}", escape_text(&item.header))
    }
}

/// One `• label: description` line per option, the description omitted when there is none.
fn options_block(item: &QuestionNoticeItem) -> String {
    item.options
        .iter()
        .map(|option| {
            if option.description.is_empty() {
                format!("• {}", escape_text(&option.label))
            } else {
                format!(
                    "• {}: {}",
                    escape_text(&option.label),
                    escape_text(&option.description)
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The three characters Telegram's HTML mode reads as markup. Quotes are left as they are: they
/// are only markup inside an attribute, and the message has none.
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
