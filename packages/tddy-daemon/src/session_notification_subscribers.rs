//! The subscribers that ship with the daemon: Telegram, and the stream `tddy-web` reads.
//!
//! PRD: `docs/ft/daemon/session-notifications.md` (FR2, FR3, FR7).

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::session_notifications::{
    LastDeliveredPerSession, SessionNotification, SessionNotificationKind,
    SessionNotificationSource, SessionNotificationSubscriber,
};
use crate::telegram_session_subscriber::TelegramDaemonHooks;

/// How many notifications a client that has stopped reading may fall behind before its oldest are
/// dropped. Sized for a burst of tool-call activity across a drawer of sessions while a tab is
/// backgrounded; a client that overruns it loses the oldest events, which for an indicator is the
/// right loss — the newest event is the one the dot is derived from.
const NOTIFICATION_STREAM_CAPACITY: usize = 256;

/// Telegram's interest in the notification bus.
///
/// It takes attention-worthy events from the activity-status path and nothing else. Two exclusions
/// carry the design (PRD FR7):
///
/// - **`Activity` is declined.** The indicator feed is why that kind exists; sending it to a chat
///   would turn every tool call into a message.
/// - **`Presenter` is declined.** A workflow session's elicitations already reach Telegram through
///   [`crate::telegram_notifier`], with their inline keyboards, per-chat FIFO and tracked-session
///   gate. Taking them here as well would send each one twice.
pub struct TelegramNotificationSubscriber {
    telegram: Arc<TelegramDaemonHooks>,
    /// Suppresses a status that was re-reported without changing. The bus publishes every report
    /// so an indicator can stay alive; a chat must not receive the repeats.
    last_delivered: LastDeliveredPerSession,
}

impl TelegramNotificationSubscriber {
    pub fn new(telegram: Arc<TelegramDaemonHooks>) -> Self {
        Self {
            telegram,
            last_delivered: LastDeliveredPerSession::default(),
        }
    }

    /// The configured broadcast recipients, or empty when Telegram is absent or disabled.
    fn broadcast_chat_ids(&self) -> Vec<i64> {
        self.telegram
            .config
            .telegram
            .as_ref()
            .filter(|tg| tg.enabled)
            .map(|tg| tg.chat_ids.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl SessionNotificationSubscriber for TelegramNotificationSubscriber {
    fn name(&self) -> &'static str {
        "telegram"
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        let enabled = self
            .telegram
            .config
            .telegram
            .as_ref()
            .is_some_and(|tg| tg.enabled);

        enabled
            && notification.kind == SessionNotificationKind::AttentionRequired
            && notification.source == SessionNotificationSource::ActivityStatus
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        if !self
            .last_delivered
            .record_and_is_new(&notification.session_id, &notification.text)
        {
            log::debug!(
                target: "tddy_daemon::telegram",
                "session notification unchanged — no alert session_id={}",
                notification.session_id
            );
            return Ok(());
        }

        // Chats explicitly tracking this session take priority: a session started or entered from
        // Telegram routes only to that operator (the same targeting the workflow keyboards use).
        // When no chat tracks it — a claude-cli session started from the web UI or `claude`
        // directly — fall back to the daemon's configured broadcast list, so activity alerts still
        // reach operators.
        let tracked_chat_ids = {
            let watcher = self.telegram.watcher.lock().await;
            watcher.chats_tracking_session(&notification.session_id)
        };
        // `None` means the tracking map could not be read, which is not the same as "nobody claimed
        // this session". Broadcasting on an unknown would announce a session one operator had
        // claimed to every configured chat, so an unreadable map sends nothing at all.
        let Some(tracked_chat_ids) = tracked_chat_ids else {
            log::error!(
                target: "tddy_daemon::telegram",
                "tracked chats unreadable for session_id={} — no alert sent",
                notification.session_id
            );
            return Ok(());
        };
        let (chat_ids, routing) = if tracked_chat_ids.is_empty() {
            (self.broadcast_chat_ids(), "configured_broadcast")
        } else {
            (tracked_chat_ids, "tracked")
        };

        if chat_ids.is_empty() {
            log::debug!(
                target: "tddy_daemon::telegram",
                "no chats tracking session_id={} and no configured chat_ids — no alert",
                notification.session_id
            );
            return Ok(());
        }

        log::info!(
            target: "tddy_daemon::telegram",
            "sending session notification session_id={} routing={} chats={}",
            notification.session_id,
            routing,
            chat_ids.len()
        );

        // The bus owns the copy: the text is sent exactly as it was published, so a chat message
        // and the drawer's tooltip read the same sentence.
        let mut failed = 0_usize;
        let total = chat_ids.len();
        for chat_id in chat_ids {
            if let Err(e) = self
                .telegram
                .sender
                .send_message(chat_id, &notification.text)
                .await
            {
                failed += 1;
                log::warn!(
                    target: "tddy_daemon::telegram",
                    "send_message failed chat_id={} session_id={}: {e:#}",
                    chat_id,
                    notification.session_id
                );
            }
        }

        if failed > 0 {
            // Reported without the underlying error text, which is already logged above and can
            // carry request details a notification path has no business repeating.
            anyhow::bail!("telegram send failed for {failed} of {total} chat(s)");
        }
        Ok(())
    }
}

/// The subscriber behind `ConnectionService.StreamSessionNotifications`.
///
/// One broadcast channel carries every session on the daemon, so a drawer of any size pays for one
/// subscription (PRD NFR1), and every connected client gets its own copy of each event.
pub struct SessionNotificationStreamSubscriber {
    tx: broadcast::Sender<SessionNotification>,
}

impl Default for SessionNotificationStreamSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionNotificationStreamSubscriber {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(NOTIFICATION_STREAM_CAPACITY);
        Self { tx }
    }

    /// A receiver for one client's stream. Events published before this call are not replayed:
    /// notifications describe moments, and a dot derived from a replayed one would claim an agent
    /// is working now because it was working when the tab was last open.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionNotification> {
        self.tx.subscribe()
    }
}

#[async_trait]
impl SessionNotificationSubscriber for SessionNotificationStreamSubscriber {
    fn name(&self) -> &'static str {
        "notification-stream"
    }

    fn wants(&self, _notification: &SessionNotification) -> bool {
        true
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        // A send with nobody subscribed is the ordinary state of a daemon with no browser
        // attached, not a delivery failure.
        if self.tx.send(notification.clone()).is_err() {
            log::debug!(
                target: "tddy_daemon::session_notifications",
                "no notification stream client attached for session {}",
                notification.session_id
            );
        }
        Ok(())
    }

    fn client_relay(&self) -> Option<broadcast::Receiver<SessionNotification>> {
        Some(self.subscribe())
    }
}
