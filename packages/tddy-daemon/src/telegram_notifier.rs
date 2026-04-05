//! Telegram session status notifications via teloxide (`Bot::send_message`).
//!
//! [`TelegramSessionWatcher`] records the last-seen status per session and emits at most one
//! notification per **status transition** for **active** sessions when Telegram is enabled.
//! The first observation for a session establishes a baseline (no message). Repeating the same
//! status—especially terminal states—does not spam.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup};

use tddy_service::gen::server_message::Event;
use tddy_service::gen::ServerMessage;

use crate::config::DaemonConfig;
use crate::telegram_session_control::chunk_telegram_text;

/// Telegram Bot API maximum message length for outbound body text before splitting (`send_message`).
const TELEGRAM_MESSAGE_BODY_MAX_UTF8: usize = 4096;

/// Session id → full confirmation strings per option index (for post-select Telegram message).
pub type ElicitationSelectOptionsCache = Arc<StdMutex<HashMap<String, Vec<String>>>>;

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

/// Row-major inline keyboard for Telegram: each row is `(button label, callback_data)`.
pub type InlineKeyboardRows = Vec<Vec<(String, String)>>;

#[async_trait]
pub trait TelegramSender: Send + Sync {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()>;

    /// Send a message with an inline keyboard (`callback_data` per Telegram Bot API, max 64 bytes each).
    async fn send_message_with_keyboard(
        &self,
        chat_id: i64,
        text: &str,
        inline_keyboard: InlineKeyboardRows,
    ) -> anyhow::Result<()>;
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

    async fn send_message_with_keyboard(
        &self,
        chat_id: i64,
        text: &str,
        inline_keyboard: InlineKeyboardRows,
    ) -> anyhow::Result<()> {
        send_telegram_with_inline_keyboard(&self.bot, ChatId(chat_id), text, inline_keyboard).await
    }
}

/// Send a text message with an inline keyboard (production path).
pub async fn send_telegram_with_inline_keyboard(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    rows: InlineKeyboardRows,
) -> anyhow::Result<()> {
    log::info!(
        target: "tddy_daemon::telegram",
        "send_telegram_with_inline_keyboard: chat_id={:?} text_len={} rows={}",
        chat_id,
        text.len(),
        rows.len()
    );
    let keyboard: Vec<Vec<InlineKeyboardButton>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(label, data)| InlineKeyboardButton::callback(label, data))
                .collect()
        })
        .collect();
    let markup = InlineKeyboardMarkup::new(keyboard);
    bot.send_message(chat_id, text.to_string())
        .reply_markup(markup)
        .await
        .map_err(|e| anyhow::anyhow!("telegram send_message with keyboard failed: {e}"))?;
    Ok(())
}

/// `(chat_id, text, inline_keyboard: label + callback_data per button)` — one recorded outbound Telegram message.
type RecordedMessage = (i64, String, InlineKeyboardRows);

/// Test-only sender that records `(chat_id, text)` for assertions (no network I/O).
///
/// Optional inline keyboard labels per row are stored for session-control harness tests.
#[derive(Clone)]
pub struct InMemoryTelegramSender {
    messages: Arc<StdMutex<Vec<RecordedMessage>>>,
}

impl Default for InMemoryTelegramSender {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryTelegramSender {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(StdMutex::new(Vec::new())),
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

