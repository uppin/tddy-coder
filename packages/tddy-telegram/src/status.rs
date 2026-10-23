//! Telegram on the daemon's integration status board.
//!
//! PRD `docs/ft/daemon/daemon-settings.md` § Integration status. The entry
//! is [`TELEGRAM_INTEGRATION_ID`]; what it says comes from three places:
//!
//! - the `telegram:` block, read once at wiring time — [`configured_state`];
//! - the bot's identity check before the dispatcher starts — [`report_bot_identity`];
//! - every send the daemon makes afterwards — [`ReportingSender`].
//!
//! **No error written to the board carries the bot token.** Telegram's API URL embeds it
//! (`https://api.telegram.org/bot<token>/…`), and an HTTP client's error text quotes that URL, so a
//! failure copied verbatim would hand the token to anyone who can open the settings screen. Every
//! error passes through [`redact_bot_token`] first.

use async_trait::async_trait;
use tddy_daemon_extensions::{IntegrationReporter, IntegrationState};
use tddy_daemon_kernel::config::TelegramConfig;
use teloxide::prelude::{Bot, Requester};

use crate::sender::{InlineKeyboardRows, TelegramSender};

/// Telegram's id on the integration status board.
pub const TELEGRAM_INTEGRATION_ID: &str = "telegram";

/// What stands in for the bot token wherever an error text would have carried it.
const REDACTED_TOKEN: &str = "<bot-token>";

/// The state the `telegram:` block alone implies, before the bot has said anything.
///
/// No block, or `enabled: false`, is [`IntegrationState::Disabled`]. Enabled with an empty
/// `bot_token` is [`IntegrationState::Misconfigured`] — the daemon starts no bot for it, and this is
/// where an operator is told why. Anything else is [`IntegrationState::Connecting`] until the bot's
/// identity check or a send says otherwise.
pub fn configured_state(telegram: Option<&TelegramConfig>) -> IntegrationState {
    match telegram {
        Some(telegram) if telegram.enabled => {
            if telegram.bot_token.is_empty() {
                IntegrationState::Misconfigured {
                    missing: vec!["bot_token".to_string()],
                }
            } else {
                IntegrationState::Connecting
            }
        }
        _ => IntegrationState::Disabled,
    }
}

/// `text` with every occurrence of `bot_token` replaced — as written, and percent-encoded the way a
/// URL path carries its `:`. An empty token redacts nothing: there is nothing to leak.
pub fn redact_bot_token(text: &str, bot_token: &str) -> String {
    if bot_token.is_empty() {
        return text.to_string();
    }
    text.replace(bot_token, REDACTED_TOKEN)
        .replace(&bot_token.replace(':', "%3A"), REDACTED_TOKEN)
}

/// `error`'s full chain as one line, token-free — what the board and the logs are given.
fn token_free_text(error: &anyhow::Error, bot_token: &str) -> String {
    redact_bot_token(&format!("{error:#}"), bot_token)
}

/// A [`TelegramSender`] that reports the outcome of every send on Telegram's board entry.
///
/// A delivered message is [`IntegrationState::Connected`]; a failed one is
/// [`IntegrationState::Unreachable`] with the error. The caller still receives the failure — with
/// the token redacted from it as well, since every caller logs what it gets back.
pub struct ReportingSender<S> {
    inner: S,
    reporter: IntegrationReporter,
    bot_token: String,
}

impl<S: TelegramSender> ReportingSender<S> {
    /// Wrap `inner`, reporting through `reporter`; `bot_token` is what every error is redacted of.
    pub fn new(inner: S, reporter: IntegrationReporter, bot_token: &str) -> Self {
        Self {
            inner,
            reporter,
            bot_token: bot_token.to_string(),
        }
    }

    /// Report `outcome` and hand it back, token-free.
    fn reported(&self, outcome: anyhow::Result<()>) -> anyhow::Result<()> {
        match outcome {
            Ok(()) => {
                self.reporter.report(IntegrationState::Connected);
                Ok(())
            }
            Err(error) => {
                let error = token_free_text(&error, &self.bot_token);
                self.reporter.report(IntegrationState::Unreachable {
                    error: error.clone(),
                });
                Err(anyhow::anyhow!(error))
            }
        }
    }
}

#[async_trait]
impl<S: TelegramSender> TelegramSender for ReportingSender<S> {
    async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        let outcome = self.inner.send_message(chat_id, text).await;
        self.reported(outcome)
    }

    async fn send_message_with_keyboard(
        &self,
        chat_id: i64,
        text: &str,
        inline_keyboard: InlineKeyboardRows,
    ) -> anyhow::Result<()> {
        let outcome = self
            .inner
            .send_message_with_keyboard(chat_id, text, inline_keyboard)
            .await;
        self.reported(outcome)
    }
}

/// Asks Telegram who the bot is — the check that tells a working token from a revoked one before
/// any message is due.
#[async_trait]
pub trait BotIdentity: Send + Sync {
    async fn check_identity(&self) -> anyhow::Result<()>;
}

/// [`BotIdentity`] over the Bot API's `getMe`.
pub struct TeloxideBotIdentity {
    bot: Bot,
}

impl TeloxideBotIdentity {
    pub fn new(bot: Bot) -> Self {
        Self { bot }
    }
}

#[async_trait]
impl BotIdentity for TeloxideBotIdentity {
    async fn check_identity(&self) -> anyhow::Result<()> {
        self.bot
            .get_me()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("telegram getMe failed: {e}"))
    }
}

/// Check the bot's identity and report the answer: [`IntegrationState::Connected`] when Telegram
/// knows the bot, [`IntegrationState::Unreachable`] with the token-free error when it does not.
pub async fn report_bot_identity(
    identity: &dyn BotIdentity,
    reporter: &IntegrationReporter,
    bot_token: &str,
) {
    match identity.check_identity().await {
        Ok(()) => reporter.report(IntegrationState::Connected),
        Err(error) => {
            let error = token_free_text(&error, bot_token);
            log::warn!(
                target: "tddy_daemon::telegram",
                "telegram bot identity check failed: {error}"
            );
            reporter.report(IntegrationState::Unreachable { error });
        }
    }
}
