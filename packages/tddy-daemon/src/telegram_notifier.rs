//! Telegram session status notifications via teloxide (`Bot::send_message`).
//!
//! [`TelegramSessionWatcher`] records the last-seen status per session and emits at most one
//! notification per **status transition** for **active** sessions when Telegram is enabled.
//! The first observation for a session establishes a baseline (no message). Repeating the same
//! status—especially terminal states—does not spam.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::ChatId;

use tddy_service::gen::server_message::Event;
use tddy_service::gen::ServerMessage;

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

/// Production [`TelegramSender`] using teloxide [`Bot`].
pub struct TeloxideSender {
    bot: Bot,
}

impl TeloxideSender {
    pub fn new(bot: Bot) -> Self {
        Self { bot }
    }

    pub fn from_bot_token(token: impl Into<String>) -> Self {
        Self {
            bot: Bot::new(token.into()),
        }
    }
}

#[async_trait]
impl TelegramSender for TeloxideSender {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        send_telegram_via_teloxide(&self.bot, ChatId(chat_id), text).await
    }
}

/// Test-only sender that records `(chat_id, text)` for assertions (no network I/O).
///
/// Optional inline keyboard labels per row are stored for session-control harness tests.
#[derive(Clone)]
pub struct InMemoryTelegramSender {
    messages: Arc<Mutex<Vec<(i64, String, Vec<Vec<String>>)>>>,
}

impl Default for InMemoryTelegramSender {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTelegramSender {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Backward-compatible view: chat id and text only (ignores inline keyboards).
    pub fn recorded(&self) -> Vec<(i64, String)> {
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .iter()
            .map(|(id, text, _)| (*id, text.clone()))
            .collect()
    }

    /// Full recording including inline keyboard button labels (row-major).
    pub fn recorded_with_keyboards(&self) -> Vec<(i64, String, Vec<Vec<String>>)> {
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .clone()
    }

    pub async fn send_message_with_inline_keyboard(
        &self,
        chat_id: i64,
        text: &str,
        inline_keyboard_labels: Vec<Vec<String>>,
    ) -> anyhow::Result<()> {
        log::debug!(
            target: "tddy_daemon::telegram",
            "InMemoryTelegramSender: send_message_with_inline_keyboard chat_id={} text_len={} keyboard_rows={}",
            chat_id,
            text.len(),
            inline_keyboard_labels.len()
        );
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .push((chat_id, text.to_string(), inline_keyboard_labels));
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl TelegramSender for InMemoryTelegramSender {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .push((chat_id, text.to_string(), Vec::new()));
        Ok(())
    }
}

/// Send the same lifecycle message to every configured chat (startup / shutdown).
pub async fn send_daemon_lifecycle_message<S: TelegramSender + ?Sized>(
    config: &DaemonConfig,
    sender: &S,
    text: &str,
) -> anyhow::Result<()> {
    let Some(tg) = config.telegram.as_ref() else {
        return Ok(());
    };
    if !tg.enabled {
        return Ok(());
    }
    for &cid in &tg.chat_ids {
        sender.send_message(cid, text).await?;
    }
    Ok(())
}

/// Tracks last-seen status per session and sends Telegram on qualifying transitions.
pub struct TelegramSessionWatcher {
    last_status: HashMap<String, String>,
    last_state_transition: HashMap<String, (String, String)>,
    last_workflow: HashMap<String, (bool, String)>,
    last_goal: HashMap<String, String>,
    last_backend: HashMap<String, (String, String)>,
    /// Last serialized [`ModeChanged`] signature per session (elicitation Telegram dedupe).
    last_elicitation_signature: HashMap<String, String>,
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
            last_state_transition: HashMap::new(),
            last_workflow: HashMap::new(),
            last_goal: HashMap::new(),
            last_backend: HashMap::new(),
            last_elicitation_signature: HashMap::new(),
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