    /// Full recording including inline keyboard (row-major: label + callback_data per button).
    pub fn recorded_with_keyboards(&self) -> Vec<RecordedMessage> {
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .clone()
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

    async fn send_message_with_keyboard(
        &self,
        chat_id: i64,
        text: &str,
        inline_keyboard: InlineKeyboardRows,
    ) -> anyhow::Result<()> {
        log::debug!(
            target: "tddy_daemon::telegram",
            "InMemoryTelegramSender: send_message_with_keyboard chat_id={} text_len={} keyboard_rows={}",
            chat_id,
            text.len(),
            inline_keyboard.len()
        );
        self.messages
            .lock()
            .expect("InMemoryTelegramSender mutex")
            .push((chat_id, text.to_string(), inline_keyboard));
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
    /// Full labels for select elicitation (shared with [`crate::telegram_session_control::TelegramWorkflowSpawn`] for confirmations).
    elicitation_select_options: ElicitationSelectOptionsCache,
}

impl TelegramSessionWatcher {
    pub fn new() -> Self {
        Self::with_elicitation_select_options(Arc::new(StdMutex::new(HashMap::new())))
    }

    /// Same as [`Self::new`], but shares the select-option cache with inbound Telegram session control (confirmations).
    pub fn with_elicitation_select_options(
        elicitation_select_options: ElicitationSelectOptionsCache,
    ) -> Self {
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
            elicitation_select_options,
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

    async fn send_mode_changed_elicitation<S: TelegramSender + ?Sized>(
        &mut self,
        config: &DaemonConfig,
        sender: &S,
        session_id: &str,
        label: &str,
        mc: &tddy_service::gen::ModeChanged,
    ) -> anyhow::Result<()> {
        let Some(tg) = config.telegram.as_ref() else {
            return Ok(());
        };
        if !tg.enabled {
            return Ok(());
        }
        log::debug!(
            target: "tddy_daemon::telegram",
            "send_mode_changed_elicitation: session_id={}",
            session_id
        );
        let sig = crate::elicitation::elicitation_signature_for_mode_changed(mc);
        if self.last_elicitation_signature.get(session_id) == Some(&sig) {
            log::debug!(
                target: "tddy_daemon::telegram",
                "send_mode_changed_elicitation: duplicate signature — skip session_id={}",
                session_id
            );
            return Ok(());
        }
        let Some(action_line) =
            crate::elicitation::telegram_elicitation_line_for_mode_changed(label, mc)
        else {
            log::debug!(
                target: "tddy_daemon::telegram",
                "send_mode_changed_elicitation: not user elicitation session_id={}",
                session_id
            );
            return Ok(());
        };
        log::info!(
            target: "tddy_daemon::telegram",
            "send_mode_changed_elicitation: ready session_id={} sig_len={}",
            session_id,
            sig.len()
        );
        self.last_elicitation_signature
            .insert(session_id.to_string(), sig);
        use tddy_service::gen::app_mode_proto::Variant;
        if let Some(Variant::Select(s)) = mc.mode.as_ref().and_then(|m| m.variant.as_ref()) {
            if let Some(q) = s.question.as_ref() {
                let labels: Vec<String> = q
                    .options
                    .iter()
                    .map(question_option_full_confirmation_text)
                    .collect();
                self.elicitation_select_options
                    .lock()
                    .unwrap()
                    .insert(session_id.to_string(), labels);
            }
        }
        let kb = mode_changed_keyboard(session_id, mc);

        if let Some(body) = document_body_for_mode_changed(mc) {
            if !body.is_empty() {
                let chunks = chunk_telegram_text(&body, TELEGRAM_MESSAGE_BODY_MAX_UTF8);
                for &cid in &tg.chat_ids {
                    for chunk in &chunks {
                        sender.send_message(cid, chunk).await?;
                    }
                }
            }
        }

        if let Some(detail) = clarification_detail_body(label, mc) {
            if !detail.is_empty() {
                let chunks = chunk_telegram_text(&detail, TELEGRAM_MESSAGE_BODY_MAX_UTF8);
                for &cid in &tg.chat_ids {
                    for chunk in &chunks {
                        sender.send_message(cid, chunk).await?;
                    }
                }
            }
        }

        log::info!(
            target: "tddy_daemon::telegram",
            "send_mode_changed_elicitation: sending action line session_id={} text_len={}",
            session_id,
            action_line.len()
        );

        for &cid in &tg.chat_ids {
            if let Some(ref rows) = kb {
                if !rows.is_empty() {
                    sender
                        .send_message_with_keyboard(cid, &action_line, rows.clone())
                        .await?;
                } else {
                    sender.send_message(cid, &action_line).await?;
                }
            } else {
                sender.send_message(cid, &action_line).await?;
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

        let notification: Option<(String, Option<InlineKeyboardRows>)> = match event {
            Event::ModeChanged(mc) => {
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "on_server_message: ModeChanged for session_id={} (elicitation path)",
                    session_id
                );
                return self
                    .send_mode_changed_elicitation(config, sender, session_id, &label, mc)
                    .await;
            }
            Event::StateChanged(sc) => {
                let key = (sc.from.clone(), sc.to.clone());
                if self.last_state_transition.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_state_transition
                    .insert(session_id.to_string(), key);
                Some((format!("Session {label}: {} -> {}", sc.from, sc.to), None))
            }
            Event::WorkflowComplete(wc) => {
                let key = (wc.ok, wc.message.clone());
                if self.last_workflow.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_workflow.insert(session_id.to_string(), key);
                Some((
                    if wc.ok {
                        format!("Session {label}: workflow completed")
                    } else {
                        format!("Session {label}: workflow failed: {}", wc.message)
                    },
                    None,
                ))
            }
            Event::GoalStarted(g) => {
                if self.last_goal.get(session_id) == Some(&g.goal) {
                    return Ok(());
                }
                self.last_goal
                    .insert(session_id.to_string(), g.goal.clone());
                Some((format!("Session {label}: goal started: {}", g.goal), None))
            }
            Event::BackendSelected(b) => {
                let key = (b.agent.clone(), b.model.clone());
                if self.last_backend.get(session_id) == Some(&key) {
                    return Ok(());
                }
                self.last_backend.insert(session_id.to_string(), key);
                Some((
                    format!("Session {label}: using {} ({})", b.agent, b.model),
                    None,
                ))
            }
            _ => None,
        };

        let Some((text, keyboard)) = notification else {
            return Ok(());
        };

        log::info!(
            target: "tddy_daemon::telegram",
            "on_server_message: sending notification session_id={} text_len={}",
            session_id,
            text.len()
        );

        for &cid in &tg.chat_ids {
            if let Some(ref rows) = keyboard {
                if !rows.is_empty() {
                    sender
                        .send_message_with_keyboard(cid, &text, rows.clone())
                        .await?;
                } else {
                    sender.send_message(cid, &text).await?;
                }
            } else {
                sender.send_message(cid, &text).await?;
            }
        }
        Ok(())
    }
}

/// Inline keyboard for session-document review / markdown viewer (matches Virtual TUI affordances).
fn document_review_keyboard(
    session_id: &str,
    mc: &tddy_service::gen::ModeChanged,
) -> Option<InlineKeyboardRows> {
    use tddy_service::gen::app_mode_proto;
    let v = mc.mode.as_ref()?.variant.as_ref()?;
    let sid = session_id.to_string();
    match v {
        app_mode_proto::Variant::DocumentReview(_) => Some(vec![vec![
            ("Approve".to_string(), format!("doc:a:{sid}")),
            ("Reject".to_string(), format!("doc:j:{sid}")),
            ("Refine".to_string(), format!("doc:r:{sid}")),
        ]]),
        app_mode_proto::Variant::MarkdownViewer(_) => Some(vec![vec![
            ("Approve".to_string(), format!("doc:a:{sid}")),
            ("Refine".to_string(), format!("doc:r:{sid}")),
            ("Back".to_string(), format!("doc:d:{sid}")),
        ]]),
        _ => None,
    }
}

fn document_body_for_mode_changed(mc: &tddy_service::gen::ModeChanged) -> Option<String> {
    let v = mc.mode.as_ref()?.variant.as_ref()?;
    use tddy_service::gen::app_mode_proto::Variant;
    match v {
        Variant::DocumentReview(d) => Some(d.content.clone()),
        Variant::MarkdownViewer(d) => Some(d.content.clone()),
        _ => None,
    }
}

/// Full text for the post-selection Telegram confirmation (label; description on the next line if present).
fn question_option_full_confirmation_text(opt: &tddy_service::gen::QuestionOptionProto) -> String {
    let label = opt.label.trim();
    let desc = opt.description.trim();
    if desc.is_empty() {
        label.to_string()
    } else {
        format!("{label}\n{desc}")
    }
}

/// One line per option in the clarification message body (extended listing; buttons stay compact).
fn select_options_extended_listing(q: &tddy_service::gen::ClarificationQuestionProto) -> String {
    let mut out = String::from("\n\nOptions (tap a button below):\n");
    for (i, opt) in q.options.iter().enumerate() {
        out.push_str(&format!("{}. {}", i + 1, opt.label.trim()));
        let d = opt.description.trim();
        if !d.is_empty() {
            out.push_str(" — ");
            out.push_str(d);
        }
        out.push('\n');
    }
    out
}

/// Question text and (for multi-select) index instructions, sent before the footer line.
fn clarification_detail_body(
    session_label: &str,
    mc: &tddy_service::gen::ModeChanged,
) -> Option<String> {
    let v = mc.mode.as_ref()?.variant.as_ref()?;
    use tddy_service::gen::app_mode_proto::Variant;
    match v {
        Variant::Select(s) => {
            let q = s.question.as_ref()?;
            let mut t = String::new();
            if !q.header.is_empty() {
                t.push_str(&q.header);
                t.push_str("\n\n");
            }
            t.push_str(&q.question);
            t.push_str(&select_options_extended_listing(q));
            Some(t)
        }
        Variant::MultiSelect(m) => {
            let q = m.question.as_ref()?;
            let mut t = String::new();
            if !q.header.is_empty() {
                t.push_str(&q.header);
                t.push_str("\n\n");
            }
            t.push_str(&q.question);
            t.push_str("\n\nOptions (0-based indices):\n");
            for (i, opt) in q.options.iter().enumerate() {
                t.push_str(&format!("{i}. {}\n", opt.label));
            }
            t.push_str(&format!(
                "\nSend: /answer-multi {session_label} <i,j,...>\nExample: /answer-multi {session_label} 0,2"
            ));
            Some(t)
        }
        Variant::TextInput(tinp) => Some(tinp.prompt.clone()),
        _ => None,
    }
}

fn clarification_select_keyboard(
    session_id: &str,
    mc: &tddy_service::gen::ModeChanged,
) -> Option<InlineKeyboardRows> {
    use tddy_service::gen::app_mode_proto::Variant;
    let v = mc.mode.as_ref()?.variant.as_ref()?;
    let sel = match v {
        Variant::Select(s) => s,
        _ => return None,
    };
    let q = sel.question.as_ref()?;
    if q.options.is_empty() {
        return None;
    }
    let mut rows: InlineKeyboardRows = Vec::new();
    let mut row: Vec<(String, String)> = Vec::new();
    for i in 0..q.options.len() {
        let label = format!("{}", i + 1);
        let cb = format!("eli:s:{session_id}:{i}");
        if cb.len() > 64 {
            log::warn!(
                target: "tddy_daemon::telegram",
                "clarification callback_data too long ({} bytes); skipping option index {}",
                cb.len(),
                i
            );
            continue;
        }
        row.push((label, cb));
        if row.len() >= 8 {
            rows.push(row);
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}

fn mode_changed_keyboard(
    session_id: &str,
    mc: &tddy_service::gen::ModeChanged,
) -> Option<InlineKeyboardRows> {
    document_review_keyboard(session_id, mc)
        .or_else(|| clarification_select_keyboard(session_id, mc))
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
    use std::collections::HashMap;
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

        async fn send_message_with_keyboard(
            &self,
            _chat_id: i64,
            _text: &str,
            _inline_keyboard: InlineKeyboardRows,
        ) -> anyhow::Result<()> {
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

    /// Acceptance: `ModeChanged` with document approval must send the presenter document text
    /// to Telegram, then a short action line with Approve / Reject / Refine (inline keyboard).
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
        assert!(
            recorded.len() >= 2,
            "document-approval ModeChanged must send document body segment(s) and a separate action message; got {} message(s)",
            recorded.len()
        );
        let joined = recorded
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("doc-preview"),
            "Telegram outbound must include the presenter document body; got {joined:?}"
        );
        let lower = joined.to_lowercase();
        assert!(
            lower.contains("input") || lower.contains("approval"),
            "action line must mention input or approval; got {joined:?}"
        );
        let full = mem.recorded_with_keyboards();
        let action_row = full.iter().find(|(_, _, kb)| !kb.is_empty()).expect(
            "document-approval ModeChanged must include an inline keyboard on the action message",
        );
        let labels: Vec<&str> = action_row
            .2
            .iter()
            .flat_map(|row| row.iter().map(|(label, _)| label.as_str()))
            .collect();
        for required in ["Approve", "Reject", "Refine"] {
            assert!(
                labels.iter().any(|l| *l == required),
                "keyboard must include {required}; got {labels:?}"
            );
        }
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
        let after_first = mem.len();
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        assert_eq!(
            mem.len(),
            after_first,
            "duplicate identical elicitation ModeChanged must not send additional Telegram messages"
        );
    }

    #[tokio::test]
    async fn mode_changed_select_sends_question_and_option_keyboard() {
        use tddy_service::gen::app_mode_proto::Variant;
        use tddy_service::gen::server_message::Event;
        use tddy_service::gen::{
            AppModeProto, AppModeSelect, ClarificationQuestionProto, ModeChanged,
            QuestionOptionProto, ServerMessage,
        };

        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let msg = ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(Variant::Select(AppModeSelect {
                        question: Some(ClarificationQuestionProto {
                            header: "Clarify".into(),
                            question: "Pick one".into(),
                            options: vec![
                                QuestionOptionProto {
                                    label: "A".into(),
                                    description: String::new(),
                                },
                                QuestionOptionProto {
                                    label: "B".into(),
                                    description: String::new(),
                                },
                            ],
                            multi_select: false,
                        }),
                        question_index: 0,
                        total_questions: 1,
                        initial_selected: 0,
                    })),
                }),
            })),
        };
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        let recorded = mem.recorded_with_keyboards();
        let joined = recorded
            .iter()
            .map(|(_, t, _)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("Pick one"),
            "question text must be sent; got {joined:?}"
        );
        assert!(
            joined.contains("1. A") && joined.contains("2. B"),
            "extended option listing (one line per option); got {joined:?}"
        );
        let with_kb = recorded.iter().find(|(_, _, kb)| !kb.is_empty());
        assert!(with_kb.is_some(), "select mode must include a keyboard");
        let labels: Vec<&str> = with_kb
            .unwrap()
            .2
            .iter()
            .flat_map(|r| r.iter().map(|(l, _)| l.as_str()))
            .collect();
        assert!(
            labels.contains(&"1"),
            "compact numeric button; got {labels:?}"
        );
        assert!(
            labels.contains(&"2"),
            "compact numeric button; got {labels:?}"
        );
    }

