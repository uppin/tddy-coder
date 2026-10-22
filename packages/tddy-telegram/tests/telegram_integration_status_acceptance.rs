//! Telegram on the integration status board: what the daemon's `telegram:` block and the bot's
//! own traffic say about it, as `GetIntegrationStatus` reports it.
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md`, change 4. Every
//! failure path is also checked for the bot token: Telegram's API URL embeds it, so an error text
//! carried verbatim onto the board would hand it to anyone who can open the settings screen.

use std::sync::Arc;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_daemon_extensions::{IntegrationReporter, IntegrationState, IntegrationStatusBoard};
use tddy_daemon_kernel::config::TelegramConfig;
use tddy_telegram::status::{
    configured_state, report_bot_identity, BotIdentity, ReportingSender, TELEGRAM_INTEGRATION_ID,
};
use tddy_telegram::{InMemoryTelegramSender, InlineKeyboardRows, TelegramSender};

const BOT_TOKEN: &str = "123456789:AAF-secret-bot-token-value";
const CHAT_ID: i64 = 424242;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn a_telegram_block() -> TelegramConfig {
    TelegramConfig {
        enabled: true,
        bot_token: BOT_TOKEN.to_string(),
        chat_ids: vec![CHAT_ID],
    }
}

fn a_switched_off_telegram_block() -> TelegramConfig {
    TelegramConfig {
        enabled: false,
        ..a_telegram_block()
    }
}

fn a_telegram_block_without_a_token() -> TelegramConfig {
    TelegramConfig {
        bot_token: String::new(),
        ..a_telegram_block()
    }
}

/// The error a teloxide request fails with when the network is down: the request URL, token
/// included, is part of its text.
fn a_network_error() -> anyhow::Error {
    anyhow::anyhow!(
        "error sending request for url (https://api.telegram.org/bot{BOT_TOKEN}/sendMessage): \
         connection refused"
    )
}

/// A sender whose every request fails as teloxide's does when the API cannot be reached.
struct AnUnreachableApi;

#[async_trait]
impl TelegramSender for AnUnreachableApi {
    async fn send_message(&self, _chat_id: i64, _text: &str) -> anyhow::Result<()> {
        Err(a_network_error())
    }

    async fn send_message_with_keyboard(
        &self,
        _chat_id: i64,
        _text: &str,
        _inline_keyboard: InlineKeyboardRows,
    ) -> anyhow::Result<()> {
        Err(a_network_error())
    }
}

/// A bot whose `getMe` answers.
struct ABotThatAnswers;

#[async_trait]
impl BotIdentity for ABotThatAnswers {
    async fn check_identity(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A bot whose `getMe` fails, as it does for a revoked token.
struct ABotWithARevokedToken;

#[async_trait]
impl BotIdentity for ABotWithARevokedToken {
    async fn check_identity(&self) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "Unauthorized calling https://api.telegram.org/bot{BOT_TOKEN}/getMe"
        ))
    }
}

/// A board with Telegram's entry registered, and the reporter that updates it.
fn a_board_with_telegram() -> (Arc<IntegrationStatusBoard>, IntegrationReporter) {
    let board = IntegrationStatusBoard::new();
    let reporter = board.reporter(TELEGRAM_INTEGRATION_ID, "Telegram");
    (board, reporter)
}

fn telegram_state_on(board: &IntegrationStatusBoard) -> IntegrationState {
    board
        .snapshot()
        .into_iter()
        .find(|entry| entry.id == TELEGRAM_INTEGRATION_ID)
        .expect("Telegram's entry is on the board")
        .state
}

fn unreachable_error_of(state: IntegrationState) -> String {
    match state {
        IntegrationState::Unreachable { error } => error,
        other => panic!("expected Telegram to be unreachable, it is {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// What the `telegram:` block says
// ---------------------------------------------------------------------------

#[test]
fn a_daemon_without_a_telegram_block_reports_telegram_disabled() {
    // When
    let state = configured_state(None);

    // Then
    assert_eq!(state, IntegrationState::Disabled);
}

#[test]
fn a_switched_off_telegram_reports_disabled() {
    // When
    let state = configured_state(Some(&a_switched_off_telegram_block()));

    // Then
    assert_eq!(state, IntegrationState::Disabled);
}

#[test]
fn an_enabled_telegram_without_a_bot_token_reports_the_missing_token() {
    // When
    let state = configured_state(Some(&a_telegram_block_without_a_token()));

    // Then
    assert_eq!(
        state,
        IntegrationState::Misconfigured {
            missing: vec!["bot_token".to_string()]
        }
    );
}

#[test]
fn a_configured_telegram_reports_connecting_until_the_bot_is_checked() {
    // When
    let state = configured_state(Some(&a_telegram_block()));

    // Then
    assert_eq!(state, IntegrationState::Connecting);
}

// ---------------------------------------------------------------------------
// What the bot's traffic says
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_successful_send_reports_connected() {
    // Given
    let (board, reporter) = a_board_with_telegram();
    let sender = ReportingSender::new(InMemoryTelegramSender::new(), reporter, BOT_TOKEN);

    // When
    sender
        .send_message(CHAT_ID, "tddy-daemon started (test-instance)")
        .await
        .expect("the in-memory sender delivers");

    // Then
    assert_eq!(telegram_state_on(&board), IntegrationState::Connected);
}

#[tokio::test]
async fn a_failed_send_reports_unreachable_with_the_error() {
    // Given
    let (board, reporter) = a_board_with_telegram();
    let sender = ReportingSender::new(AnUnreachableApi, reporter, BOT_TOKEN);

    // When
    let outcome = sender
        .send_message(CHAT_ID, "tddy-daemon started (test-instance)")
        .await;

    // Then — the caller still sees the failure, and the board says why
    assert!(
        outcome.is_err(),
        "a failed send is not reported as delivered"
    );
    let error = unreachable_error_of(telegram_state_on(&board));
    assert!(
        error.contains("connection refused"),
        "the board names the cause: {error:?}"
    );
}

#[tokio::test]
async fn a_failed_send_never_reports_the_bot_token() {
    // Given
    let (board, reporter) = a_board_with_telegram();
    let sender = ReportingSender::new(AnUnreachableApi, reporter, BOT_TOKEN);

    // When
    let _ = sender
        .send_message_with_keyboard(CHAT_ID, "Session my-feature: approve?", Vec::new())
        .await;

    // Then
    let error = unreachable_error_of(telegram_state_on(&board));
    assert!(
        !error.contains(BOT_TOKEN),
        "the bot token leaked onto the board: {error:?}"
    );
}

#[tokio::test]
async fn a_bot_whose_identity_checks_out_reports_connected() {
    // Given
    let (board, reporter) = a_board_with_telegram();
    reporter.report(configured_state(Some(&a_telegram_block())));

    // When
    report_bot_identity(&ABotThatAnswers, &reporter, BOT_TOKEN).await;

    // Then
    assert_eq!(telegram_state_on(&board), IntegrationState::Connected);
}

#[tokio::test]
async fn a_bot_whose_identity_check_fails_reports_unreachable_without_the_token() {
    // Given
    let (board, reporter) = a_board_with_telegram();
    reporter.report(configured_state(Some(&a_telegram_block())));

    // When
    report_bot_identity(&ABotWithARevokedToken, &reporter, BOT_TOKEN).await;

    // Then
    let error = unreachable_error_of(telegram_state_on(&board));
    assert!(
        error.contains("Unauthorized") && !error.contains(BOT_TOKEN),
        "the board names the cause and never the token: {error:?}"
    );
}
