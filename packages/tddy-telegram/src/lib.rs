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
//! - [`elicitation`] — what counts as a presenter gate worth asking an operator about.
//! - [`telegram_tracked_session`] — the per-chat tracked-session gate and traffic logging.
//! - [`telegram_github_link`] — the Telegram user ↔ GitHub login binding.
//!
//! The other four — `telegram_session_control`, `telegram_bot`, `telegram_session_subscriber` and
//! `telegram_multi_select_shortcuts` — plus `telegram_notifier`'s policy half
//! (`TelegramSessionWatcher`) could not come here, because they drive the session lifecycle and
//! `tddy-session-lifecycle` depends on this crate: holding them would close a cycle. They are the
//! **control plane**, and since `#carve` 8/11 they live in `tddy-telegram-control`, which sits
//! between this crate and `tddy-session-lifecycle` and depends on both.
//!
//! `teloxide` is therefore used here (the transport) and by `tddy-telegram-control`'s
//! `telegram_bot`; `tddy-daemon`'s `runtime` still builds the `Bot` itself.
//!
//! # Direction of dependency
//!
//! One way only: `tddy-session-lifecycle`, `tddy-session-activity` and `tddy-telegram-control`
//! depend on `tddy-telegram`, never the reverse. Nothing here names either as a path — the doc references to the control
//! plane are deliberately plain code spans, not intra-doc links, because this crate cannot see it.
//!
//! `tddy-session-lifecycle`'s `session_list_enrichment` and `tddy-session-activity`'s
//! notifications call [`elicitation`], which is that same one-way edge; the modules that call
//! *back* into them (`telegram_session_control`, `telegram_session_subscriber`) are the control
//! plane, which now sits above both crates in `tddy-telegram-control`, so neither pair is mutual.

pub mod active_elicitation;
pub mod elicitation;
pub mod sender;
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
