//! What a claude-cli session is asking, as the one Telegram message that tells the operator
//! (`#home-inbox` 3/8).
//!
//! A permission names the tool and its input summary; a question gives its text and lists its
//! options. The message is HTML in Telegram's tag subset, so every piece of the session's own
//! words is escaped.
//!
//! A module of its own rather than more of `telegram_notifier.rs` or
//! `telegram_session_subscriber.rs`: both are over budget, and rendering is a pure function of the
//! notification.

use tddy_daemon_kernel::session_notification::SessionNotification;

/// The HTML message one pending question is sent as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionAlert {
    html: String,
}

impl QuestionAlert {
    /// The alert for `notification`, or `None` when it asks no question.
    #[must_use]
    pub fn render(notification: &SessionNotification) -> Option<Self> {
        // TODO(elicit-content): implement
        let _ = notification;
        None
    }

    /// The message, in Telegram's HTML parse mode.
    #[must_use]
    pub fn html(&self) -> &str {
        &self.html
    }
}
