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
//! Four modules plus `telegram_notifier`'s policy half stayed in `tddy-daemon`, because moving
//! them would invert the dependency:
//!
//! - `telegram_session_control` (4,472 lines) is an orchestrator of the daemon's session
//!   lifecycle. It reaches fourteen daemon-owned modules — `spawner`, `session_room`,
//!   `cli_session_manager`, `session_reader`, `session_deletion`, `session_list_enrichment`,
//!   `spawn_worker`, `supervisor_client`, `supervisor_spawn`, `cursor_cli_spawn`,
//!   `presenter_intent_client`, `user_sessions_path` among them — and holds several of them as
//!   struct fields. None of those has left `tddy-daemon` yet; they belong to nodes 4 and 6-8.
//! - `telegram_bot` reaches `telegram_session_control`, so it is blocked transitively.
//! - `telegram_session_subscriber` reaches the daemon's `session_notifications`, and
//!   `telegram_notifier`'s watcher reaches `telegram_session_control`.
//! - `telegram_multi_select_shortcuts` needed only `InlineKeyboardRows`, which is now
//!   [`sender`]'s, so its one edge is no longer a blocker — it was simply out of this step's
//!   scope and is the next module to follow.
//!
//! `teloxide` therefore does **not** leave `tddy-daemon` in this node: `telegram_session_control`,
//! `telegram_bot`, `telegram_notifier`'s watcher and `runtime` all still use it directly.
//!
//! # Direction of dependency
//!
//! One way only: `tddy-daemon` depends on `tddy-telegram`, never the reverse. Nothing here names
//! `tddy_daemon` as a path — the three doc references to the blocked modules are deliberately
//! plain code spans, not intra-doc links, because this crate cannot see them.
//!
//! The daemon's `session_list_enrichment` and `session_notifications` call [`elicitation`], which
//! is that same one-way edge; the modules that call *back* into them
//! (`telegram_session_control`, `telegram_session_subscriber`) are exactly the ones held back, so
//! the mutual pair stays inside `tddy-daemon`. Both pairs are recorded with their `file:line` in
//! docs/dev/1-WIP/2026-09-09-unbundle-model-telegram-screen.md, "The telegram cycle".

// TODO(unbundle-node-2, M4): four telegram modules — `telegram_bot`,
// `telegram_multi_select_shortcuts`, `telegram_session_control`, `telegram_session_subscriber` —
// and `telegram_notifier`'s watcher half are still in `tddy-daemon`. All but
// `telegram_multi_select_shortcuts` are blocked on the daemon's session machinery leaving first
// (nodes 4 and 6-8), not on anything this crate is missing.
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
