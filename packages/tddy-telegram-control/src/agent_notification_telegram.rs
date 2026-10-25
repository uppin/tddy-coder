//! An agent-written notification as the Telegram messages that carry it.
//!
//! The layout is fixed — the title in bold, the session label in italics, the markdown body
//! rendered to Telegram's HTML subset, then the session link — and split into as many messages as
//! Telegram's 4,096-byte limit needs, each of them well-formed HTML on its own, because the Bot API
//! refuses a message with an unclosed tag outright.
//!
//! A module of its own rather than more of `telegram_notifier.rs` or
//! `telegram_session_subscriber.rs`: both are over budget, and rendering is a pure function of the
//! notification with nothing to share with the presenter path's plain-text chunking.

use tddy_daemon_kernel::session_notification::SessionNotification;

/// The most a single Telegram message may carry, in bytes of UTF-8.
pub const TELEGRAM_HTML_MESSAGE_MAX_BYTES: usize = 4096;

/// The HTML messages one agent notification is sent as, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNotificationTelegram {
    messages: Vec<String>,
    silent: bool,
}

impl AgentNotificationTelegram {
    /// The messages `notification` is sent as, or `None` when it carries no agent-written content
    /// — a status notification is sent as its `text`, not rendered.
    #[must_use]
    pub fn render(notification: &SessionNotification) -> Option<Self> {
        // TODO(agent-notify): implement
        let _ = notification;
        None
    }

    /// The HTML messages, in send order; each at most [`TELEGRAM_HTML_MESSAGE_MAX_BYTES`].
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// Whether they are sent with `disable_notification` — the agent did not ask for a sound.
    #[must_use]
    pub fn silent(&self) -> bool {
        self.silent
    }
}
