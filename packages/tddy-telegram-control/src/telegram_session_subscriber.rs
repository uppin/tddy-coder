//! Telegram as the daemon's operator channel: the shared hooks the daemon wires once, and the
//! [`OperatorChannel`] through which a session's presenter events and its attention-worthy
//! notifications reach a chat.
//!
//! PRD `docs/ft/daemon/telegram-notifications.md`, and
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
use tddy_session_activity::session_notifications::LastDeliveredPerSession;

use crate::agent_notification_telegram::AgentNotificationTelegram;
use crate::question_alert::QuestionAlert;
use crate::telegram_notifier::{TelegramSender, TelegramSessionWatcher};
use tddy_session_lifecycle::config::DaemonConfig;

/// Shared handles for Telegram + watcher; installed as the daemon's operator channel, and used by
/// the Telegram-started spawn path the same way.
pub struct TelegramDaemonHooks {
    pub config: DaemonConfig,
    pub sender: Arc<dyn TelegramSender + Send + Sync>,
    pub watcher: Arc<Mutex<TelegramSessionWatcher>>,
    /// Last notification text sent per session: the bus re-publishes an unchanged status so an
    /// indicator stays alive, and a chat must not receive the repeats.
    last_alerted: LastDeliveredPerSession,
}

impl TelegramDaemonHooks {
    /// Hooks over `config`, `sender` and `watcher`, with no notification sent yet.
    pub fn new(
        config: DaemonConfig,
        sender: Arc<dyn TelegramSender + Send + Sync>,
        watcher: Arc<Mutex<TelegramSessionWatcher>>,
    ) -> Self {
        Self {
            config,
            sender,
            watcher,
            last_alerted: LastDeliveredPerSession::default(),
        }
    }

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

    /// Who a notification for `session_id` goes to, or `None` when it goes to nobody.
    ///
    /// Chats explicitly tracking this session take priority: a session started or entered from
    /// Telegram routes only to that operator (the same targeting the workflow keyboards use). When
    /// no chat tracks it — a claude-cli session started from the web UI or `claude` directly — the
    /// daemon's configured broadcast list gets it, so activity alerts still reach operators.
    async fn recipients_of(&self, session_id: &str) -> Option<Vec<i64>> {
        let tracked_chat_ids = self.watcher.lock().await.chats_tracking_session(session_id);
        // `None` means the tracking map could not be read, which is not the same as "nobody claimed
        // this session". Broadcasting on an unknown would announce a session one operator had
        // claimed to every configured chat, so an unreadable map sends nothing at all.
        let Some(tracked_chat_ids) = tracked_chat_ids else {
            log::error!(
                target: "tddy_daemon::telegram",
                "tracked chats unreadable for session_id={session_id} — no alert sent"
            );
            return None;
        };
        let (chat_ids, routing) = if tracked_chat_ids.is_empty() {
            (self.broadcast_chat_ids(), "configured_broadcast")
        } else {
            (tracked_chat_ids, "tracked")
        };
        if chat_ids.is_empty() {
            log::debug!(
                target: "tddy_daemon::telegram",
                "no chats tracking session_id={session_id} and no configured chat_ids — no alert"
            );
            return None;
        }
        log::info!(
            target: "tddy_daemon::telegram",
            "sending session notification session_id={session_id} routing={routing} chats={}",
            chat_ids.len()
        );
        Some(chat_ids)
    }

