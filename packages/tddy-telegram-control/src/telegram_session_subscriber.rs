//! Telegram's side of a session's presenter observer: the shared hooks the daemon wires once, and
//! the [`PresenterEventSink`] through which each presenter event reaches the
//! [`TelegramSessionWatcher`].
//!
//! The observer loop itself — connect, stream, publish to the notification bus — is not here: it
//! runs whether or not Telegram is configured, so it lives with the connection service in
//! [`tddy_session_lifecycle::presenter_observer_task`].

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use tddy_daemon_kernel::presenter_observer::PresenterEventSink;
use tddy_service::gen::ServerMessage;

use crate::telegram_notifier::{TelegramSender, TelegramSessionWatcher};
use tddy_session_lifecycle::config::DaemonConfig;

/// Shared handles for Telegram + watcher; injected into ConnectionService as its presenter-event
/// sink, and used by the Telegram-started spawn path the same way.
pub struct TelegramDaemonHooks {
    pub config: DaemonConfig,
    pub sender: Arc<dyn TelegramSender + Send + Sync>,
    pub watcher: Arc<Mutex<TelegramSessionWatcher>>,
}

#[async_trait]
impl PresenterEventSink for TelegramDaemonHooks {
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
}
