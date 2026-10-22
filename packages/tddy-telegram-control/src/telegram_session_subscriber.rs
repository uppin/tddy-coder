//! Telegram as the daemon's operator channel: the shared hooks the daemon wires once, and the
//! [`OperatorChannel`] through which a session's presenter events and its attention-worthy
//! notifications reach a chat.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md`, change 1, and
//! `docs/ft/daemon/session-notifications.md` (FR2, FR3, FR7).
//!
//! Neither loop that feeds the channel is here. The presenter observer — connect, stream, publish
//! to the notification bus — runs whether or not Telegram is configured, so it lives in
//! `tddy_session_activity::presenter_observer_task`; the bus registers the channel through
//! `tddy_session_activity::session_notifications::OperatorChannelSubscriber`.
//!
//! The module keeps its name from before the observer loop left it; it subscribes to nothing
//! itself.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use tddy_daemon_kernel::operator_channel::OperatorChannel;
use tddy_daemon_kernel::session_notification::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};
use tddy_service::gen::ServerMessage;

use crate::telegram_notifier::{TelegramSender, TelegramSessionWatcher};
use tddy_session_lifecycle::config::DaemonConfig;

/// Shared handles for Telegram + watcher; installed as the daemon's operator channel, and used by
/// the Telegram-started spawn path the same way.
pub struct TelegramDaemonHooks {
    pub config: DaemonConfig,
    pub sender: Arc<dyn TelegramSender + Send + Sync>,
    pub watcher: Arc<Mutex<TelegramSessionWatcher>>,
}

impl TelegramDaemonHooks {
    /// Whether the daemon's `telegram:` block is present and switched on.
    fn enabled(&self) -> bool {
        self.config.telegram.as_ref().is_some_and(|tg| tg.enabled)
    }

    /// The configured broadcast recipients, or empty when Telegram is absent or disabled.
    fn broadcast_chat_ids(&self) -> Vec<i64> {
        self.config
            .telegram
            .as_ref()
            .filter(|tg| tg.enabled)
            .map(|tg| tg.chat_ids.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl OperatorChannel for TelegramDaemonHooks {
    fn id(&self) -> &'static str {
        tddy_telegram::status::TELEGRAM_INTEGRATION_ID
    }

    async fn on_presenter_event(
        &self,
        session_id: &str,
        event: &ServerMessage,
    ) -> anyhow::Result<()> {
        let mut guard = self.watcher.lock().await;
        guard
            .on_server_message(&self.config, self.sender.as_ref(), session_id, event)
            .await
    }

    /// Telegram's interest in the notification bus.
    ///
    /// It takes attention-worthy events from the activity-status path and nothing else. Two
    /// exclusions carry the design (PRD FR7):
    ///
    /// - **`Activity` is declined.** The indicator feed is why that kind exists; sending it to a
    ///   chat would turn every tool call into a message.
    /// - **`Presenter` is declined.** A workflow session's elicitations already reach Telegram
    ///   through [`Self::on_presenter_event`], with their inline keyboards, per-chat FIFO and
    ///   tracked-session gate. Taking them here as well would send each one twice.
    fn wants_notification(&self, notification: &SessionNotification) -> bool {
        self.enabled()
            && notification.kind == SessionNotificationKind::AttentionRequired
            && notification.source == SessionNotificationSource::ActivityStatus
    }

    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        // Chats explicitly tracking this session take priority: a session started or entered from
        // Telegram routes only to that operator (the same targeting the workflow keyboards use).
        // When no chat tracks it — a claude-cli session started from the web UI or `claude`
        // directly — fall back to the daemon's configured broadcast list, so activity alerts still
        // reach operators.
        let tracked_chat_ids = {
            let watcher = self.watcher.lock().await;
            // The bus publishes every reported status so an indicator can stay alive; a chat must
            // not receive the repeats.
            if !watcher.alert_is_new(&notification.session_id, &notification.text) {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "session notification unchanged — no alert session_id={}",
                    notification.session_id
                );
                return Ok(());
            }
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
            if let Err(e) = self.sender.send_message(chat_id, &notification.text).await {
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