    /// Send `notification`'s text to each of `chat_ids`, failing with how many it missed.
    ///
    /// The bus owns the copy: the text is sent exactly as it was published, so a chat message and
    /// the drawer's tooltip read the same sentence.
    async fn send_to_every_chat(
        &self,
        chat_ids: Vec<i64>,
        notification: &SessionNotification,
    ) -> anyhow::Result<()> {
        let total = chat_ids.len();
        let mut failed = 0_usize;
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

    /// Send `html` to each of `chat_ids` in Telegram's HTML mode, loudly, failing with how many it
    /// missed.
    async fn send_html_to_every_chat(
        &self,
        chat_ids: Vec<i64>,
        session_id: &str,
        html: &str,
    ) -> anyhow::Result<()> {
        let total = chat_ids.len();
        let mut failed = 0_usize;
        for chat_id in chat_ids {
            if let Err(e) = self.sender.send_html_message(chat_id, html, false).await {
                failed += 1;
                log::warn!(
                    target: "tddy_daemon::telegram",
                    "send_html_message failed chat_id={chat_id} session_id={session_id}: {e:#}"
                );
            }
        }
        if failed > 0 {
            anyhow::bail!("telegram send failed for {failed} of {total} chat(s)");
        }
        Ok(())
    }

    /// Tell the chats what a session is asking: one message per request.
    ///
    /// The bus publishes an unchanged question again, as it does a status, so the request is
    /// recorded before anything is sent and a repeat is no alert. The record is keyed by the request,
    /// apart from the texts of status notifications, which share the same per-session memory.
    async fn alert_pending_question(
        &self,
        notification: &SessionNotification,
    ) -> anyhow::Result<()> {
        let Some(question) = notification.question.as_ref() else {
            return Ok(());
        };
        let Some(alert) = QuestionAlert::render(notification) else {
            return Ok(());
        };
        if !self.last_alerted.record_and_is_new(
            &notification.session_id,
            &format!("question:{}", question.request_id),
        ) {
            log::debug!(
                target: "tddy_daemon::telegram",
                "pending question already alerted — no alert session_id={}",
                notification.session_id
            );
            return Ok(());
        }
        let Some(chat_ids) = self.recipients_of(&notification.session_id).await else {
            return Ok(());
        };
        self.send_html_to_every_chat(chat_ids, &notification.session_id, alert.html())
            .await
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
    /// It takes attention-worthy events from the activity-status path, the agent's own words and
    /// a claude-cli session's pending question, and nothing else. Two exclusions carry the design
    /// (PRD FR7):
    ///
    /// - **`Activity` is declined.** The indicator feed is why that kind exists; sending it to a
    ///   chat would turn every tool call into a message.
    /// - **`Presenter` is declined.** A workflow session's elicitations already reach Telegram
    ///   through [`Self::on_presenter_event`], with their inline keyboards, per-chat FIFO and
    ///   tracked-session gate. Taking them here as well would send each one twice.
    ///
    /// The status notification that follows a pending question is declined too: the question
    /// already told the operator, so the generic "needs your input" would say it twice.
    fn wants_notification(&self, notification: &SessionNotification) -> bool {
        self.enabled()
            && notification.kind == SessionNotificationKind::AttentionRequired
            && match notification.source {
                SessionNotificationSource::PendingQuestion => true,
                SessionNotificationSource::ActivityStatus => notification.question.is_none(),
                SessionNotificationSource::Agent => true,
                SessionNotificationSource::AgentToolCall | SessionNotificationSource::Presenter => {
                    false
                }
            }
    }

    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        if notification.source == SessionNotificationSource::PendingQuestion {
            return self.alert_pending_question(notification).await;
        }
        // An agent's notification is an event of its own, so two identical ones are both delivered;
        // only a re-published status is de-duplicated.
        let from_agent = AgentNotificationTelegram::render(notification);
        if from_agent.is_none()
            && !self
                .last_alerted
                .record_and_is_new(&notification.session_id, &notification.text)
        {
            log::debug!(
                target: "tddy_daemon::telegram",
                "session notification unchanged — no alert session_id={}",
                notification.session_id
            );
            return Ok(());
        }
        let Some(chat_ids) = self.recipients_of(&notification.session_id).await else {
            return Ok(());
        };
        match from_agent {
            Some(telegram) => telegram.send_to(self.sender.as_ref(), &chat_ids).await,
            None => self.send_to_every_chat(chat_ids, notification).await,
        }
    }
}
