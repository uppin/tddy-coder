//! Telegram session status notifications via teloxide (`Bot::send_message`).
//!
//! [`TelegramSessionWatcher`] records the last-seen status per session and emits at most one
//! notification per **status transition** for **active** sessions when Telegram is enabled.
//! The first observation for a session establishes a baseline (no message). Repeating the same
//! status—especially terminal states—does not spam.

use std::collections::HashMap;

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::ChatId;

use crate::config::DaemonConfig;

/// Development trace hook — logs structured scope for debugging (reduced in later phases).
fn marker_json(marker_id: &str, scope: &str) {
    log::debug!(
        target: "tddy_daemon::telegram",
        "telegram_notifier trace marker marker_id={} scope={}",
        marker_id,
        scope
    );
}

/// Short label for Telegram: first two hyphen-separated segments of `session_id` (UUID-shaped).
///
/// Example: `018f1234-5678-7abc-8def-123456789abc` → `018f1234-5678`.
pub fn session_telegram_label(session_id: &str) -> Option<String> {
    marker_json(
        "M001",
        "tddy_daemon::telegram_notifier::session_telegram_label",
    );
    let parts: Vec<&str> = session_id.split('-').collect();
    if parts.len() < 2 {
        log::debug!(
            target: "tddy_daemon::telegram",
            "session_telegram_label: fewer than two hyphen segments (len={})",
            session_id.len()
        );
        return None;
    }
    Some(format!("{}-{}", parts[0], parts[1]))
}

/// Whether `status` is terminal (session finished; repeated reads should not notify).
pub fn is_terminal_session_status(status: &str) -> bool {
    marker_json(
        "M002",
        "tddy_daemon::telegram_notifier::is_terminal_session_status",
    );
    status.eq_ignore_ascii_case("completed") || status.eq_ignore_ascii_case("failed")
}

/// Mask bot token for log lines — must never print the full secret.
pub fn mask_bot_token_for_logs(token: &str) -> String {
    marker_json(
        "M006",
        "tddy_daemon::telegram_notifier::mask_bot_token_for_logs",
    );
    if token.is_empty() {
        return String::new();
    }
    // Never embed substrings of the token; length-only metadata is enough for operators.
    format!("<redacted bot_token len={}>", token.len())
}

/// Send a plain-text Telegram message using teloxide (production path; tests use [`TelegramSender`]).
pub async fn send_telegram_via_teloxide(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
) -> anyhow::Result<()> {
    marker_json(
        "M007",
        "tddy_daemon::telegram_notifier::send_telegram_via_teloxide",
    );
    log::info!(
        target: "tddy_daemon::telegram",
        "send_telegram_via_teloxide: dispatching send_message chat_id={:?} text_len={}",
        chat_id,
        text.len()
    );
    bot.send_message(chat_id, text.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("telegram send_message failed: {e}"))?;
    log::debug!(
        target: "tddy_daemon::telegram",
        "send_telegram_via_teloxide: send completed chat_id={:?}",
        chat_id
    );
    Ok(())
}

#[async_trait]
pub trait TelegramSender: Send + Sync {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()>;
}

/// Tracks last-seen status per session and sends Telegram on qualifying transitions.
pub struct TelegramSessionWatcher {
    last_status: HashMap<String, String>,
}

impl TelegramSessionWatcher {
    pub fn new() -> Self {
        marker_json(
            "M003",
            "tddy_daemon::telegram_notifier::TelegramSessionWatcher::new",
        );
        log::info!(target: "tddy_daemon::telegram", "TelegramSessionWatcher: initialized");
        Self {
            last_status: HashMap::new(),
        }
    }

