//! Telegram as a session front-end: starting and controlling sessions from a chat, notifying on
//! agent activity, and the elicitation loop that asks an operator a question and waits.
//!
//! # What is here, and what is not
//!
//! `#unbundle` node 2 planned to move all nine of the daemon's telegram modules into this crate.
//! Five could move:
//!
//! - [`sender`] — the **transport**: the [`TelegramSender`] port, its teloxide and in-memory
//!   implementations, and [`send_daemon_lifecycle_message`]. Split out of the daemon's
//!   `telegram_notifier.rs`, whose *policy* half (`TelegramSessionWatcher`) stayed behind.
//! - [`active_elicitation`] — the per-chat active-elicitation lease and its ordered queue.
//! - [`elicitation`] — the chat line a presenter gate asks its operator with.
//! - [`telegram_tracked_session`] — the per-chat tracked-session gate and traffic logging.
//! - [`telegram_github_link`] — the Telegram user ↔ GitHub login binding.
//! - [`status`] — Telegram's entry on the daemon's integration status board, and the token
//!   redaction every error written there passes through.
//!
//! The other four — `telegram_session_control`, `telegram_bot`, `telegram_session_subscriber` and
//! `telegram_multi_select_shortcuts` — plus `telegram_notifier`'s policy half
//! (`TelegramSessionWatcher`) did not come here, because they drive the session lifecycle. They
//! are the **control plane**, and since `#carve` 8/11 they live in `tddy-telegram-control`, which
//! sits above this crate and `tddy-session-lifecycle` and depends on both.
//!
//! `teloxide` is therefore used here (the transport) and by `tddy-telegram-control`'s
//! `telegram_bot`; `tddy-daemon`'s `runtime` still builds the `Bot` itself.
//!
//! # Direction of dependency
//!
//! Telegram is a **satellite**: only `tddy-telegram-control` (the control plane) and `tddy-daemon`
//! (the wiring) depend on this crate, never a session crate or the kernel — the ratchet
//! `tddy-daemon-extensions/tests/telegram_is_a_satellite.rs` holds that line. The core reaches a
//! chat only through `tddy_daemon_kernel::operator_channel::OperatorChannel`, which the control
//! plane's `TelegramDaemonHooks` implements. Nothing here names the control plane as a path — the
//! doc references to it are deliberately plain code spans, not intra-doc links, because this crate
//! cannot see it.

pub mod active_elicitation;
pub mod elicitation;
pub mod sender;
pub mod status;
pub mod telegram_github_link;
pub mod telegram_tracked_session;

pub use sender::{
    send_daemon_lifecycle_message, send_telegram_via_teloxide, send_telegram_with_inline_keyboard,
    InMemoryTelegramSender, InlineKeyboardRows, TelegramSender, TeloxideSender,
};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use tddy_daemon_kernel::config::DaemonConfig;

    /// A sender that records what it was asked to deliver, so a test can assert on the message
    /// rather than on the fact that a call happened.
    #[derive(Default)]
    struct ARecordingSender {
        sent: Mutex<Vec<(i64, String)>>,
    }

    #[async_trait]
    impl TelegramSender for ARecordingSender {
        async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
            self.sent.lock().unwrap().push((chat_id, text.to_string()));
            Ok(())
        }

        async fn send_message_with_keyboard(
            &self,
            chat_id: i64,
            text: &str,
            _inline_keyboard: InlineKeyboardRows,
        ) -> anyhow::Result<()> {
            self.sent.lock().unwrap().push((chat_id, text.to_string()));
            Ok(())
        }
    }

    fn config_from_yaml(yaml: &str) -> DaemonConfig {
        let dir = tempfile::tempdir().expect("a temp dir for the daemon config");
        let path = dir.path().join("daemon.yaml");
        std::fs::write(&path, yaml).expect("writing the daemon config");
        DaemonConfig::load(&path).expect("loading the daemon config")
    }

    #[tokio::test]
    async fn announces_that_the_daemon_started() {
        // Given
        let config = config_from_yaml(
            r#"
telegram:
  enabled: true
  bot_token: "test-token"
  chat_ids: [424242]
"#,
        );
        let sender = ARecordingSender::default();
        // The text is the caller's, exactly as `tddy_daemon::server` composes it at startup.
        let announcement = "tddy-daemon started (test-instance)";

        // When
        send_daemon_lifecycle_message(&config, &sender, announcement)
            .await
            .expect("a configured sender delivers");

        // Then
        let sent = sender.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 424242, "delivered to the configured chat");
        assert!(
            sent[0].1.to_lowercase().contains("started"),
            "the message says what happened: {:?}",
            sent[0].1
        );
    }

    /// An unconfigured telegram is not an error the daemon should fail to start over, but it is
    /// also not a silent success — nothing is delivered, and every call site checks.
    #[tokio::test]
    async fn delivers_nothing_when_telegram_is_unconfigured() {
        // Given
        let config = config_from_yaml("{}\n");
        let sender = ARecordingSender::default();

        // When
        send_daemon_lifecycle_message(&config, &sender, "tddy-daemon started (test-instance)")
            .await
            .expect("an unconfigured telegram is not a startup failure");

        // Then
        assert!(
            sender.sent.lock().unwrap().is_empty(),
            "unconfigured telegram delivers nothing"
        );
    }

    /// Configured but switched off is a third state, and it is the one an operator uses to mute
    /// the bot without deleting their token.
    #[tokio::test]
    async fn delivers_nothing_when_telegram_is_configured_but_disabled() {
        // Given
        let config = config_from_yaml(
            r#"
telegram:
  enabled: false
  bot_token: "test-token"
  chat_ids: [424242]
"#,
        );
        let sender = ARecordingSender::default();

        // When
        send_daemon_lifecycle_message(&config, &sender, "tddy-daemon started (test-instance)")
            .await
            .expect("a disabled telegram is not a startup failure");

        // Then
        assert!(
            sender.sent.lock().unwrap().is_empty(),
            "disabled telegram delivers nothing"
        );
    }
}
