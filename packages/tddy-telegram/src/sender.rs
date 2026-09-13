//! The Telegram transport: the [`TelegramSender`] port, its teloxide and in-memory
//! implementations, and the daemon's startup/shutdown announcement.
//!
//! Split out of `tddy_daemon::telegram_notifier` at the seam between **transport** and **policy**.
//! Everything here knows how to put bytes on the Telegram Bot API and nothing about when to; the
//! `TelegramSessionWatcher` left behind knows when and nothing about how. That is why this half
//! could leave `tddy-daemon` while the other half could not — see the crate docs.
//!
//! Moved verbatim. The `log` targets still read `tddy_daemon::telegram` on purpose: an
//! operator-facing log filter is an observable interface, and renaming it would be a behaviour
//! change rather than a relocation.

use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::*;
use teloxide::requests::Requester;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup};

use tddy_daemon_kernel::config::DaemonConfig;

/// Send a plain-text Telegram message using teloxide (production path; tests use [`TelegramSender`]).
pub async fn send_telegram_via_teloxide(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
) -> anyhow::Result<()> {
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