    /// Invoked when daemon observes session metadata for an active session (PID alive).
    ///
    /// Behavior:
    /// - If Telegram is disabled or unset, never call `sender`.
    /// - Inactive sessions (`is_active == false`): no sends and no last-status tracking updates.
    /// - Only notify on **status change** after the first observation (baseline tick is silent).
    /// - Message text includes [`session_telegram_label`] for `session_id`.
    /// - Repeating the same status (including terminal) does not send again.
    pub async fn on_metadata_tick<S: TelegramSender + ?Sized>(
        &mut self,
        config: &DaemonConfig,
        sender: &S,
        session_id: &str,
        status: &str,
        is_active: bool,
    ) -> anyhow::Result<()> {
        marker_json(
            "M004",
            "tddy_daemon::telegram_notifier::TelegramSessionWatcher::on_metadata_tick",
        );
        log::debug!(
            target: "tddy_daemon::telegram",
            "on_metadata_tick: entry session_id={} status={} is_active={}",
            session_id,
            status,
            is_active
        );

        let Some(tg) = config.telegram.as_ref() else {
            log::debug!(target: "tddy_daemon::telegram", "on_metadata_tick: no telegram config");
            return Ok(());
        };
        if !tg.enabled {
            log::info!(
                target: "tddy_daemon::telegram",
                "on_metadata_tick: telegram disabled in config"
            );
            return Ok(());
        }

        if !is_active {
            log::debug!(
                target: "tddy_daemon::telegram",
                "on_metadata_tick: session not active — skipping session_id={}",
                session_id
            );
            return Ok(());
        }

        let prev = self.last_status.get(session_id).map(String::as_str);
        match prev {
            None => {
                self.last_status
                    .insert(session_id.to_string(), status.to_string());
                log::info!(
                    target: "tddy_daemon::telegram",
                    "on_metadata_tick: baseline status recorded (no notification) session_id={} status={}",
                    session_id,
                    status
                );
            }
            Some(p) if p == status => {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "on_metadata_tick: status unchanged — no notification session_id={} status={} terminal={}",
                    session_id,
                    status,
                    is_terminal_session_status(status)
                );
            }
            Some(p) => {
                let label = session_telegram_label(session_id).unwrap_or_else(|| {
                    log::debug!(
                        target: "tddy_daemon::telegram",
                        "session_telegram_label returned None; using raw session_id in message session_id={}",
                        session_id
                    );
                    session_id.to_string()
                });
                let text = format!("Session {label}: status changed from {p} to {status}");
                log::info!(
                    target: "tddy_daemon::telegram",
                    "on_metadata_tick: status transition — sending Telegram notification session_id={} old={} new={} label={} chat_targets={}",
                    session_id,
                    p,
                    status,
                    label,
                    tg.chat_ids.len()
                );
                self.last_status
                    .insert(session_id.to_string(), status.to_string());
                for &cid in &tg.chat_ids {
                    log::debug!(
                        target: "tddy_daemon::telegram",
                        "on_metadata_tick: send_message chat_id={}",
                        cid
                    );
                    sender.send_message(cid, &text).await?;
                }
            }
        }
        Ok(())
    }
}

impl Default for TelegramSessionWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod acceptance_unit_tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockSender {
        calls: Arc<Mutex<usize>>,
    }

    impl MockSender {
        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl TelegramSender for MockSender {
        async fn send_message(&self, _chat_id: i64, _text: &str) -> anyhow::Result<()> {
            *self.calls.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[test]
    fn two_segment_label_from_uuid_session_id() {
        let sid_a = "018f1234-5678-7abc-8def-123456789abc";
        let sid_b = "018f9999-0000-1111-2222-333333333333";
        let a = session_telegram_label(sid_a);
        let b = session_telegram_label(sid_b);
        assert_eq!(a.as_deref(), Some("018f1234-5678"));
        assert_eq!(b.as_deref(), Some("018f9999-0000"));
        assert_ne!(a, b);
    }

    #[test]
    fn is_terminal_session_status_recognizes_completed_and_failed() {
        assert!(
            is_terminal_session_status("completed"),
            "completed is terminal"
        );
        assert!(is_terminal_session_status("failed"), "failed is terminal");
        assert!(
            !is_terminal_session_status("active"),
            "active is in progress"
        );
    }

    #[test]
    fn mask_bot_token_redacts_secret() {
        let masked = mask_bot_token_for_logs("MY_SECRET_TOKEN_12345");
        assert!(
            !masked.contains("MY_SECRET"),
            "logs must not contain raw token material; got {masked:?}"
        );
    }

    #[tokio::test]
    async fn inactive_session_skips_notification_even_on_transition() {
        let mut watcher = TelegramSessionWatcher::new();
        let cfg = DaemonConfig::default();
        let mock = MockSender::default();
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        watcher
            .on_metadata_tick(&cfg, &mock, sid, "active", false)
            .await
            .unwrap();
        watcher
            .on_metadata_tick(&cfg, &mock, sid, "paused", false)
            .await
            .unwrap();
        assert_eq!(
            mock.calls(),
            0,
            "inactive sessions must not trigger Telegram sends"
        );
    }
}