    /// Handle a gRPC [`ServerMessage`] from the child `tddy-coder` Presenter observer stream.
    ///
    /// Maps `StateChanged`, `WorkflowComplete`, `GoalStarted`, `BackendSelected`, and
    /// `ModeChanged` (elicitation); deduplicates repeated identical payloads per session so Telegram
    /// is not spammed.
    pub async fn on_server_message<S: TelegramSender + ?Sized>(
        &mut self,
        config: &DaemonConfig,
        sender: &S,
        session_id: &str,
        msg: &ServerMessage,
    ) -> anyhow::Result<()> {
        let Some(tg) = config.telegram.as_ref() else {
            return Ok(());
        };
        if !tg.enabled {
            return Ok(());
        }

        let Some(ref event) = msg.event else {
            return Ok(());
        };

        let label = session_telegram_label(session_id).unwrap_or_else(|| session_id.to_string());

        let text: Option<String> = match event {
            Event::StateChanged(sc) => {
                let key = (sc.from.clone(), sc.to.clone());
                if self.last_state_transition.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_state_transition
                    .insert(session_id.to_string(), key);
                Some(format!("Session {label}: {} -> {}", sc.from, sc.to))
            }
            Event::WorkflowComplete(wc) => {
                let key = (wc.ok, wc.message.clone());
                if self.last_workflow.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_workflow.insert(session_id.to_string(), key);
                Some(if wc.ok {
                    format!("Session {label}: workflow completed")
                } else {
                    format!("Session {label}: workflow failed: {}", wc.message)
                })
            }
            Event::GoalStarted(g) => {
                if self.last_goal.get(session_id) == Some(&g.goal) {
                    return Ok(());
                }
                self.last_goal
                    .insert(session_id.to_string(), g.goal.clone());
                Some(format!("Session {label}: goal started: {}", g.goal))
            }
            Event::BackendSelected(b) => {
                let key = (b.agent.clone(), b.model.clone());
                if self.last_backend.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_backend.insert(session_id.to_string(), key);
                Some(format!("Session {label}: using {} ({})", b.agent, b.model))
            }
            Event::ModeChanged(mc) => {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "on_server_message: ModeChanged for session_id={} (elicitation path)",
                    session_id
                );
                let sig = crate::elicitation::elicitation_signature_for_mode_changed(mc);
                if self.last_elicitation_signature.get(session_id) == Some(&sig) {
                    log::debug!(
                        target: "tddy_daemon::telegram",
                        "on_server_message: duplicate elicitation ModeChanged signature — skip send session_id={}",
                        session_id
                    );
                    return Ok(());
                }
                let line =
                    crate::elicitation::telegram_elicitation_line_for_mode_changed(&label, mc);
                if let Some(t) = line {
                    log::info!(
                        target: "tddy_daemon::telegram",
                        "on_server_message: elicitation Telegram line ready session_id={} sig_len={}",
                        session_id,
                        sig.len()
                    );
                    self.last_elicitation_signature
                        .insert(session_id.to_string(), sig);
                    Some(t)
                } else {
                    log::debug!(
                        target: "tddy_daemon::telegram",
                        "on_server_message: ModeChanged not classified as user elicitation — no Telegram session_id={}",
                        session_id
                    );
                    None
                }
            }
            _ => None,
        };

        let Some(text) = text else {
            return Ok(());
        };

        log::info!(
            target: "tddy_daemon::telegram",
            "on_server_message: sending notification session_id={} text_len={}",
            session_id,
            text.len()
        );

        for &cid in &tg.chat_ids {
            sender.send_message(cid, &text).await?;
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
    async fn on_server_message_state_changed_sends_then_dedupes() {
        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let m1 = ServerMessage {
            event: Some(Event::StateChanged(tddy_service::gen::StateChanged {
                from: "a".into(),
                to: "b".into(),
            })),
        };
        watcher
            .on_server_message(&cfg, &mem, sid, &m1)
            .await
            .unwrap();
        watcher
            .on_server_message(&cfg, &mem, sid, &m1)
            .await
            .unwrap();
        assert_eq!(mem.len(), 1, "duplicate StateChanged must not send twice");
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

    /// Acceptance: `ModeChanged` with document approval (user-input mode) must emit a Telegram
    /// line that clearly indicates approval or input is required (plus session label).
    #[tokio::test]
    async fn telegram_notifier_sends_elicitation_message_on_mode_changed_to_user_input() {
        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let msg = tddy_service::convert::session_document_approval_to_server_message(
            "doc-preview".to_string(),
        );
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        let recorded = mem.recorded();
        assert_eq!(
            recorded.len(),
            1,
            "document-approval ModeChanged must produce exactly one Telegram message"
        );
        let text = &recorded[0].1;
        let lower = text.to_lowercase();
        assert!(
            lower.contains("input") || lower.contains("approval"),
            "elicitation Telegram text must mention input or approval; got {text:?}"
        );
    }

    /// Acceptance: identical `ModeChanged` payloads must not increase send count (dedupe).
    #[tokio::test]
    async fn telegram_notifier_dedupes_repeated_identical_elicitation_signals() {
        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018faaaa-1111-7abc-8def-123456789abc";
        let msg = tddy_service::convert::session_document_approval_to_server_message(
            "same-doc".to_string(),
        );
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        assert_eq!(
            mem.len(),
            1,
            "duplicate identical elicitation ModeChanged must not spam Telegram"
        );
    }
}