    #[tokio::test]
    async fn mode_changed_select_populates_shared_elicitation_cache() {
        use tddy_service::gen::app_mode_proto::Variant;
        use tddy_service::gen::server_message::Event;
        use tddy_service::gen::{
            AppModeProto, AppModeSelect, ClarificationQuestionProto, ModeChanged,
            QuestionOptionProto, ServerMessage,
        };

        let cache: ElicitationSelectOptionsCache = Arc::new(StdMutex::new(HashMap::new()));
        let mut watcher = TelegramSessionWatcher::with_elicitation_select_options(cache.clone());
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018f1234-5678-7abc-8def-123456789abc";
        let msg = ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(Variant::Select(AppModeSelect {
                        question: Some(ClarificationQuestionProto {
                            header: "Clarify".into(),
                            question: "Pick one".into(),
                            options: vec![
                                QuestionOptionProto {
                                    label: "Alpha choice".into(),
                                    description: "detail a".into(),
                                },
                                QuestionOptionProto {
                                    label: "Beta".into(),
                                    description: String::new(),
                                },
                            ],
                            multi_select: false,
                        }),
                        question_index: 0,
                        total_questions: 1,
                        initial_selected: 0,
                    })),
                }),
            })),
        };
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        let labels = cache
            .lock()
            .unwrap()
            .get(sid)
            .cloned()
            .expect("cache entry for session");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], "Alpha choice\ndetail a");
        assert_eq!(labels[1], "Beta");
    }

    #[tokio::test]
    async fn document_review_telegram_includes_full_multiline_presenter_body() {
        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018fbbbb-2222-7abc-8def-123456789abc";
        let body = "PRD_SECTION_A\nPRD_SECTION_B\nPRD_SECTION_C";
        let msg =
            tddy_service::convert::session_document_approval_to_server_message(body.to_string());
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        let joined = mem
            .recorded()
            .into_iter()
            .map(|(_, t)| t)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("PRD_SECTION_A\nPRD_SECTION_B\nPRD_SECTION_C"),
            "full presenter document must appear in Telegram text; got {joined:?}"
        );
    }

    #[tokio::test]
    async fn markdown_viewer_telegram_includes_full_presenter_body() {
        use tddy_service::gen::app_mode_proto;
        use tddy_service::gen::server_message::Event;
        use tddy_service::gen::{AppModeMarkdownViewer, AppModeProto, ModeChanged, ServerMessage};

        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018fcccc-3333-7abc-8def-123456789abc";
        let body = "## Plan\n\n- step one\n- step two";
        let msg = ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(app_mode_proto::Variant::MarkdownViewer(
                        AppModeMarkdownViewer {
                            content: body.to_string(),
                        },
                    )),
                }),
            })),
        };
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        let joined = mem
            .recorded()
            .into_iter()
            .map(|(_, t)| t)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("## Plan"),
            "markdown viewer body must be sent to Telegram; got {joined:?}"
        );
        assert!(
            joined.contains("- step two"),
            "markdown viewer body must be complete; got {joined:?}"
        );
    }

    #[tokio::test]
    async fn document_review_splits_body_exceeding_telegram_message_limit() {
        let mut watcher = TelegramSessionWatcher::new();
        let mut cfg = DaemonConfig::default();
        cfg.telegram = Some(crate::config::TelegramConfig {
            enabled: true,
            bot_token: "x".to_string(),
            chat_ids: vec![42],
        });
        let mem = InMemoryTelegramSender::new();
        let sid = "018fdddd-4444-7abc-8def-123456789abc";
        let body = "Z".repeat(5000);
        let msg = tddy_service::convert::session_document_approval_to_server_message(body.clone());
        watcher
            .on_server_message(&cfg, &mem, sid, &msg)
            .await
            .unwrap();
        assert!(
            mem.len() >= 3,
            "oversized document must be split across multiple Telegram messages before the action keyboard; got {} message(s)",
            mem.len()
        );
        let plain_count = mem
            .recorded_with_keyboards()
            .iter()
            .filter(|(_, _, kb)| kb.is_empty())
            .count();
        assert!(
            plain_count >= 2,
            "oversized body must use at least two plain text sends; got {plain_count}"
        );
        let joined: String = mem.recorded().into_iter().map(|(_, t)| t).collect();
        assert_eq!(joined.matches('Z').count(), 5000);
    }
}
